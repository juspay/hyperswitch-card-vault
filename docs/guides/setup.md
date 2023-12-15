# Setup Guide

## Prerequisites

The locker consists of 3 main feature flags which will be enabled when building the docker image.

- `key_custodian`: This adds an extra layer of protection before booting up the actual locker, this requires two keys to be passed in order to decrypt the master key, which can be then used for the locker functions
- `middleware`: This adds a JWE + JWS Encryption middleware on top of the vault APIs, to encrypt and sign any and all communications that takes place between the tenant and the locker.
- `kms`: This is an AWS specific feature (Key Management Service) which adds another layer of encryption for the keys and sensitive data passed to the locker during boot up (environment variables)

Thus, if the locker is building normally using the `Dockerfile` included, all of the above features will be enabled.

### Preparing the required keys

- Master Key:

  To generate the master key a utiliy is bundled in the repository, you can use the following command to generate the master key.

  ```bash
  cargo run --bin utils -- master-key
  ```

  This will create the master key and the associated key_custodian keys, (if you are opting out of the `key_custodian` feature, you can pass an extra argument to only generate the master key `-w`)
  Now to fulfill the `kms` requirement you would need to kms encrypt the generated master key, but while operating the locker, the instance that it is running on must have sufficient permissions to perform kms decrypt operation. You can use the following command to kms encrypt the master key

  ```bash
  aws kms encrypt --region={{region}} --key-id={{key id}} --plaintext $(echo -n {{master key}} | base64)
  ```

  Also, make sure to pass the same key id and region in the configuration of the locker while starting it.
  (this step is necessary in case you are using the `kms` feature flag)

- JWE + JWS Keys:

  These are asymmetric key pairs which should be present with both the locker and the application that is using it (tenant). Here you need to generate 2 key pairs one for the tenant and one for the locker, the below mentioned command can be used to generate the key pairs

  ```bash
  # Generating the private keys
  openssl genrsa -out locker-private-key.pem 2048
  openssl genrsa -out tenant-private-key.pem 2048

  # Generating the public keys
  openssl rsa -in locker-private-key.pem -pubout -out locker-public-key.pem
  openssl rsa -in tenant-private-key.pem -pubout -out tenant-public-key.pem
  ```

  This step is required if you have enabled the `middleware` feature.

  The locker only requires 2 Keys

  - `locker-private-key`
  - `tenant-public-key`

    These keys need to be present in the configuration before starting the application.
    If `kms` is enabled, these keys need to be kms encrypted and then passed as configuration values. The command similar to the one used for kms encrypting the master key, replacing the master key with the actual content of the `.pem` files

- Database Password:

  Only if the `kms` feature flag is enabled the database password also needs to be encrypted.

### Providing Configuration

The configuration can be provided in two ways

- As a toml file:

  replacing the content of `config/development.toml` with your respective configuration values

- environment variables:

  The `docker-compose.yaml` file can be referred to get a more detailed view of how environment variables are to be passed. Here the hirerchy of the toml file is used, separated by `__` and prefixed with `LOCKER`.
  e.g.

  ```bash
  LOCKER__SERVER__HOST=0.0.0.0
  ```

  this is similar to:

  ```toml
  [server]
  host = "0.0.0.0"
  ```

### Setting up database

- Local Setup:

  User setup before running the diesel commands,

  ```bash
  export DB_USER="db_user"
  export DB_PASS="db_pass"
  export DB_NAME="locker"
  ```

  On Ubuntu-based systems (also applicable for Ubuntu on WSL2):

  ```bash
  sudo -u postgres psql -e -c \
   "CREATE USER $DB_USER WITH PASSWORD '$DB_PASS' SUPERUSER CREATEDB CREATEROLE INHERIT LOGIN;"
  sudo -u postgres psql -e -c \
   "CREATE DATABASE $DB_NAME;"
  ```

  On MacOS:

  ```bash
  psql -e -U postgres -c \
  "CREATE USER $DB_USER WITH PASSWORD '$DB_PASS' SUPERUSER CREATEDB CREATEROLE INHERIT LOGIN;"
  psql -e -U postgres -c \
  "CREATE DATABASE $DB_NAME"
  ```

  For local setup, you can use the diesel-cli to run the diesel migrations.
  To install the diesel cli, simply run

  ```bash
  cargo install diesel_cli --no-default-features --features "postgres"
  ```

  After installing it, run:

  ```bash
  diesel migration --database-url postgres://$DB_USER:$DB_PASS@localhost:5432/$DB_NAME run
  ```

- For any external database, you can get the migration commands by running
  ```bash
  cat migrations/2023-*/up.sql
  ```
  which can then be ran using `psql` or any other tool

## Running the Locker

There are 2 main ways of running the locker:

### Docker

To use the docker variation, primarily build the docker image, simply by running

```bash
docker build -t locker:latest .
```

After the image is built, you can create a environment file which can be passed to the docker container with the required environment variables. The environment variable file must contain the following

```bash
LOCKER__SERVER__HOST=0.0.0.0
LOCKER__SERVER__PORT=8080
LOCKER__LOG__CONSOLE__ENABLED=true
LOCKER__LOG__CONSOLE__LEVEL=DEBUG
LOCKER__LOG__CONSOLE__LOG_FORMAT=default

LOCKER__DATABASE__USERNAME=
LOCKER__DATABASE__PASSWORD=
LOCKER__DATABASE__HOST=
LOCKER__DATABASE__PORT=
LOCKER__DATABASE__DBNAME=

LOCKER__LIMIT__REQUEST_COUNT=100
LOCKER__LIMIT__DURATION=60

LOCKER__SECRETS__TENANT=
LOCKER__SECRETS__MASTER_KEY=
LOCKER__SECRETS__LOCKER_PRIVATE_KEY=
LOCKER__SECRETS__TENANT_PUBLIC_KEY=

LOCKER__KMS__KEY_ID=
LOCKER__KMS__REGION=
```

Once created, you can start the locker by running

```bash
docker run --env-file .env -d locker:latest
```

This is assuming that the environment variables mentioned above reside in `.env` file

## Cargo

If you wish to directly run the executable, the configuration can be added in the `config/development.toml` file, and then to run the application simply do:

```bash

cargo run --release --features release

```
