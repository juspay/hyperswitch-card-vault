//!
//! # Utils
//!
//! Simple Cli tool for generating keys to be used in the locker before deployment
//!

use std::io::{Read, Write, stdin, stdout};
#[cfg(feature = "kms-aws")]
use std::path::PathBuf;

use josekit::jwe;
use tartarus::{
    crypto::encryption_manager::{
        encryption_interface::Encryption,
        managers::{
            aes::{GcmAes256, generate_aes256_key},
            jw::JWEncryption,
        },
    },
    error,
};

#[derive(argh::FromArgs, Debug)]
/// Utilities to generate associated properties used by locker
struct Cli {
    #[argh(subcommand)]
    nested: SubCommand,
}

#[derive(argh::FromArgs, Debug)]
#[argh(subcommand)]
#[non_exhaustive]
enum SubCommand {
    MasterKey(MasterKey),
    JweEncrypt(JweE),
    JweDecrypt(JweD),
    #[cfg(feature = "kms-aws")]
    MigrateToKms(MigrateToKms),
}

#[derive(argh::FromArgs, Debug)]
#[argh(subcommand, name = "master-key")]
/// Generate the master key and optionally the associated key custodian keys
struct MasterKey {
    /// generate master key for key custodian feature disabled
    #[argh(switch, short = 'w')]
    without_custodian: bool,
}

#[derive(argh::FromArgs, Debug)]
#[argh(subcommand, name = "jwe-encrypt")]
/// Perform JWE operation
struct JweE {
    /// private key to be used to perform jwe operation
    #[argh(option, long = "priv")]
    private_key: Option<String>,
    /// public key to be used to perform jwe operation
    #[argh(option, long = "pub")]
    public_key: Option<String>,
}

#[derive(argh::FromArgs, Debug)]
#[argh(subcommand, name = "jwe-decrypt")]
/// Perform JWE operation
struct JweD {
    /// private key to be used to perform jwe operation
    #[argh(option, long = "priv")]
    private_key: Option<String>,
    /// public key to be used to perform jwe operation
    #[argh(option, long = "pub")]
    public_key: Option<String>,
}

#[cfg(feature = "kms-aws")]
#[derive(argh::FromArgs, Debug)]
#[argh(subcommand, name = "migrate-to-kms")]
/// One-time bulk migration of cardholder data from internal AES-GCM to AWS KMS.
/// Run with the service paused. Reads each row, decrypts via master_key+DEK,
/// re-encrypts via KMS with `entity_id` encryption context, writes back.
struct MigrateToKms {
    /// tenant id whose schema to migrate
    #[argh(option, long = "tenant-id")]
    tenant_id: String,
    /// path to the locker config file (TOML); defaults to the locker's lookup
    #[argh(option, long = "config-path")]
    config_path: Option<PathBuf>,
    /// rows per batch (default 500)
    #[argh(option, long = "batch-size", default = "500")]
    batch_size: i64,
    /// soft cap on KMS calls per second (default 800)
    #[argh(option, long = "rps", default = "800")]
    rps: u32,
    /// path to the checkpoint file (resume cursor)
    #[argh(option, long = "checkpoint")]
    checkpoint: PathBuf,
    /// decrypt + KMS encrypt every row but skip the UPDATE
    #[argh(switch, long = "dry-run")]
    dry_run: bool,
    /// skip migration; KMS-decrypt the most recent N rows and check hashes
    #[argh(switch, long = "verify-only")]
    verify_only: bool,
    /// row count for --verify-only sampling (default 50)
    #[argh(option, long = "verify-sample-size", default = "50")]
    verify_sample_size: i64,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Cli = argh::from_env();

    match args.nested {
        SubCommand::MasterKey(master_key_conf) => master_key_generator(master_key_conf)?,
        SubCommand::JweEncrypt(JweE {
            private_key,
            public_key,
        }) => {
            let priv_key = read_file_to_string(
                &private_key.ok_or(error::CryptoError::InvalidData("private key not found"))?,
            )?;
            let pub_key = read_file_to_string(
                &public_key.ok_or(error::CryptoError::InvalidData("public key not found"))?,
            )?;
            jwe_operation(|payload| {
                JWEncryption::new(priv_key, pub_key, jwe::RSA_OAEP_256, jwe::RSA_OAEP)
                    .encrypt(payload)
                    .and_then(|payload| {
                        Ok(serde_json::to_vec(&payload)
                            .map_err(error::CryptoError::SerdeJsonError)?)
                    })
            })?;
        }
        SubCommand::JweDecrypt(JweD {
            private_key,
            public_key,
        }) => {
            let priv_key = read_file_to_string(
                &private_key.ok_or(error::CryptoError::InvalidData("private key not found"))?,
            )?;
            let pub_key = read_file_to_string(
                &public_key.ok_or(error::CryptoError::InvalidData("private key not found"))?,
            )?;
            jwe_operation(|payload| {
                serde_json::from_slice(&payload)
                    .map_err(error::CryptoError::SerdeJsonError)
                    .map_err(Into::into)
                    .and_then(|payload| {
                        JWEncryption::new(priv_key, pub_key, jwe::RSA_OAEP_256, jwe::RSA_OAEP)
                            .decrypt(payload)
                    })
                // (x)
            })?;
        }
        #[cfg(feature = "kms-aws")]
        SubCommand::MigrateToKms(args) => {
            let runtime = tokio::runtime::Runtime::new()?;
            runtime.block_on(run_kms_migration(args))?;
        }
    }

    Ok(())
}

#[cfg(feature = "kms-aws")]
async fn run_kms_migration(args: MigrateToKms) -> Result<(), Box<dyn std::error::Error>> {
    let mut global_config = tartarus::config::GlobalConfig::new_with_config_path(args.config_path)?;
    global_config.validate()?;
    global_config.fetch_raw_secrets().await?;

    let opts = tartarus::migration::MigrationOptions {
        tenant_id: args.tenant_id,
        batch_size: args.batch_size,
        rps: args.rps,
        checkpoint_path: args.checkpoint,
        dry_run: args.dry_run,
        verify_only: args.verify_only,
        verify_sample_size: args.verify_sample_size,
    };

    tartarus::migration::run(&global_config, opts).await?;
    Ok(())
}

fn master_key_generator(master_key_conf: MasterKey) -> Result<(), Box<dyn std::error::Error>> {
    let master_key = generate_aes256_key();
    if master_key_conf.without_custodian {
        println!("master key: {}", hex::encode(master_key));
        Ok(())
    } else {
        let encryption_key = generate_aes256_key();
        let key_custodian_key = hex::encode(encryption_key);
        let algo = GcmAes256::new(encryption_key.to_vec());
        let encrypted_master_key = algo.encrypt(master_key.to_vec())?;
        let hexed_master_key = hex::encode(encrypted_master_key);
        println!("master key: {}", hexed_master_key);
        let (key1, key2) = key_custodian_key.split_at(key_custodian_key.len() / 2);
        println!("key 1: {}", key1);
        println!("key 2: {}", key2);

        Ok(())
    }
}

fn read_file_to_string(name: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut file = std::fs::File::open(name)?;
    let mut output = String::new();
    file.read_to_string(&mut output)?;
    Ok(output)
}

fn jwe_operation(
    op: impl FnOnce(Vec<u8>) -> Result<Vec<u8>, error::ContainerError<error::CryptoError>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    stdin().read_to_string(&mut input)?;

    // let output = op(input.as_bytes().to_vec())?;
    let output = op(input.trim().as_bytes().to_vec())?;

    stdout().write_all(&output)?;

    Ok(())
}
