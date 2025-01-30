[![Contributor Covenant](https://img.shields.io/badge/Contributor%20Covenant-2.1-4baaaa.svg)](.github/CODE_OF_CONDUCT.md)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![Cubtera version release](https://github.com/cubtera/cubtera/actions/workflows/release_please.yaml/badge.svg?branch=main)](https://github.com/cubtera/cubtera/actions/workflows/release_please.yaml)
# Cubtera
## Multi-dimensional Infrastructure Manager

Cubtera is a powerful CLI tool designed for managing multi-layer and multi-dimensional infrastructure deployments. It provides a flexible framework for organizing and executing infrastructure code across different dimensions like environments, regions, and accounts.

## Installation

### Via Homebrew (MacOS and Linux)
```bash
brew tap cubtera/cubtera
brew install cubtera
```

### Manual Installation
Download the latest binary from [releases](https://github.com/cubtera/cubtera/releases) and add it to your PATH.

## Core Concepts

### Dimensions
Dimensions are logical groupings that help organize your infrastructure. Common dimension types could represent:

- **Environments** (dev, staging, prod)
- **DataBaese** (us-east-1, eu-west-1)
- **Accounts** (management, production, staging)
- **Applications** (frontend, backend, database)
- **Custom dimensions** (storage, domains, repos, etc.)

Dimensions can be hierarchical (parent-child relationships) or flat.

### Units
Units are atomic operational components that represent infrastructure tasks. A unit can be:

- Terraform/OpenTofu modules
- Bash scripts
- Helm charts
- Other IaC tools

Each unit is defined with a manifest that specifies:
- Required dimensions
- Runner type (tf, bash, etc.)
- Environment variables
- State configuration

Example manifest.toml:
```toml
dimensions = ["env", "region"]
type = "tf"
overwrite = false

[runner]
version = "1.5.0"
state_backend = "s3"

[spec.envVars.required]
AWS_ACCESS_KEY_ID = "AWS_ACCESS_KEY_ID"
AWS_SECRET_ACCESS_KEY = "AWS_SECRET_ACCESS_KEY"
```

### Features

- **Multi-dimensional Management**: Run the same IaC code with different dimension values
- **Multiple Runners**: Support for Terraform, OpenTofu, and Bash
- **State Management**: Flexible state backend configuration
- **Deployment Logging**: Track deployments with BOM (Bill of Materials)
- **Inventory API**: MongoDB-backed API for querying infrastructure state
- **Docker Support**: Container image for API service
- **CI/CD Integration**: GitHub Actions support

## Usage

### Basic Commands

1. Run a unit with specific dimensions:
```bash
cubtera run -d env:prod -d region:us-east-1 -u network -- plan
```

2. Query dimension data:
```bash
cubtera im getAll env
```

3. View deployment logs:
```bash
cubtera log get -q unit_name:network -l 10
```

### Configuration

Configure Cubtera using either:
- Environment variables (CUBTERA_*)
- Configuration file (~/.cubtera/config.toml)

Example config.toml:
```toml
org = "mycompany"
workspace_path = "~/.cubtera/workspace"
inventory_path = "~/.cubtera/inventory"

[runner.tf]
version = "1.5.0"
state_backend = "s3"

[state.s3]
bucket = "terraform-state"
region = "us-east-1"
key = "states/{{org}}/{{unit_name}}"
```

## Architecture

Cubtera consists of several core components:

1. **Runner System**: Executes infrastructure code using different runners (Terraform, Bash)
2. **Dimension Management**: Handles dimension data and relationships
3. **Inventory Management**: Stores and retrieves infrastructure state
4. **Deployment Logging**: Tracks deployment history and metadata
5. **API Server**: Provides HTTP access to inventory data

## Development

### Project Structure
```
cubtera/
├── src/
│   ├── core/           # Core functionality
│   │   ├── runner/     # Runner implementations
│   │   ├── dim/        # Dimension management
│   │   ├── dlog/       # Deployment logging
│   │   └── unit/       # Unit management
│   ├── bin/            # CLI and API binaries
│   └── utils/          # Helper functions
```

### Building
```bash
cargo build --release
```

### Testing
```bash
cargo test
```

## Contributing

1. Fork the repository
2. Create a feature branch
3. Commit your changes
4. Push to the branch
5. Create a Pull Request

## License

This project is licensed under the Apache License 2.0 - see the [LICENSE](LICENSE) file for details.