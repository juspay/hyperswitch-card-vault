# Setup Guide

This guide will help you set up Hyperswitch Card Vault (Tartarus) for different environments:

1. [Local Development](#1-local-development-setup): Minimal setup for development with reduced dependencies
2. [Test Environment](#2-test-environment-setup): Integration testing setup with external communication features
3. [Staging Environment](#3-staging-environment-setup): Production-like deployment using Docker

## Introduction

Hyperswitch Card Vault (Tartarus) is a secure vault service designed to store sensitive payment information such as card details while maintaining PCI DSS compliance. It uses feature flags to enable different capabilities based on deployment requirements.

## 1. Local Development Setup

This setup minimizes dependencies for rapid development and testing.

### Prerequisites

- Rust toolchain (1.75.0 or newer)
- PostgreSQL database
- Git

### Feature Flags

For local development, we'll use minimal feature flags:

```
caching
```

This enables in-memory caching for better performance while keeping the setup simple.

### Database Setup

1. Create a PostgreSQL database:

```bash
# Set environment variables
export DB_USER="db_user"
export DB_PASS="db_pass"
export DB_NAME="locker"

# On Ubuntu/Debian
sudo -u postgres psql -e -c \
  "CREATE USER $DB_USER WITH PASSWORD '$DB_PASS' SUPERUSER CREATEDB CREATEROLE INHERIT LOGIN;"
sudo -u postgres psql -e -c \
  "CREATE DATABASE $DB_NAME;"

# On MacOS
psql -e -U postgres -c \
  "CREATE USER $DB_USER WITH PASSWORD '$DB_PASS' SUPERUSER CREATEDB CREATEROLE INHERIT LOGIN;"
psql -e -U postgres -c \
  "CREATE DATABASE $DB_NAME"
```

2. Run database migrations:

```bash
# Install Diesel CLI
cargo install diesel_cli --no-default-features --features "postgres"

# Run migrations
diesel migration --database-url postgres://$DB_USER:$DB_PASS@localhost:5432/$DB_NAME run
```

### Generate Master Key

Even for local development, you'll need a master key for data encryption:

```bash
# Generate a master key without key custodian
cargo run --bin utils -- master-key -w

# Output example:
# master key: feffe9928665731c6d6a8f9467308308feffe9928665731c6d6a8f9467308308
```

This generates an AES-256 key that will be used for encryption/decryption of sensitive data in the vault.

### Configuration

Create or modify `config/development.toml`:

```toml
[log.console]
enabled = true
level = "DEBUG"
log_format = "default"

[server]
host = "127.0.0.1"
port = 3001

[database]
username = "db_user"
password = "db_pass"
host = "localhost"
port = 5432
dbname = "locker"

[cache]
tti = 7200
max_capacity = 5000

[tenant_secrets]
public = { 
  # Replace with your generated master key
  master_key = "feffe9928665731c6d6a8f9467308308feffe9928665731c6d6a8f9467308308", 
  schema = "public" 
}
```

### Running the Application

```bash
# Run with minimal features
cargo run --features "caching"
```

### Verification

1. Check the application logs for successful startup
2. Test the health endpoint:

```bash
curl http://localhost:3001/health
```

## 2. Test Environment Setup

This setup includes features needed for integration testing with external components.

### Prerequisites

- All local development prerequisites
- OpenSSL for generating keys

### Feature Flags

For the test environment, we'll add middleware and external key manager features:

```
caching middleware external_key_manager
```

### Generate Required Keys

1. Generate master key:

```bash
cargo run --bin utils -- master-key -w
# Output example: master key: feffe9928665731c6d6a8f9467308308feffe9928665731c6d6a8f9467308308
```

2. Generate JWE/JWS key pairs:

```bash
# Generate private keys
openssl genrsa -out locker-private-key.pem 2048
openssl genrsa -out tenant-private-key.pem 2048

# Generate public keys
openssl rsa -in locker-private-key.pem -pubout -out locker-public-key.pem
openssl rsa -in tenant-private-key.pem -pubout -out tenant-public-key.pem
```

### Configuration

Update `config/development.toml` with additional settings:

```toml
[log.console]
enabled = true
level = "DEBUG"
log_format = "default"

[server]
host = "127.0.0.1"
port = 3001

[database]
username = "db_user"
password = "db_pass"
host = "localhost"
port = 5432
dbname = "locker"

[cache]
tti = 7200
max_capacity = 5000

[secrets]
locker_private_key = """
-----BEGIN RSA PRIVATE KEY-----
... content of locker-private-key.pem ...
-----END RSA PRIVATE KEY-----
"""

[tenant_secrets]
public = { 
  master_key = "feffe9928665731c6d6a8f9467308308feffe9928665731c6d6a8f9467308308", 
  public_key = """
  -----BEGIN PUBLIC KEY-----
  ... content of tenant-public-key.pem ...
  -----END PUBLIC KEY-----
  """,
  schema = "public" 
}

[external_key_manager]
url = "http://localhost:5000"
```

### Running the Application

```bash
# Run with test environment features
cargo run --features "caching middleware external_key_manager"
```

### Testing External Communication

To test JWE encryption/decryption:

```bash
# Encrypt a message
cargo run --bin utils -- jwe-encrypt --priv tenant-private-key.pem --pub locker-public-key.pem < message.txt > encrypted.txt

# Decrypt the message
cargo run --bin utils -- jwe-decrypt --priv locker-private-key.pem --pub tenant-public-key.pem < encrypted.txt
```

## 3. Staging Environment Setup

This setup mirrors production using Docker with all necessary features enabled.

### Prerequisites

- Docker and Docker Compose
- Access to a PostgreSQL database server
- AWS credentials (if using KMS)

### Feature Flags

For staging deployments, we use the `release` meta-feature which enables all recommended production features:

```
release
```

This includes:
- `kms-aws` or `kms-hashicorp-vault`
- `middleware`
- `key_custodian`
- `limit`
- `caching`
- `external_key_manager_mtls`

### Generate and Prepare Keys

1. Generate master key with key custodian:

```bash
cargo run --bin utils -- master-key

# Output example:
# master key: fe37a8c0a9b3e2d1f5c6940827b5d38e
# key 1: fe37a8c0a9b3e2d1
# key 2: f5c6940827b5d38e
```

2. Generate JWE/JWS key pairs (as shown in the test setup)

3. If using AWS KMS, encrypt the sensitive values:

```bash
# Encrypt master key
aws kms encrypt --region=us-west-2 --key-id=your-key-id --plaintext $(echo -n fe37a8c0a9b3e2d1f5c6940827b5d38e | base64)

# Encrypt locker private key
aws kms encrypt --region=us-west-2 --key-id=your-key-id --plaintext "$(cat locker-private-key.pem | base64)"

# Encrypt tenant public key
aws kms encrypt --region=us-west-2 --key-id=your-key-id --plaintext "$(cat tenant-public-key.pem | base64)"

# Encrypt database password
aws kms encrypt --region=us-west-2 --key-id=your-key-id --plaintext $(echo -n your-db-password | base64)
```

### Docker Setup

1. Build the Docker image:

```bash
docker build -t hyperswitch-card-vault:staging .
```

2. Create a `.env` file for Docker:

```bash
LOCKER__SERVER__HOST=0.0.0.0
LOCKER__SERVER__PORT=8080
LOCKER__LOG__CONSOLE__ENABLED=true
LOCKER__LOG__CONSOLE__LEVEL=INFO
LOCKER__LOG__CONSOLE__LOG_FORMAT=json

LOCKER__DATABASE__USERNAME=db_user
LOCKER__DATABASE__PASSWORD=<KMS-encrypted-password>
LOCKER__DATABASE__HOST=db-host.example.com
LOCKER__DATABASE__PORT=5432
LOCKER__DATABASE__DBNAME=locker
LOCKER__DATABASE__POOL_SIZE=10

LOCKER__LIMIT__REQUEST_COUNT=100
LOCKER__LIMIT__DURATION=60

LOCKER__SECRETS__LOCKER_PRIVATE_KEY=<KMS-encrypted-locker-private-key>

LOCKER__CACHE__MAX_CAPACITY=10000
LOCKER__CACHE__TTI=7200

LOCKER__SECRETS_MANAGEMENT__SECRETS_MANAGER=aws_kms
LOCKER__SECRETS_MANAGEMENT__AWS_KMS__KEY_ID=your-key-id
LOCKER__SECRETS_MANAGEMENT__AWS_KMS__REGION=us-west-2

LOCKER__TENANT_SECRETS__HYPERSWITCH__MASTER_KEY=<KMS-encrypted-master-key>
LOCKER__TENANT_SECRETS__HYPERSWITCH__PUBLIC_KEY=<KMS-encrypted-tenant-public-key>
LOCKER__TENANT_SECRETS__HYPERSWITCH__SCHEMA=public

LOCKER__EXTERNAL_KEY_MANAGER__URL=https://key-manager.example.com
LOCKER__EXTERNAL_KEY_MANAGER__CERT=<KMS-encrypted-certificate>
LOCKER__API_CLIENT__IDENTITY=<KMS-encrypted-client-identity>
```

3. Run the Docker container:

```bash
docker run --env-file .env -p 8080:8080 -d hyperswitch-card-vault:staging
```

### Initializing Key Custodian (if enabled)

After deploying, you need to unlock the key custodian:

1. Submit key 1:

```bash
curl -X POST http://your-server:8080/custodian/key1 \
  -H "Content-Type: application/json" \
  -H "x-tenant-id: hyperswitch" \
  -d '{"key": "fe37a8c0a9b3e2d1"}'
```

2. Submit key 2:

```bash
curl -X POST http://your-server:8080/custodian/key2 \
  -H "Content-Type: application/json" \
  -H "x-tenant-id: hyperswitch" \
  -d '{"key": "f5c6940827b5d38e"}'
```

3. Decrypt the master key using both keys:

```bash
curl -X POST http://your-server:8080/custodian/decrypt \
  -H "Content-Type: application/json" \
  -H "x-tenant-id: hyperswitch"
```

### Verification

Test the health endpoints to verify the setup:

```bash
# Basic health check
curl http://your-server:8080/health

# Detailed diagnostics
curl http://your-server:8080/health/diagnostics
```

## Troubleshooting

### Common Issues

1. **Database connection errors**:
   - Verify database credentials and connectivity
   - Check if the database server allows connections from your IP

2. **Key custodian errors**:
   - Ensure you're using the correct key parts
   - Verify the tenant ID in your request headers

3. **KMS decryption failures**:
   - Check AWS credentials and permissions
   - Verify the KMS key ID and region

4. **JWE/JWS errors**:
   - Ensure the key pairs match between tenant and locker
   - Verify the format of the keys in the configuration
