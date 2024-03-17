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
    pub conf: AwsConfigData,
    pub cred: AwsCredentialData,
}

pub type AwsProfile = WithAwsProfileMetadata<AwsProfileData>;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct AwsConfigData {
    /// Comment lines in ~/.aws/config.
    pub comments: Vec<String>,

    /// `region` in ~/.aws/config.
    pub region: Option<String>,

    /// `output` in ~/.aws/config.
    pub output: Option<String>,
}

pub type AwsConfig = WithAwsProfileMetadata<AwsConfigData>;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AwsCredentialData {
    /// Comment lines in ~/.aws/credentials.
    pub comments: Vec<String>,

    /// `aws_access_key_id` in ~/.aws/credentials.
    pub aws_access_key_id: String,

    /// `aws_secret_access_key` in ~/.aws/credentials.
    pub aws_secret_access_key: String,

    /// `aws_session_token` in ~/.aws/credentials.
    pub aws_session_token: Option<String>,

    /// `aws_session_expiration` in ~/.aws/credentials.
    pub aws_session_expiration: Option<String>,

    /// `aws_security_token` in ~/.aws/credentials.
    pub aws_security_token: Option<String>,

    /// `region` in ~/.aws/credentials.
    pub region: Option<String>,
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
                let (conf_is_production, conf_is_locked, conf_data) = config
                    .remove(name)
                    .map(|conf| (conf.is_production, conf.is_locked, conf.data))
                    .unwrap_or_default();
                let (cred_is_production, cred_is_locked, cred_data) = credentials
                    .remove(name)
                    .map(|cred| (cred.is_production, cred.is_locked, cred.data))
                    .ok_or_else(|| anyhow!("credentials '{name}' not found",))?;

                Ok(AwsProfile {
                    is_production: conf_is_production || cred_is_production,
                    is_locked: conf_is_locked || cred_is_locked,
                    name: name.clone(),
                    data: AwsProfileData {
                        conf: conf_data,
                        cred: cred_data,
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
                    let ["profile", name] = *entry.header.splitn(2, ' ').collect::<Vec<_>>() else {
                        bail!("unexpected header in your config: {:?}", entry.header);
                    };

                    name.into()
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
                let get_required = |key| {
                    entry
                        .values
                        .get(key)
                        .ok_or_else(|| anyhow!("missing key '{key}' in '{name}' credentials"))
                };
                let get_optional = |key| entry.values.get(key);

                let aws_access_key_id = get_required("aws_access_key_id")?.clone();
                let aws_secret_access_key = get_required("aws_secret_access_key")?.clone();
                let aws_session_token = get_optional("aws_session_token").cloned();
                let aws_session_expiration = get_optional("aws_session_expiration").cloned();
                let aws_security_token = get_optional("aws_security_token").cloned();
                let region = get_optional("region").cloned();

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

            let mut write = |key: &str, value: Option<&str>| -> Result<()> {
                if let Some(value) = value {
                    writeln!(self.config, "{}{} = {}", locked_prefix, key, value)?;
                }

                Ok(())
            };

            let AwsConfigData { region, output, .. } = &conf.data;
            write("region", region.as_deref())?;
            write("output", output.as_deref())?;
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

            let mut write = |key: &str, value: Option<&str>| -> Result<()> {
                if let Some(value) = value {
                    writeln!(self.credentials, "{}{} = {}", locked_prefix, key, value)?;
                }

                Ok(())
            };

            let AwsCredentialData {
                aws_access_key_id,
                aws_secret_access_key,
                aws_session_token,
                aws_session_expiration,
                aws_security_token,
                region,
                ..
            } = &cred.data;

            write("aws_access_key_id", Some(aws_access_key_id))?;
            write("aws_secret_access_key", Some(aws_secret_access_key))?;
            write("aws_session_token", aws_session_token.as_deref())?;
            write("aws_session_expiration", aws_session_expiration.as_deref())?;
            write("aws_security_token", aws_security_token.as_deref())?;
            write("region", region.as_deref())?;
        }

        Ok(())
    }
}
