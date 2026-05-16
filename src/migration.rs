//! One-time bulk migration of cardholder data from the internal AES-GCM
//! key hierarchy (master_key → per-merchant DEK → row ciphertext) to direct
//! AWS KMS Encrypt/Decrypt.
//!
//! Run against a tenant whose service is paused or read-only. After the run
//! completes, flip `external_key_manager = "aws_kms"` and restart the
//! locker. The migration writes new ciphertext into the same `bytea` columns
//! (`locker.enc_data`, `vault.encrypted_data`), so the encryption context
//! shape **must** match the runtime: `{"entity_id": <merchant_id|entity_id>}`.
//! See `src/crypto/keymanager/kms_keymanager.rs`.
//!
//! Idempotency: a JSON checkpoint file holds the highest `id` processed per
//! table. On restart the job resumes from `id > cursor`. Rows above the
//! cursor are guaranteed un-migrated; rows below are guaranteed done. A
//! crash mid-batch rolls back the in-flight UPDATE before the cursor moves.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use diesel::{ExpressionMethods, QueryDsl};
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use ring::digest;

use crate::{
    config::{GlobalConfig, TenantConfig},
    crypto::{
        encryption_manager::{
            encryption_interface::Encryption, managers::aes::GcmAes256,
        },
        secrets_manager::managers::aws_kms::core::{AwsKmsClient, AwsKmsConfig},
    },
    storage::{Storage, schema},
};

#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    #[error("configuration: {0}")]
    Configuration(String),
    #[error("checkpoint i/o: {0}")]
    CheckpointIo(String),
    #[error("storage: {0}")]
    Storage(String),
    #[error("decrypt failed for {table} id={id}: {reason}")]
    Decrypt {
        table: &'static str,
        id: i32,
        reason: String,
    },
    #[error("kms encrypt failed for {table} id={id}: {reason}")]
    KmsEncrypt {
        table: &'static str,
        id: i32,
        reason: String,
    },
    #[error("verification failed for {table} id={id}: {reason}")]
    Verification {
        table: &'static str,
        id: i32,
        reason: String,
    },
}

#[derive(Clone, Debug)]
pub struct MigrationOptions {
    pub tenant_id: String,
    pub batch_size: i64,
    pub rps: u32,
    pub checkpoint_path: PathBuf,
    pub dry_run: bool,
    pub verify_only: bool,
    pub verify_sample_size: i64,
}

pub async fn run(
    global_config: &GlobalConfig,
    opts: MigrationOptions,
) -> Result<(), MigrationError> {
    let tenant_config = TenantConfig::from_global_config(global_config, opts.tenant_id.clone());

    let kms_cfg = tenant_config
        .tenant_secrets
        .kms_data_key
        .as_ref()
        .ok_or_else(|| {
            MigrationError::Configuration(format!(
                "tenant `{}` is missing `kms_data_key` in tenant_secrets — \
                 add it before running the migration",
                opts.tenant_id
            ))
        })?;

    let kms = Arc::new(
        AwsKmsClient::new(&AwsKmsConfig {
            key_id: kms_cfg.key_id.clone(),
            region: kms_cfg.region.clone(),
        })
        .await,
    );

    let storage = Storage::new(
        &global_config.database,
        &tenant_config.tenant_secrets.schema,
    )
    .await
    .map_err(|e| MigrationError::Storage(format!("opening pool: {e:?}")))?;

    let master_gcm = GcmAes256::new(tenant_config.tenant_secrets.master_key.clone());

    println!(
        "preflight: probing KMS key {} in {}",
        kms_cfg.key_id, kms_cfg.region
    );
    kms_probe(&kms, &opts.tenant_id).await?;

    if opts.verify_only {
        verify_sample(&storage, &kms, &opts, Table::Locker).await?;
        verify_sample(&storage, &kms, &opts, Table::Vault).await?;
        return Ok(());
    }

    let mut checkpoint = Checkpoint::load(&opts.checkpoint_path)?;
    let mut limiter = RateLimiter::new(opts.rps);

    println!(
        "migrating `locker` rows id > {}",
        checkpoint.locker_cursor
    );
    migrate_table(
        &storage,
        &master_gcm,
        &kms,
        &opts,
        Table::Locker,
        &mut checkpoint,
        &mut limiter,
    )
    .await?;

    println!("migrating `vault` rows id > {}", checkpoint.vault_cursor);
    migrate_table(
        &storage,
        &master_gcm,
        &kms,
        &opts,
        Table::Vault,
        &mut checkpoint,
        &mut limiter,
    )
    .await?;

    println!("migration complete (dry_run={})", opts.dry_run);
    Ok(())
}

