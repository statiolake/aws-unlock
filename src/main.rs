use anyhow::{bail, Result};
use aws_unlock::{aws_lock::AwsLockGuard, aws_profile::AwsFile};
use cancellable_timer::Timer;
use clap::{CommandFactory, Parser};
use itertools::Itertools;
use std::time::Duration;

const UNLOCK_DURATION: Duration = Duration::from_secs(60);

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

    let is_silent = args.silent;

    // Convert 'default' profile to None profile
    let target_profiles: Vec<_> = args
        .target_profiles
        .into_iter()
        .map(|name| if name == "default" { None } else { Some(name) })
        .collect();

    // prepare timer
    let (mut timer, canceller) = Timer::new2()?;

    ctrlc::set_handler(move || {
        may_println!(is_silent, "Ctrl+C detected. Locking soon...");
        if let Err(e) = canceller.cancel() {
            may_println!(is_silent, "cancellation failed! reason: {}", e);
        }
    })?;

    let _guard = AwsLockGuard::unlock(&target_profiles, true, !is_silent)?;

    may_println!(
        args.silent,
        "unlock profiles {} for {} seconds...",
        target_profiles
            .iter()
            .map(|s| format!("'{}'", s.as_deref().unwrap_or("default")))
            .format(", "),
        UNLOCK_DURATION.as_secs()
    );

    if timer.sleep(UNLOCK_DURATION).is_err() {
        may_println!(is_silent, "timer cancelled.");
    }

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
