use anyhow::{bail, Result};
use aws_unlock::{
    aws_lock::AwsLockGuard,
    aws_profile::{AwsFile, ProfileName},
    timer::ObservableTimer,
};
use clap::{CommandFactory, Parser};
use itertools::Itertools;
use std::{
    io::{stdout, Write},
    process::ExitCode,
    time::Duration,
};
use tokio::process::Command;

#[derive(clap::Parser)]
struct Args {
    #[clap(long, default_value_t = false)]
    silent: bool,

    #[clap(long, default_value_t = false)]
    lock_all: bool,

    #[clap(short, long, default_value_t = 60)]
    seconds: u64,

    target_profiles: Vec<String>,

    #[clap(last(true))]
    commands: Vec<String>,
}

macro_rules! may_print {
    ($silent:expr) => {
        if !$silent {
            print!();
        }
    };
    ($silent:expr, $($args:tt)*) => {
        if !$silent {
            print!($($args)*);
            stdout().flush().expect("failed to flush stdout");
        }
    };
}

macro_rules! may_println {
    ($silent:expr) => {
        if !$silent {
            println!();
        }
    };
    ($silent:expr, $($args:tt)*) => {
        if !$silent {
            println!($($args)*);
        }
    };
}

#[tokio::main]
async fn main() -> Result<ExitCode> {
    let args = Args::parse();

    if args.lock_all {
        lock_all()?;
        return Ok(ExitCode::SUCCESS);
    }

    if args.target_profiles.is_empty() {
        Args::command().print_long_help()?;
        bail!("no target profiles are specified.");
    }

    let is_silent = args.silent;

    let target_profiles: Vec<_> = args.target_profiles.into_iter().map(Into::into).collect();
    let (locked_profiles, unlocked_profiles) = check_current_lock_status(&target_profiles)?;
    if !unlocked_profiles.is_empty() {
        let unlocked_profiles = unlocked_profiles
            .iter()
            .map(|s| format!("'{s}'"))
            .format(", ");
        bail!("profile {unlocked_profiles} is not locked");
    }

    if args.commands.is_empty() {
        unlock_during_specified_duration(
            is_silent,
            &locked_profiles,
            Duration::from_secs(args.seconds),
        )
        .await?;

        Ok(ExitCode::SUCCESS)
    } else {
        unlock_during_commands(is_silent, &locked_profiles, args.commands).await
    }
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

fn check_current_lock_status(
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
        let unknown_profiles = unknown_profiles
            .iter()
            .map(|s| format!("'{s}'"))
            .format(", ");
        bail!("some target profiles not found: {unknown_profiles}",)
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

async fn unlock_during_specified_duration(
    is_silent: bool,
    target_profiles: &[ProfileName],
    dur: Duration,
) -> Result<()> {
    // prepare timer
    let (timer, canceller) = ObservableTimer::new()?;

    ctrlc::set_handler(move || {
        may_println!(is_silent, "Ctrl+C detected. Locking soon...");
        if let Err(e) = canceller.cancel() {
            may_println!(is_silent, "cancellation failed! reason: {}", e);
        }
    })?;

    let _guard = AwsLockGuard::unlock(target_profiles, true, !is_silent)?;

    may_println!(
        is_silent,
        "unlock profiles {} for {} seconds...",
        target_profiles
            .iter()
            .map(|s| format!("'{s}'"))
            .format(", "),
        dur.as_secs(),
    );

    let res = timer
        .sleep(dur, Duration::from_millis(1000), |remaining| {
            may_print!(
                is_silent,
                "\r{} seconds remaining... ",
                remaining.as_secs_f64().ceil()
            );
        })
        .await;

    match res {
        Ok(_) => may_println!(is_silent),
        Err(_) => may_println!(is_silent, "timer cancelled"),
    }

    Ok(())
}

async fn unlock_during_commands(
    is_silent: bool,
    target_profiles: &[ProfileName],
    commands: Vec<String>,
) -> Result<ExitCode> {
    let _guard = AwsLockGuard::unlock(target_profiles, true, !is_silent)?;

    ctrlc::set_handler(move || {
        // Signaling SIGINT to all group processes is the job of the terminal. All we have to do
        // here is to wait for child process to gracefully finish.
        may_println!(
            is_silent,
            "Ctrl+C detected. Waiting for command to finish..."
        );
    })?;

    let mut child = Command::new(commands[0].as_str())
        .args(&commands[1..])
        .spawn()?;
    let status = child.wait().await?;

    Ok(ExitCode::from(status.code().map(|c| c as u8).unwrap_or(1)))
}