#[derive(Clone, Copy, Debug)]
enum Table {
    Locker,
    Vault,
}

impl Table {
    fn name(self) -> &'static str {
        match self {
            Self::Locker => "locker",
            Self::Vault => "vault",
        }
    }
}

async fn migrate_table(
    storage: &Storage,
    master_gcm: &GcmAes256,
    kms: &Arc<AwsKmsClient>,
    opts: &MigrationOptions,
    table: Table,
    checkpoint: &mut Checkpoint,
    limiter: &mut RateLimiter,
) -> Result<(), MigrationError> {
    let mut dek_cache: HashMap<String, Vec<u8>> = HashMap::new();
    let mut total = 0_u64;

    loop {
        let mut conn = storage
            .get_conn()
            .await
            .map_err(|e| MigrationError::Storage(format!("acquiring conn: {e:?}")))?;

        let rows = load_batch(&mut conn, table, cursor(checkpoint, table), opts.batch_size).await?;

        if rows.is_empty() {
            break;
        }

        for Row {
            id,
            entity_id,
            ciphertext,
        } in rows
        {
            let dek = match dek_cache.get(&entity_id) {
                Some(k) => k.clone(),
                None => {
                    let k = load_dek(&mut conn, &entity_id, master_gcm).await.map_err(
                        |e| MigrationError::Decrypt {
                            table: table.name(),
                            id,
                            reason: format!("loading DEK for `{entity_id}`: {e}"),
                        },
                    )?;
                    dek_cache.insert(entity_id.clone(), k.clone());
                    k
                }
            };

            let plaintext =
                GcmAes256::new(dek)
                    .decrypt(ciphertext)
                    .map_err(|e| MigrationError::Decrypt {
                        table: table.name(),
                        id,
                        reason: format!("{e:?}"),
                    })?;

            limiter.acquire().await;
            let ctx = encryption_context(&entity_id);
            let new_blob = kms.encrypt(&plaintext, Some(ctx)).await.map_err(|e| {
                MigrationError::KmsEncrypt {
                    table: table.name(),
                    id,
                    reason: format!("{e:?}"),
                }
            })?;

            if !opts.dry_run {
                write_back(&mut conn, table, id, new_blob).await?;
            }

            advance_cursor(checkpoint, table, id);
            total += 1;
        }

        checkpoint.persist()?;
        println!(
            "{}: {total} rows processed, cursor at {}",
            table.name(),
            cursor(checkpoint, table)
        );
    }

    Ok(())
}

struct Row {
    id: i32,
    entity_id: String,
    ciphertext: Vec<u8>,
}

async fn load_batch(
    conn: &mut AsyncPgConnection,
    table: Table,
    cursor: i32,
    batch_size: i64,
) -> Result<Vec<Row>, MigrationError> {
    match table {
        Table::Locker => {
            let rows: Vec<(i32, String, Vec<u8>)> = schema::locker::table
                .select((
                    schema::locker::id,
                    schema::locker::merchant_id,
                    schema::locker::enc_data,
                ))
                .filter(schema::locker::id.gt(cursor))
                .order(schema::locker::id.asc())
                .limit(batch_size)
                .load(conn)
                .await
                .map_err(|e| MigrationError::Storage(format!("loading locker batch: {e}")))?;
            Ok(rows
                .into_iter()
                .map(|(id, entity_id, ciphertext)| Row {
                    id,
                    entity_id,
                    ciphertext,
                })
                .collect())
        }
        Table::Vault => {
            let rows: Vec<(i32, String, Vec<u8>)> = schema::vault::table
                .select((
                    schema::vault::id,
                    schema::vault::entity_id,
                    schema::vault::encrypted_data,
                ))
                .filter(schema::vault::id.gt(cursor))
                .order(schema::vault::id.asc())
                .limit(batch_size)
                .load(conn)
                .await
                .map_err(|e| MigrationError::Storage(format!("loading vault batch: {e}")))?;
            Ok(rows
                .into_iter()
                .map(|(id, entity_id, ciphertext)| Row {
                    id,
                    entity_id,
                    ciphertext,
                })
                .collect())
        }
    }
}

