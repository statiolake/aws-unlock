# aws-unlock

Unlock your AWS profile as needed.

Your AWS profiles and credentials stored in `~/.aws' are usually always available. So sometimes you can accidentally deploy your infrastructure to an unexpected environment. For example, even if you intended to deploy your new application to your development AWS environment, your terraform will happily deploy it to your production environment, just because of a single mistake in the terraform definition. A countermeasure to this error is to comment out all your credentials most of the time. Only explicitly uncomment them when you actually need them.

The `aws-unlock` tool lets you easily edit and manage your credentials. After installation, the first thing you should do is `aws-unlock --lock-all` to comment out all current credentials. Then you can unlock your credentials only when you need to. There are two ways to do this.

## Usage 1 - Unlock specific profiles for a specified period of time

You can unlock your credentials for a period of time. The following
command will unlock `example-profile` for 60 seconds:

```
aws-unlock example-profile -s 60
```

## Usage 2 - Run commands with AWS credentials in environment variables

You can run commands with AWS credentials automatically set in environment variables. By default, credentials remain locked and are only temporarily accessed to set AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY, AWS_SESSION_TOKEN, and AWS_REGION:

```
aws-unlock example-profile -- terraform plan
```

If you need the credentials to remain unlocked during command execution (e.g., for interactive authentication), use the `--keep-unlocked-during-run` flag:

```
aws-unlock example-profile --keep-unlocked-during-run -- aws sso login
```

## Install

You can install aws-unlock via cargo:

```
cargo install aws-unlock
```

Alternatively, you can download your binary from [GitHub Release](https://github.com/statiolake/aws-unlock/releases/latest) page.

## Discraimer

This tool parses and rebuilds your AWS configuration, so it sometimes corrupts
your configuration file. For example, this tool does not preserve your
comments during rebuild. Please be careful and make a backup before using this
tool.
