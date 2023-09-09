use anyhow::{bail, Result};
use aws_unlock::{aws_lock::AwsLockGuard, aws_profile::AwsFile, timer::ObservableTimer};
use clap::{CommandFactory, Parser};
use itertools::Itertools;
use std::{
    io::{stdout, Write},
    time::Duration,
};

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
async fn main() -> Result<()> {
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
    let target_profiles: Vec<_> = args.target_profiles.into_iter().map(Into::into).collect();

    // prepare timer
    let (timer, canceller) = ObservableTimer::new()?;

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
            .map(|s| format!("'{s}'"))
            .format(", "),
        args.seconds
    );

    let res = timer
        .sleep(
            Duration::from_secs(args.seconds),
            Duration::from_millis(1000),
            |remaining| {
                may_print!(
                    is_silent,
                    "\r{} seconds remaining... ",
                    remaining.as_secs_f64().ceil()
                );
            },
        )
        .await;

    match res {
        Ok(_) => may_println!(is_silent,),
        Err(_) => may_println!(is_silent, "timer cancelled"),
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
