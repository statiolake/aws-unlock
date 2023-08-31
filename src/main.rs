use anyhow::Result;
use aws_unlock::aws_profile::AwsFile;
use clap::Parser;
use std::time::Duration;

const UNLOCK_DURATION: Duration = Duration::from_secs(60);

#[derive(clap::Parser)]
struct Args {
    #[clap(long, default_value_t = false)]
    silent: bool,

    #[clap(long, default_value_t = false)]
    lock_all: bool,

    profiles: Vec<String>,
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

    let mut aws_file = AwsFile::open()?;
    let all_profiles = aws_file.parse()?;

    println!("profiles: {:#?}", all_profiles);

    Ok(())
}
