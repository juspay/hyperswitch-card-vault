## Overview of Feature Flags

| Feature Flag | Description |
|-------------|-------------|
| `caching` | Enables in-memory caching for database operations to improve performance |
| `middleware` | Enables request/response middleware for JWE and JWS encryption |
| `key_custodian` | Enables the key custodian API routes for secure key management with split keys |
| `limit` | Enables rate limiting for delete operations to prevent abuse |
| `kms-aws` | Enables AWS KMS integration for key management |
| `kms-hashicorp-vault` | Enables HashiCorp Vault integration for key management |
| `external_key_manager` | Enables integration with an external key management service |
| `external_key_manager_mtls` | Enables mTLS for secure communication with the external key manager |
| `console` | Enables the tokio console for runtime monitoring and debugging |
| `release` | A meta-feature that enables a predefined set of features suitable for production |

## Detailed Feature Descriptions

### `caching`
Implements in-memory caching using the Moka library for database entities like merchants, hash tables, and fingerprints. This significantly improves performance by reducing database queries for frequently accessed data.

### `middleware`
Adds request and response processing middleware that handles JWE (JSON Web Encryption) and JWS (JSON Web Signature) for secure communication. This ensures that all API requests and responses are properly encrypted and signed.

### `key_custodian`
Enables the key custodian security model where the master encryption key is split into two parts and managed by separate custodians. This provides an additional security layer as both parts are required to decrypt the master key.

### `limit`
Implements rate limiting for delete operations to prevent abuse. This is important for protecting the service from potential denial-of-service attacks targeting the deletion endpoints.

### `kms-aws`
Enables integration with AWS Key Management Service for secure key storage and operations. This allows the service to use AWS KMS for cryptographic operations.

### `kms-hashicorp-vault`
Enables integration with HashiCorp Vault for secure key storage and management. This provides an alternative to AWS KMS for organizations using Vault.

### `external_key_manager`
Enables integration with an external key management service, allowing encryption/decryption operations to be performed by a separate service. This provides better security by isolating the key management functions.

### `external_key_manager_mtls`
Extends the `external_key_manager` feature by adding mutual TLS authentication for secure communication with the external key management service. This ensures that communications with the key manager are encrypted and authenticated.

### `console`
Enables the tokio console subscriber for monitoring and debugging the async runtime. This is useful for development and troubleshooting.

### `release`
A meta-feature that enables a predefined set of features suitable for production deployments. Currently enables: `kms-aws`, `middleware`, `key_custodian`, `limit`, `kms-hashicorp-vault`, `caching`, and `external_key_manager_mtls`.

## Environment-Specific Configurations

### Development Environment

For local development and testing, a minimal feature set is often sufficient:

```toml
[features]
default = ["caching"]
```

This enables caching for better performance while keeping the setup simple. You may also want to add `console` during development for better debugging:

```bash
cargo run --features "caching console"
```

### Testing Environment

For testing environments, you might want to enable more features to test functionality:

```bash
cargo run --features "caching middleware limit"
```

### Staging Environment

Staging should mirror production as closely as possible:

```bash
cargo build --release --features "release"
```

Or if you need a more customized setup:

```bash
cargo build --release --features "caching middleware key_custodian limit external_key_manager kms-aws"
```

### Production Environment

For production deployments, use the `release` meta-feature which enables all recommended production features:

```bash
cargo build --release --features "release"
```

If you need to customize the production configuration, you can specify the exact features needed:

```bash
cargo build --release --features "caching middleware key_custodian limit external_key_manager_mtls kms-aws"
```

## Feature Interdependencies

Some features have dependencies on others:

- `external_key_manager_mtls` requires `external_key_manager`
- `release` incorporates multiple features including `kms-aws`, `middleware`, `key_custodian`, `limit`, `kms-hashicorp-vault`, `caching`, and `external_key_manager_mtls`

## Best Practices

1. **Development**: Start with minimal features and add as needed
2. **Testing**: Test with features that will be used in production
3. **Staging/Production**: Use the `release` feature flag or a well-defined set of features that meet security requirements
4. **Key Management**: For production, always use `key_custodian` and either `kms-aws` or `kms-hashicorp-vault`
5. **Security**: In production environments, `middleware` should always be enabled to ensure secure communication

## Docker Deployment

When deploying using Docker, feature flags are specified at build time:

```dockerfile
# For a standard production build
FROM rust:slim-bookworm as builder
WORKDIR /locker
COPY . .
RUN cargo build --release --features release ${EXTRA_FEATURES}
```

You can customize the build by setting the `EXTRA_FEATURES` environment variable during the Docker build:

```bash
docker build --build-arg EXTRA_FEATURES="console" -t hyperswitch-card-vault:latest .
```

This approach allows for flexible configuration while maintaining consistency across deployments.
