use anyhow::{anyhow, bail, Result};
use home::home_dir;
use std::{
    collections::{HashMap, HashSet},
    fmt,
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
};

use crate::{line_lexer::EntryLineLexer, line_parser::EntryLineParser};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ProfileName {
    Default,
    Named(String),
}

impl<S> From<S> for ProfileName
where
    S: AsRef<str> + Into<String>,
{
    fn from(value: S) -> Self {
        if value.as_ref() == "default" {
            ProfileName::Default
        } else {
            ProfileName::Named(value.into())
        }
    }
}

impl fmt::Display for ProfileName {
    fn fmt(&self, b: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ProfileName::Default => write!(b, "default"),
            ProfileName::Named(name) => write!(b, "{name}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AwsProfile {
    /// Comment lines in config file.
    pub config_comments: Vec<String>,

    /// Comment lines in credentials file.
    pub credentials_comments: Vec<String>,

    /// Whether this profile is for production environment or not.
    pub is_production: bool,

    /// Whether this profile is currently locked or not.
    pub is_locked: bool,

    /// The profile name. None if it is default profile.
    pub name: ProfileName,

    /// `region` in ~/.aws/config.
    pub region: Option<String>,

    /// `output` in ~/.aws/config.
    pub output: Option<String>,

    /// `aws_access_key_id` in ~/.aws/credentials.
    pub aws_access_key_id: String,

    /// `aws_secret_access_key` in ~/.aws/credentials.
    pub aws_secret_access_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct AwsConfig {
    comments: Vec<String>,
    is_production: bool,
    is_locked: bool,
    name: ProfileName,
    region: Option<String>,
    output: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct AwsCredential {
    comments: Vec<String>,
    is_production: bool,
    is_locked: bool,
    name: ProfileName,
    aws_access_key_id: String,
    aws_secret_access_key: String,
}

#[derive(Debug)]
pub struct AwsFile {
    config: File,
    credentials: File,
}

impl AwsFile {
    pub fn open() -> Result<AwsFile> {
        let home_dir = home_dir().expect("failed to locate home directory");

        let aws_home = home_dir.join(".aws");
        let config = OpenOptions::new()
            .read(true)
            .write(true)
            .open(aws_home.join("config"))?;
        let credentials = OpenOptions::new()
            .read(true)
            .write(true)
            .open(aws_home.join("credentials"))?;

        Ok(AwsFile {
            config,
            credentials,
        })
    }

    pub fn flush(&mut self) -> Result<()> {
        self.config.flush()?;
        self.credentials.flush()?;

        Ok(())
    }

    pub fn parse(&mut self) -> Result<Vec<AwsProfile>> {
        let config = self.parse_config()?;
        let config_names: Vec<_> = config.iter().map(|conf| conf.name.clone()).collect();
        let mut config: HashMap<_, _> = config
            .into_iter()
            .map(|conf| (conf.name.clone(), conf))
            .collect();

        let credentials = self.parse_credentials()?;
        let credentials_names: Vec<_> = credentials.iter().map(|cred| cred.name.clone()).collect();
        let mut credentials: HashMap<_, _> = credentials
            .into_iter()
            .map(|cred| (cred.name.clone(), cred))
            .collect();

        let mut names = vec![];
        let mut inserted = HashSet::new();
        for name in config_names.iter().chain(&credentials_names) {
            if inserted.insert(name) {
                names.push(name);
            }
        }

        names
            .into_iter()
            .map(|name| {
                let conf = config
                    .remove(name)
                    .ok_or_else(|| anyhow!("config '{name}' not found",))?;
                let cred = credentials
                    .remove(name)
                    .ok_or_else(|| anyhow!("credentials '{name}' not found",))?;

                Ok(AwsProfile {
                    config_comments: conf.comments,
                    credentials_comments: cred.comments,
                    is_production: conf.is_production || cred.is_production,
                    is_locked: conf.is_locked || cred.is_locked,
                    name: name.clone(),
                    region: conf.region,
                    output: conf.output,
                    aws_access_key_id: cred.aws_access_key_id,
                    aws_secret_access_key: cred.aws_secret_access_key,
                })
            })
            .collect()
    }

    fn parse_config(&mut self) -> Result<Vec<AwsConfig>> {
        let mut buf = String::new();
        self.config.seek(SeekFrom::Start(0))?;
        self.config.read_to_string(&mut buf)?;
        let lexer = &mut EntryLineLexer::new(&buf);
        let lines = lexer.tokenize()?;
        let entries = EntryLineParser::new(lines).parse()?;

        entries
            .into_iter()
            .map(|entry| {
                let name = if entry.header == "default" {
                    ProfileName::Default
                } else {
                    match *entry.header.splitn(2, ' ').collect::<Vec<_>>() {
                        [lit_profile, name] if lit_profile == "profile" => name.into(),
                        _ => bail!("unexpected header in your config: {:?}", entry.header),
                    }
                };
                let region = entry.values.get("region").cloned();
                let output = entry.values.get("output").cloned();
                Ok(AwsConfig {
                    comments: entry.comments,
                    is_production: entry.is_production,
                    is_locked: entry.is_locked,
                    name,
                    region,
                    output,
                })
            })
            .collect()
    }

    fn parse_credentials(&mut self) -> Result<Vec<AwsCredential>> {
        let mut buf = String::new();
        self.config.seek(SeekFrom::Start(0))?;
        self.credentials.read_to_string(&mut buf)?;
        let lexer = &mut EntryLineLexer::new(&buf);
        let lines = lexer.tokenize()?;
        let entries = EntryLineParser::new(lines).parse()?;

        entries
            .into_iter()
            .map(|entry| {
                let name = entry.header.into();
                let aws_access_key_id = entry
                    .values
                    .get("aws_access_key_id")
                    .ok_or_else(|| {
                        anyhow!("failed to find 'aws_access_key_id' in your credentials")
                    })?
                    .to_string();
                let aws_secret_access_key = entry
                    .values
                    .get("aws_secret_access_key")
                    .ok_or_else(|| {
                        anyhow!("failed to find 'aws_secret_access_key' in your credentials")
                    })?
                    .to_string();
                Ok(AwsCredential {
                    comments: entry.comments,
                    is_production: entry.is_production,
                    is_locked: entry.is_locked,
                    name,
                    aws_access_key_id,
                    aws_secret_access_key,
                })
            })
            .collect()
    }

    pub fn write(&mut self, profiles: &[AwsProfile]) -> Result<()> {
        let config: Vec<_> = profiles
            .iter()
            .map(|profile| AwsConfig {
                comments: profile.config_comments.clone(),
                is_production: profile.is_production,
                is_locked: profile.is_locked,
                name: profile.name.clone(),
                region: profile.region.clone(),
                output: profile.output.clone(),
            })
            .collect();
        let credentials: Vec<_> = profiles
            .iter()
            .map(|profile| AwsCredential {
                comments: profile.credentials_comments.clone(),
                is_production: profile.is_production,
                is_locked: profile.is_locked,
                name: profile.name.clone(),
                aws_access_key_id: profile.aws_access_key_id.clone(),
                aws_secret_access_key: profile.aws_secret_access_key.clone(),
            })
            .collect();
        self.write_config(&config)?;
        self.write_credentials(&credentials)?;

        Ok(())
    }

    fn write_config(&mut self, config: &[AwsConfig]) -> Result<()> {
        self.config.seek(SeekFrom::Start(0))?;
        self.config.set_len(0)?;

        let mut first = true;
        for conf in config {
            if !first {
                writeln!(self.config)?;
            }
            first = false;

            for comment in &conf.comments {
                writeln!(self.config, "# {}", comment)?;
            }

            if conf.is_production {
                writeln!(self.config, "# production")?;
            }

            let locked_prefix = if conf.is_locked { "# " } else { "" };

            match &conf.name {
                ProfileName::Named(name) => {
                    writeln!(self.config, "{}[profile {}]", locked_prefix, name)?
                }
                ProfileName::Default => writeln!(self.config, "{}[default]", locked_prefix)?,
            }

            if let Some(region) = &conf.region {
                writeln!(self.config, "{}region = {}", locked_prefix, region)?;
            }

            if let Some(output) = &conf.output {
                writeln!(self.config, "{}output = {}", locked_prefix, output)?;
            }
        }

        Ok(())
    }

    fn write_credentials(&mut self, credentials: &[AwsCredential]) -> Result<()> {
        self.credentials.seek(SeekFrom::Start(0))?;
        self.credentials.set_len(0)?;

        let mut first = true;
        for cred in credentials {
            if !first {
                writeln!(self.credentials)?;
            }
            first = false;

            for comment in &cred.comments {
                writeln!(self.credentials, "# {}", comment)?;
            }

            if cred.is_production {
                writeln!(self.credentials, "# production")?;
            }

            let locked_prefix = if cred.is_locked { "# " } else { "" };

            writeln!(self.credentials, "{}[{}]", locked_prefix, cred.name)?;
            writeln!(
                self.credentials,
                "{}aws_access_key_id = {}",
                locked_prefix, cred.aws_access_key_id
            )?;
            writeln!(
                self.credentials,
                "{}aws_secret_access_key = {}",
                locked_prefix, cred.aws_secret_access_key
            )?;
        }

        Ok(())
    }
}
