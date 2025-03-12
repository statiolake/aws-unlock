# AWS-UNLOCK Development Guide

## Build Commands
```bash
cargo build                 # Build debug version
cargo build --release       # Build release version
cargo test                  # Run all tests
cargo test <test_name>      # Run a specific test
cargo run -- [args]         # Run with arguments
cargo fmt                   # Format code
cargo clippy                # Run linter
```

## Code Style Guidelines
- **Imports**: Group by std lib, external crates, then internal modules
- **Types**: Use clear type definitions with documentation comments
- **Error Handling**: Use anyhow for error handling with `?` operator
- **Naming**: snake_case for variables/functions, PascalCase for types
- **Functions**: Should do one thing well with clear parameter/return types
- **Documentation**: Add doc comments for public APIs
- **File Structure**: Each module in its own file (aws_profile.rs, aws_lock.rs)
- **CLI Interface**: Use clap crate with derive API for argument parsing
- **Error Messages**: Clear, actionable error messages with context
- **Memory Management**: Proper ownership with references and Drop trait implementations