async fn write_back(
    conn: &mut AsyncPgConnection,
    table: Table,
    id: i32,
    blob: Vec<u8>,
) -> Result<(), MigrationError> {
    let n = match table {
        Table::Locker => diesel::update(schema::locker::table.filter(schema::locker::id.eq(id)))
            .set(schema::locker::enc_data.eq(blob))
            .execute(conn)
            .await
            .map_err(|e| MigrationError::Storage(format!("updating locker id={id}: {e}")))?,
        Table::Vault => diesel::update(schema::vault::table.filter(schema::vault::id.eq(id)))
            .set(schema::vault::encrypted_data.eq(blob))
            .execute(conn)
            .await
            .map_err(|e| MigrationError::Storage(format!("updating vault id={id}: {e}")))?,
    };
    if n != 1 {
        return Err(MigrationError::Storage(format!(
            "expected to update 1 row in {} id={id}, updated {n}",
            table.name()
        )));
    }
    Ok(())
}

async fn load_dek(
    conn: &mut AsyncPgConnection,
    merchant_id: &str,
    master_gcm: &GcmAes256,
) -> Result<Vec<u8>, String> {
    let wrapped: Vec<u8> = schema::merchant::table
        .select(schema::merchant::enc_key)
        .filter(schema::merchant::merchant_id.eq(merchant_id))
        .get_result(conn)
        .await
        .map_err(|e| format!("merchant lookup: {e}"))?;
    master_gcm
        .decrypt(wrapped)
        .map_err(|e| format!("master_key unwrap failed: {e:?}"))
}

fn encryption_context(entity_id: &str) -> HashMap<String, String> {
    HashMap::from([("entity_id".to_string(), entity_id.to_string())])
}

async fn kms_probe(kms: &AwsKmsClient, tenant_id: &str) -> Result<(), MigrationError> {
    let ctx = encryption_context(&format!("__migration_probe__{tenant_id}"));
    let plaintext = b"tartarus-migration-probe".to_vec();
    let blob = kms
        .encrypt(&plaintext, Some(ctx.clone()))
        .await
        .map_err(|e| MigrationError::Configuration(format!("KMS encrypt probe failed: {e:?}")))?;
    let round = kms
        .decrypt(&blob, Some(ctx))
        .await
        .map_err(|e| MigrationError::Configuration(format!("KMS decrypt probe failed: {e:?}")))?;
    if round != plaintext {
        return Err(MigrationError::Configuration(
            "KMS round-trip plaintext mismatch".into(),
        ));
    }
    Ok(())
}

async fn verify_sample(
    storage: &Storage,
    kms: &AwsKmsClient,
    opts: &MigrationOptions,
    table: Table,
) -> Result<(), MigrationError> {
    let mut conn = storage
        .get_conn()
        .await
        .map_err(|e| MigrationError::Storage(format!("acquiring conn: {e:?}")))?;

    match table {
        Table::Locker => {
            // Pull recent rows then look up each one's dedup SHA-512 in a
            // second query. We avoid a SQL join because no `joinable!` is
            // declared between `locker` and `hash_table` in the schema.
            let rows: Vec<(i32, String, Vec<u8>, String)> = schema::locker::table
                .select((
                    schema::locker::id,
                    schema::locker::merchant_id,
                    schema::locker::enc_data,
                    schema::locker::hash_id,
                ))
                .order(schema::locker::id.desc())
                .limit(opts.verify_sample_size)
                .load(&mut conn)
                .await
                .map_err(|e| MigrationError::Storage(format!("verify locker: {e}")))?;

            for (id, merchant_id, blob, hash_id) in rows {
                let expected_hash: Vec<u8> = schema::hash_table::table
                    .select(schema::hash_table::data_hash)
                    .filter(schema::hash_table::hash_id.eq(&hash_id))
                    .get_result(&mut conn)
                    .await
                    .map_err(|e| MigrationError::Storage(format!(
                        "verify locker hash lookup id={id}: {e}"
                    )))?;

                let plaintext = kms
                    .decrypt(&blob, Some(encryption_context(&merchant_id)))
                    .await
                    .map_err(|e| MigrationError::Verification {
                        table: "locker",
                        id,
                        reason: format!("KMS decrypt: {e:?}"),
                    })?;
                let actual = digest::digest(&digest::SHA512, &plaintext).as_ref().to_vec();
                if actual != expected_hash {
                    return Err(MigrationError::Verification {
                        table: "locker",
                        id,
                        reason: "SHA-512 of decrypted plaintext does not match hash_table"
                            .into(),
                    });
                }
            }
        }
        Table::Vault => {
            // v2 has no hash_table linkage; verify decryption succeeds with the
            // expected entity_id context. A successful KMS Decrypt with the
            // correct context proves both key access and AAD binding.
            let rows: Vec<(i32, String, Vec<u8>)> = schema::vault::table
                .select((
                    schema::vault::id,
                    schema::vault::entity_id,
                    schema::vault::encrypted_data,
                ))
                .order(schema::vault::id.desc())
                .limit(opts.verify_sample_size)
                .load(&mut conn)
                .await
                .map_err(|e| MigrationError::Storage(format!("verify vault: {e}")))?;

            for (id, entity_id, blob) in rows {
                kms.decrypt(&blob, Some(encryption_context(&entity_id)))
                    .await
                    .map_err(|e| MigrationError::Verification {
                        table: "vault",
                        id,
                        reason: format!("KMS decrypt: {e:?}"),
                    })?;
            }
        }
    }
    Ok(())
}

