use std::{
    collections::HashMap,
    io::{stdin, stdout, Write},
};

use anyhow::{bail, Result};
use itertools::Itertools;

use crate::aws_profile::{AwsFile, ProfileName};

#[derive(Debug)]
pub struct AwsLockGuard<'a> {
    target_profiles: &'a [ProfileName],
}

impl<'a> AwsLockGuard<'a> {
    pub fn unlock(
        target_profiles: &'a [ProfileName],
        error_if_not_exist: bool,
        warn_on_production: bool,
    ) -> Result<Self> {
        modify_lock_status(
            target_profiles,
            error_if_not_exist,
            warn_on_production,
            false,
        )?;
        Ok(Self { target_profiles })
    }

    pub fn lock(self) {
        // Drop re-locks profiles. No need to do anything.
    }
}

impl Drop for AwsLockGuard<'_> {
    fn drop(&mut self) {
        let _ = modify_lock_status(self.target_profiles, false, false, true);
    }
}

pub fn check_current_lock_status(
    target_profiles: &[ProfileName],
) -> Result<(Vec<ProfileName>, Vec<ProfileName>)> {
    let mut locked_profiles = vec![];
    let mut unlocked_profiles = vec![];

    let mut aws_file = AwsFile::open()?;
    let profiles = aws_file.parse()?;

    // check all target profiles exist
    let mut unknown_profiles = vec![];
    for profile in target_profiles {
        if !profiles.iter().any(|p| p.name == *profile) {
            unknown_profiles.push(profile.clone());
        }
    }
    if !unknown_profiles.is_empty() {
        bail!(
            "some target profiles not found: {}",
            unknown_profiles
                .iter()
                .map(|s| format!("'{s}'"))
                .format(", ")
        )
    }

    for profile in profiles {
        if target_profiles.contains(&profile.name) {
            if profile.is_locked {
                locked_profiles.push(profile.name)
            } else {
                unlocked_profiles.push(profile.name)
            }
        }
    }

    Ok((locked_profiles, unlocked_profiles))
}

fn modify_lock_status(
    target_profiles: &[ProfileName],
    error_if_not_exist: bool,
    warn_on_production: bool,
    lock: bool,
) -> Result<()> {
    let mut aws_file = AwsFile::open()?;

    let mut profiles = aws_file.parse()?;
    let profile_indices: HashMap<_, _> = profiles
        .iter()
        .enumerate()
        .map(|(index, profile)| (profile.name.clone(), index))
        .collect();

    if error_if_not_exist {
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
                    .map(|s| format!("'{s}'"))
                    .format(", ")
            );
        }
    }

    if warn_on_production {
        // Warn if target profile contains production profile
        let production_profiles: Vec<_> = target_profiles
            .iter()
            .filter(|name| profile_indices.contains_key(name))
            .filter(|name| profiles[profile_indices[name]].is_production)
            .collect();

        if !production_profiles.is_empty() {
            print!(
                "You are unlocking production profiles: {}. Are you sure? (y/N) ",
                production_profiles
                    .into_iter()
                    .map(|s| format!("'{s}'"))
                    .format(", ")
            );
            stdout().flush()?;
            let mut buf = String::new();
            stdin().read_line(&mut buf)?;
            if !["y", "Y"].contains(&buf.trim()) {
                bail!("Unlocking production profiles cancelled by user");
            }
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
