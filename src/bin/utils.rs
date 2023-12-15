//!
//! # Utils
//!
//! Simple Cli tool for generating keys to be used in the locker before deployment
//!

use std::io::{stdin, stdout, Read, Write};

use tartarus::{
    crypto::{
        aes::{generate_aes256_key, GcmAes256},
        jw::JWEncryption,
        Encryption,
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
            jwe_operation(|x| JWEncryption::new(priv_key, pub_key).encrypt(x))?;
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
            jwe_operation(|x| JWEncryption::new(priv_key, pub_key).decrypt(x))?;
        }
    }

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
        let encryted_master_key = algo.encrypt(master_key.to_vec())?;
        let hexed_master_key = hex::encode(encryted_master_key);
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