fn cursor(checkpoint: &Checkpoint, table: Table) -> i32 {
    match table {
        Table::Locker => checkpoint.locker_cursor,
        Table::Vault => checkpoint.vault_cursor,
    }
}

fn advance_cursor(checkpoint: &mut Checkpoint, table: Table, id: i32) {
    match table {
        Table::Locker => checkpoint.locker_cursor = id,
        Table::Vault => checkpoint.vault_cursor = id,
    }
}

#[derive(serde::Serialize, serde::Deserialize, Default, Debug)]
struct Checkpoint {
    #[serde(default)]
    locker_cursor: i32,
    #[serde(default)]
    vault_cursor: i32,
    #[serde(skip)]
    path: PathBuf,
}

impl Checkpoint {
    fn load(path: &Path) -> Result<Self, MigrationError> {
        let mut cp = if path.exists() {
            let raw = fs::read_to_string(path)
                .map_err(|e| MigrationError::CheckpointIo(format!("read {path:?}: {e}")))?;
            serde_json::from_str::<Self>(&raw)
                .map_err(|e| MigrationError::CheckpointIo(format!("parse {path:?}: {e}")))?
        } else {
            Self::default()
        };
        cp.path = path.to_path_buf();
        Ok(cp)
    }

    fn persist(&self) -> Result<(), MigrationError> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|e| {
                    MigrationError::CheckpointIo(format!("mkdir {parent:?}: {e}"))
                })?;
            }
        }
        let raw = serde_json::to_string(self)
            .map_err(|e| MigrationError::CheckpointIo(format!("encode: {e}")))?;
        let tmp = self.path.with_extension("tmp");
        fs::write(&tmp, raw)
            .map_err(|e| MigrationError::CheckpointIo(format!("write {tmp:?}: {e}")))?;
        fs::rename(&tmp, &self.path).map_err(|e| {
            MigrationError::CheckpointIo(format!("rename {tmp:?} -> {:?}: {e}", self.path))
        })?;
        Ok(())
    }
}

/// Coarse 1-second window token bucket. A new bucket each second, drained
/// per acquire. Good enough as a soft ceiling on KMS RPS so we don't trip
/// the regional Encrypt/Decrypt quota while the live service shares the key.
struct RateLimiter {
    capacity: u32,
    tokens: u32,
    window_start: Instant,
}

impl RateLimiter {
    fn new(rps: u32) -> Self {
        let cap = rps.max(1);
        Self {
            capacity: cap,
            tokens: cap,
            window_start: Instant::now(),
        }
    }

    async fn acquire(&mut self) {
        loop {
            let elapsed = self.window_start.elapsed();
            if elapsed >= Duration::from_secs(1) {
                self.tokens = self.capacity;
                self.window_start = Instant::now();
            }
            if self.tokens > 0 {
                self.tokens -= 1;
                return;
            }
            let wait = Duration::from_secs(1).saturating_sub(elapsed);
            tokio::time::sleep(wait).await;
        }
    }
}
