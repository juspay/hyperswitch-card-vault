//!
//! # Utils
//!
//! Simple Cli tool for generating keys to be used in the locker before deployment
//!

use tartarus::crypto::{
    aes::{generate_aes256_key, GcmAes256},
    Encryption,
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
}

#[derive(argh::FromArgs, Debug)]
#[argh(subcommand, name = "master-key")]
/// Generate the master key and optionally the associated key custodian keys
struct MasterKey {
    /// generate master key for key custodian feature disabled
    #[argh(switch, short = 'w')]
    without_custodian: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Cli = argh::from_env();

    match args.nested {
        SubCommand::MasterKey(master_key_conf) => master_key_generator(master_key_conf)?,
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
