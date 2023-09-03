use anyhow::{bail, Result};
use aws_unlock::aws_profile::AwsFile;
use clap::{CommandFactory, Parser};
use itertools::Itertools;
use std::thread::sleep;
use std::{collections::HashMap, time::Duration};

const UNLOCK_DURATION: Duration = Duration::from_secs(10);

#[derive(clap::Parser)]
struct Args {
    #[clap(long, default_value_t = false)]
    silent: bool,

    #[clap(long, default_value_t = false)]
    lock_all: bool,

    target_profiles: Vec<String>,
}

macro_rules! may_println {
    ($silent:expr, $($args:tt)*) => {
        if !$silent {
            println!($($args)*);
        }
    };
}

fn main() -> Result<()> {
    let args = Args::parse();

    if args.lock_all {
        return lock_all();
    }

    if args.target_profiles.is_empty() {
        Args::command().print_long_help()?;
        bail!("no target profiles are specified.");
    }

    // Convert 'default' profile to None profile
    let target_profiles: Vec<_> = args
        .target_profiles
        .into_iter()
        .map(|name| if name == "default" { None } else { Some(name) })
        .collect();

    modify_lock_status(&target_profiles, true, false)?;

    may_println!(
        args.silent,
        "unlock profiles {} for {} seconds...",
        target_profiles
            .iter()
            .map(|s| format!("'{}'", s.as_deref().unwrap_or("default")))
            .format(", "),
        UNLOCK_DURATION.as_secs()
    );
    sleep(UNLOCK_DURATION);

    modify_lock_status(&target_profiles, false, true)?;

    Ok(())
}

fn lock_all() -> Result<()> {
    let mut aws_file = AwsFile::open()?;
    let mut profiles = aws_file.parse()?;
    profiles
        .iter_mut()
        .for_each(|profile| profile.is_locked = true);
    aws_file.write(&profiles)?;

    Ok(())
}

fn modify_lock_status(
    target_profiles: &[Option<String>],
    error_if_not_exists: bool,
    lock: bool,
) -> Result<()> {
    let mut aws_file = AwsFile::open()?;

    let mut profiles = aws_file.parse()?;
    let profile_indices: HashMap<_, _> = profiles
        .iter()
        .enumerate()
        .map(|(index, profile)| (profile.name.clone(), index))
        .collect();

    if error_if_not_exists {
        // Check profiles exist if non-existence is explicit error
        let unknown_profiles: Vec<_> = target_profiles
            .iter()
            .filter(|name| !profile_indices.contains_key(name))
            .collect();

        if !unknown_profiles.is_empty() {
            bail!(
                "unknown profiles: {}",
                unknown_profiles
                    .into_iter()
                    .map(|s| format!("'{}'", s.as_deref().unwrap_or("default")))
                    .format(", ")
            );
        }
    }

    // Lock target profiles
    target_profiles
        .iter()
        .filter(|name| profile_indices.contains_key(name))
        .for_each(|name| profiles[profile_indices[name]].is_locked = lock);

    // Write to file
    aws_file.write(&profiles)?;
    aws_file.flush()?;

    Ok(())
}
