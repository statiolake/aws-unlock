use std::collections::HashMap;

use anyhow::{bail, Result};
use itertools::Itertools;

use crate::aws_profile::AwsFile;

#[derive(Debug)]
pub struct AwsLockGuard<'a> {
    target_profiles: &'a [Option<String>],
}

impl<'a> AwsLockGuard<'a> {
    pub fn unlock(target_profiles: &'a [Option<String>]) -> Result<Self> {
        modify_lock_status(target_profiles, true, false)?;
        Ok(Self { target_profiles })
    }

    pub fn lock(self) {
        // Drop re-locks profiles. No need to do anything.
    }
}

impl Drop for AwsLockGuard<'_> {
    fn drop(&mut self) {
        let _ = modify_lock_status(self.target_profiles, false, true);
    }
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
