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
pub struct WithAwsProfileMetadata<T> {
    pub name: ProfileName,
    pub is_production: bool,
    pub is_locked: bool,
    pub data: T,
}

impl<T> WithAwsProfileMetadata<T> {
    pub fn to_ref(&self) -> WithAwsProfileMetadata<&T> {
        WithAwsProfileMetadata {
            name: self.name.clone(),
            is_production: self.is_production,
            is_locked: self.is_locked,
            data: &self.data,
        }
    }

    pub fn map<U, F>(self, f: F) -> WithAwsProfileMetadata<U>
    where
        F: FnOnce(T) -> U,
    {
        WithAwsProfileMetadata {
            name: self.name,
            is_production: self.is_production,
            is_locked: self.is_locked,
            data: f(self.data),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AwsProfileData {
    conf: AwsConfigData,
    cred: AwsCredentialData,
}

pub type AwsProfile = WithAwsProfileMetadata<AwsProfileData>;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AwsConfigData {
    /// Comment lines in ~/.aws/config.
    comments: Vec<String>,

    /// `region` in ~/.aws/config.
    region: Option<String>,

    /// `output` in ~/.aws/config.
    output: Option<String>,
}

pub type AwsConfig = WithAwsProfileMetadata<AwsConfigData>;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AwsCredentialData {
    /// Comment lines in ~/.aws/credentials.
    comments: Vec<String>,

    /// `aws_access_key_id` in ~/.aws/credentials.
    aws_access_key_id: String,

    /// `aws_secret_access_key` in ~/.aws/credentials.
    aws_secret_access_key: String,

    /// `aws_session_token` in ~/.aws/credentials.
    aws_session_token: Option<String>,

    /// `aws_session_expiration` in ~/.aws/credentials.
    aws_session_expiration: Option<String>,

    /// `aws_security_token` in ~/.aws/credentials.
    aws_security_token: Option<String>,

    /// `region` in ~/.aws/credentials.
    region: Option<String>,
}

pub type AwsCredential = WithAwsProfileMetadata<AwsCredentialData>;

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
                    is_production: conf.is_production || cred.is_production,
                    is_locked: conf.is_locked || cred.is_locked,
                    name: name.clone(),
                    data: AwsProfileData {
                        conf: conf.data,
                        cred: cred.data,
                    },
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
                        ["profile", name] => name.into(),
                        _ => bail!("unexpected header in your config: {:?}", entry.header),
                    }
                };
                let region = entry.values.get("region").cloned();
                let output = entry.values.get("output").cloned();
                Ok(AwsConfig {
                    name,
                    is_production: entry.is_production,
                    is_locked: entry.is_locked,
                    data: AwsConfigData {
                        comments: entry.comments,
                        region,
                        output,
                    },
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
                let get_required_entry = |key| {
                    entry
                        .values
                        .get(key)
                        .ok_or_else(|| anyhow!("missing key '{key}' in '{name}' credentials",))
                        .map(|s| s.to_string())
                };
                let aws_access_key_id = get_required_entry("aws_access_key_id")?;
                let aws_secret_access_key = get_required_entry("aws_secret_access_key")?;
                let aws_session_token = entry.values.get("aws_session_token").cloned();
                let aws_session_expiration = entry.values.get("aws_session_expiration").cloned();
                let aws_security_token = entry.values.get("aws_security_token").cloned();
                let region = entry.values.get("region").cloned();
                Ok(AwsCredential {
                    name,
                    is_production: entry.is_production,
                    is_locked: entry.is_locked,
                    data: AwsCredentialData {
                        comments: entry.comments,
                        aws_access_key_id,
                        aws_secret_access_key,
                        aws_session_token,
                        aws_session_expiration,
                        aws_security_token,
                        region,
                    },
                })
            })
            .collect()
    }

    pub fn write(&mut self, profiles: &[AwsProfile]) -> Result<()> {
        let config: Vec<_> = profiles
            .iter()
            .map(|profile| profile.to_ref().map(|data| data.conf.clone()))
            .collect();
        let credentials: Vec<_> = profiles
            .iter()
            .map(|profile| profile.to_ref().map(|data| data.cred.clone()))
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

            for comment in &conf.data.comments {
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

            if let Some(region) = &conf.data.region {
                writeln!(self.config, "{}region = {}", locked_prefix, region)?;
            }

            if let Some(output) = &conf.data.output {
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

            for comment in &cred.data.comments {
                writeln!(self.credentials, "# {}", comment)?;
            }

            if cred.is_production {
                writeln!(self.credentials, "# production")?;
            }

            let locked_prefix = if cred.is_locked { "# " } else { "" };
            writeln!(self.credentials, "{}[{}]", locked_prefix, cred.name)?;

            let mut write_key_value_if_needed = |key: &str, value: Option<&str>| -> Result<()> {
                if let Some(value) = value {
                    writeln!(self.credentials, "{}{} = {}", locked_prefix, key, value)?;
                }

                Ok(())
            };

            write_key_value_if_needed("aws_access_key_id", Some(&cred.data.aws_access_key_id))?;
            write_key_value_if_needed(
                "aws_secret_access_key",
                Some(&cred.data.aws_secret_access_key),
            )?;
            write_key_value_if_needed("aws_session_token", cred.data.aws_session_token.as_deref())?;
            write_key_value_if_needed(
                "aws_session_expiration",
                cred.data.aws_session_expiration.as_deref(),
            )?;
            write_key_value_if_needed(
                "aws_security_token",
                cred.data.aws_security_token.as_deref(),
            )?;
            write_key_value_if_needed("region", cred.data.region.as_deref())?;
        }

        Ok(())
    }
}
