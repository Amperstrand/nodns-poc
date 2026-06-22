//! `SQLite` persistence layer for processed events.
//!
//! Ported 1:1 from `nodns-bot/internal/store/sqlite.go`.
//! Uses `rusqlite` with a `Mutex<Connection>` for `Send + Sync` safety.

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use base64::Engine;
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tracing::info;

use crate::types::{AcmeDnsRegistration, AcmeOrder, AcmeOrderLog, DelegationRecord, EventRecord};

// ---------------------------------------------------------------------------
// AES-256-GCM encryption helpers
// ---------------------------------------------------------------------------

/// Derive a 32-byte AES key from a secret string via SHA-256.
fn derive_key(secret: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(secret.as_bytes());
    hasher.finalize().into()
}

/// Encrypt `plaintext` with AES-256-GCM. Returns base64(nonce || ciphertext).
fn encrypt_aes_gcm(key: &[u8; 32], plaintext: &str) -> Result<String, aes_gcm::Error> {
    let cipher = Aes256Gcm::new_from_slice(key).expect("key is 32 bytes");
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher.encrypt(&nonce, plaintext.as_bytes())?;
    let mut combined = Vec::with_capacity(12 + ciphertext.len());
    combined.extend_from_slice(&nonce);
    combined.extend_from_slice(&ciphertext);
    Ok(base64::engine::general_purpose::STANDARD.encode(&combined))
}

/// Decrypt base64(nonce || ciphertext) with AES-256-GCM. Returns plaintext.
fn decrypt_aes_gcm(key: &[u8; 32], encoded: &str) -> Result<String, aes_gcm::Error> {
    let cipher = Aes256Gcm::new_from_slice(key).expect("key is 32 bytes");
    let combined = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| aes_gcm::Error)?;
    if combined.len() < 13 {
        return Err(aes_gcm::Error);
    }
    let nonce = Nonce::from_slice(&combined[..12]);
    let ciphertext = &combined[12..];
    let plaintext = cipher.decrypt(nonce, ciphertext)?;
    String::from_utf8(plaintext).map_err(|_| aes_gcm::Error)
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("opening sqlite {path}: {source}")]
    Open {
        path: String,
        source: rusqlite::Error,
    },

    #[error("setting WAL mode: {0}")]
    WalMode(#[source] rusqlite::Error),

    #[error("creating schema: {0}")]
    Schema(#[source] rusqlite::Error),

    #[error("saving event {0}: {1}")]
    SaveEvent(String, #[source] rusqlite::Error),

    #[error("marking event {0} deleted: {1}")]
    MarkDeleted(String, #[source] rusqlite::Error),

    #[error("getting event {0}: {1}")]
    GetEvent(String, #[source] rusqlite::Error),

    #[error("querying records for pubkey {0}: {1}")]
    GetRecordsByPubkey(String, #[source] rusqlite::Error),

    #[error("counting records for pubkey {0}: {1}")]
    RecordCountByPubkey(String, #[source] rusqlite::Error),

    #[error("counting total records: {0}")]
    TotalRecordCount(#[source] rusqlite::Error),

    #[error("counting recent events for pubkey {0}: {1}")]
    EventsInLastMinute(String, #[source] rusqlite::Error),

    #[error("getting last_seen: {0}")]
    GetLastSeen(#[source] rusqlite::Error),

    #[error("setting last_seen: {0}")]
    SetLastSeen(#[source] rusqlite::Error),

    #[error("listing all records: {0}")]
    ListAllRecords(#[source] rusqlite::Error),

    #[error("saving delegation {0}/{1}: {2}")]
    SaveDelegation(String, String, #[source] rusqlite::Error),

    #[error("getting delegation {0}/{1}: {2}")]
    GetActiveDelegation(String, String, #[source] rusqlite::Error),

    #[error("querying delegations for npub {0}: {1}")]
    GetDelegationsByPubkey(String, #[source] rusqlite::Error),

    #[error("saving registrar key for {0}: {1}")]
    SaveRegistrarKey(String, #[source] rusqlite::Error),

    #[error("getting registrar key for {0}: {1}")]
    GetRegistrarKey(String, #[source] rusqlite::Error),

    #[error("checking record existence: {0}")]
    HasRecord(#[source] rusqlite::Error),

    #[error("scanning record: {0}")]
    ScanRecord(#[source] rusqlite::Error),

    #[error("closing database: {0}")]
    Close(#[source] rusqlite::Error),

    #[error("saving ACME order {0}: {1}")]
    SaveAcmeOrder(String, #[source] rusqlite::Error),

    #[error("updating ACME order {0}: {1}")]
    UpdateAcmeOrder(String, #[source] rusqlite::Error),

    #[error("getting ACME order {0}: {1}")]
    GetAcmeOrder(String, #[source] rusqlite::Error),

    #[error("listing ACME orders for npub {0}: {1}")]
    ListAcmeOrdersByNpub(String, #[source] rusqlite::Error),

    #[error("scanning ACME order: {0}")]
    ScanAcmeOrder(#[source] rusqlite::Error),

    #[error("saving ACME order log for {0}: {1}")]
    SaveAcmeOrderLog(String, #[source] rusqlite::Error),

    #[error("getting ACME order logs for {0}: {1}")]
    GetAcmeOrderLogs(String, #[source] rusqlite::Error),

    #[error("scanning ACME order log: {0}")]
    ScanAcmeOrderLog(#[source] rusqlite::Error),

    #[error("getting meta value: {0}")]
    GetMeta(#[source] rusqlite::Error),

    #[error("setting meta value: {0}")]
    SetMeta(#[source] rusqlite::Error),

    #[error("saving acme-dns registration {0}: {1}")]
    SaveAcmeDnsRegistration(String, #[source] rusqlite::Error),

    #[error("getting acme-dns registration {0}: {1}")]
    GetAcmeDnsRegistration(String, #[source] rusqlite::Error),

    #[error("updating acme-dns TXT {0}: {1}")]
    UpdateAcmeDnsTxt(String, #[source] rusqlite::Error),
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

/// SQLite-backed store wrapping a `Mutex<Connection>` for thread safety.
pub struct Store {
    conn: Mutex<Connection>,
    acme_encryption_key: Option<[u8; 32]>,
}

impl Store {
    // -----------------------------------------------------------------------
    // Construction
    // -----------------------------------------------------------------------

    pub fn new(path: &str, acme_encryption_key: Option<&str>) -> Result<Self, StoreError> {
        let conn = Connection::open(path).map_err(|e| StoreError::Open {
            path: path.to_string(),
            source: e,
        })?;

        conn.execute_batch("PRAGMA journal_mode=WAL")
            .map_err(StoreError::WalMode)?;

        let acme_encryption_key = acme_encryption_key.map(derive_key);

        Ok(Self {
            conn: Mutex::new(conn),
            acme_encryption_key,
        })
    }

    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| {
            tracing::error!("SQLite mutex poisoned, recovering: {}", e);
            e.into_inner()
        })
    }

    // -----------------------------------------------------------------------
    // Schema
    // -----------------------------------------------------------------------

    /// Create the database schema if it does not already exist.
    ///
    /// Schema matches the Go version exactly: same table names, column names,
    /// types, constraints, and indexes.
    pub fn init(&self) -> Result<(), StoreError> {
        let conn = self.conn();
        conn.execute_batch(SCHEMA).map_err(StoreError::Schema)?;

        // Migrations for existing databases that predate schema changes.
        // These are idempotent — ALTER TABLE ADD COLUMN is a no-op if the
        // column already exists (SQLite has no IF NOT EXISTS for columns,
        // so we catch and ignore the "duplicate column" error).
        let migrations = [
            "ALTER TABLE acme_orders ADD COLUMN csr_der TEXT",
            "ALTER TABLE acme_orders ADD COLUMN environment TEXT",
            "ALTER TABLE delegations ADD COLUMN renewal_price INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE delegations ADD COLUMN status TEXT NOT NULL DEFAULT 'active'",
        ];
        for sql in &migrations {
            match conn.execute(sql, []) {
                Ok(_) => info!("migration applied: {}", sql),
                Err(rusqlite::Error::ExecuteReturnedResults) => {}
                Err(e) if e.to_string().contains("duplicate column") => {}
                Err(e) if e.to_string().contains("already exists") => {}
                Err(e) => return Err(StoreError::Schema(e)),
            }
        }

        // Migration: add 'zone' to events primary key.
        // SQLite doesn't support ALTER TABLE to change a PK, so we rebuild.
        // Idempotent: checks sqlite_master for the old 3-column PK first.
        {
            let pk_has_zone: bool = conn
                .query_row(
                    "SELECT sql FROM sqlite_master WHERE type='table' AND name='events'",
                    [],
                    |row| {
                        let sql: String = row.get(0)?;
                        // If the CREATE TABLE statement contains our new PK,
                        // the migration is already applied.
                        Ok(sql.contains("event_id, record_type, name, zone)"))
                    },
                )
                .unwrap_or(false);

            if !pk_has_zone {
                info!("migrating events table: adding zone to primary key");
                conn.execute_batch(
                    "
                    ALTER TABLE events RENAME TO _events_old;
                    CREATE TABLE events (
                        event_id TEXT NOT NULL,
                        npub TEXT NOT NULL,
                        pubkey TEXT NOT NULL,
                        name TEXT NOT NULL,
                        record_type TEXT NOT NULL,
                        ttl INTEGER NOT NULL,
                        rdata TEXT NOT NULL,
                        zone TEXT NOT NULL DEFAULT 'nodns.shop',
                        created_at INTEGER NOT NULL,
                        processed_at INTEGER NOT NULL DEFAULT (unixepoch()),
                        deleted INTEGER NOT NULL DEFAULT 0,
                        PRIMARY KEY (event_id, record_type, name, zone)
                    );
                    INSERT INTO events SELECT * FROM _events_old;
                    DROP TABLE _events_old;
                    CREATE INDEX IF NOT EXISTS idx_events_pubkey ON events(pubkey);
                    CREATE INDEX IF NOT EXISTS idx_events_pubkey_type ON events(pubkey, record_type);
                    CREATE INDEX IF NOT EXISTS idx_events_created ON events(created_at);
                    ",
                )
                .map_err(StoreError::Schema)?;
                info!("migration complete: zone added to events primary key");
            }
        }

        run_migrations(&conn)?;

        info!("database initialized");
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Events
    // -----------------------------------------------------------------------

    /// Persist a processed event record (`INSERT OR REPLACE`).
    pub fn save_event(
        &self,
        event_id: &str,
        npub: &str,
        pubkey: &str,
        name: &str,
        record_type: &str,
        ttl: u32,
        rdata: &str,
        zone: &str,
        created_at: i64,
    ) -> Result<(), StoreError> {
        let conn = self.conn();
        conn.execute(
            "INSERT OR REPLACE INTO events (event_id, npub, pubkey, name, record_type, ttl, rdata, zone, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![event_id, npub, pubkey, name, record_type, ttl, rdata, zone, created_at],
        )
        .map_err(|e| StoreError::SaveEvent(event_id.to_string(), e))?;
        Ok(())
    }

    /// Soft-delete an event by setting `deleted = 1`.
    pub fn mark_deleted(&self, event_id: &str) -> Result<(), StoreError> {
        let conn = self.conn();
        conn.execute(
            "UPDATE events SET deleted = 1 WHERE event_id = ?1",
            params![event_id],
        )
        .map_err(|e| StoreError::MarkDeleted(event_id.to_string(), e))?;
        Ok(())
    }

    pub fn delete_records_by_key(
        &self,
        npub: &str,
        record_type: &str,
        name: &str,
        zone: &str,
    ) -> Result<(), StoreError> {
        let conn = self.conn();
        conn.execute(
            "UPDATE events SET deleted = 1 WHERE npub = ?1 AND record_type = ?2 AND name = ?3 AND zone = ?4 AND deleted = 0",
            params![npub, record_type, name, zone],
        ).map_err(|e| StoreError::MarkDeleted(format!("{npub}-{record_type}-{name}-{zone}"), e))?;
        Ok(())
    }

    /// Retrieve a specific event by ID.  Returns `None` when no row matches.
    pub fn get_event(&self, event_id: &str) -> Result<Option<EventRecord>, StoreError> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT event_id, npub, pubkey, name, record_type, ttl, rdata, zone, created_at, processed_at, deleted
                 FROM events WHERE event_id = ?1",
            )
            .map_err(|e| StoreError::GetEvent(event_id.to_string(), e))?;

        let result = stmt
            .query_row(params![event_id], scan_event_row)
            .optional()
            .map_err(|e| StoreError::GetEvent(event_id.to_string(), e))?;

        Ok(result)
    }

    /// Return all non-deleted records for a pubkey.
    pub fn get_records_by_pubkey(&self, pubkey: &str) -> Result<Vec<EventRecord>, StoreError> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT event_id, npub, pubkey, name, record_type, ttl, rdata, zone, created_at, processed_at, deleted
                 FROM events WHERE pubkey = ?1 AND deleted = 0",
            )
            .map_err(|e| StoreError::GetRecordsByPubkey(pubkey.to_string(), e))?;

        let records = stmt
            .query_map(params![pubkey], scan_event_row)
            .map_err(|e| StoreError::GetRecordsByPubkey(pubkey.to_string(), e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::ScanRecord)?;

        Ok(records)
    }

    /// Return all non-deleted records for an exact npub match.
    pub fn get_records_by_npub_exact(&self, npub: &str) -> Result<Vec<EventRecord>, StoreError> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT event_id, npub, pubkey, name, record_type, ttl, rdata, zone, created_at, processed_at, deleted
                 FROM events WHERE npub = ?1 AND deleted = 0",
            )
            .map_err(|e| StoreError::GetRecordsByPubkey(npub.to_string(), e))?;

        let records = stmt
            .query_map(params![npub], scan_event_row)
            .map_err(|e| StoreError::GetRecordsByPubkey(npub.to_string(), e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::ScanRecord)?;

        Ok(records)
    }

    /// Return all non-deleted records matching a hex pubkey prefix (LIKE prefix%).
    pub fn lookup_by_pubkey_prefix(&self, prefix: &str) -> Result<Vec<EventRecord>, StoreError> {
        let pattern = format!("{}%", prefix.to_lowercase());
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT event_id, npub, pubkey, name, record_type, ttl, rdata, zone, created_at, processed_at, deleted
                 FROM events WHERE pubkey LIKE ?1 AND deleted = 0",
            )
            .map_err(|e| StoreError::GetRecordsByPubkey(prefix.to_string(), e))?;

        let records = stmt
            .query_map(params![pattern], scan_event_row)
            .map_err(|e| StoreError::GetRecordsByPubkey(prefix.to_string(), e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::ScanRecord)?;

        Ok(records)
    }

    /// Return the count of active (non-deleted) records for a pubkey.
    pub fn record_count_by_pubkey(&self, pubkey: &str) -> Result<usize, StoreError> {
        let conn = self.conn();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM events WHERE pubkey = ?1 AND deleted = 0",
                params![pubkey],
                |row| row.get(0),
            )
            .map_err(|e| StoreError::RecordCountByPubkey(pubkey.to_string(), e))?;

        Ok(count as usize)
    }

    /// Return the total count of active (non-deleted) records across all zones.
    pub fn total_record_count(&self) -> Result<i64, StoreError> {
        let conn = self.conn();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM events WHERE deleted = 0", [], |row| {
                row.get(0)
            })
            .map_err(StoreError::TotalRecordCount)?;
        Ok(count)
    }

    /// Return the number of events processed for a pubkey in the last 60 seconds.
    pub fn events_in_last_minute(&self, pubkey: &str) -> Result<usize, StoreError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let cutoff = now - 60;

        let conn = self.conn();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM events WHERE pubkey = ?1 AND processed_at > ?2",
                params![pubkey, cutoff],
                |row| row.get(0),
            )
            .map_err(|e| StoreError::EventsInLastMinute(pubkey.to_string(), e))?;

        Ok(count as usize)
    }

    // -----------------------------------------------------------------------
    // Meta
    // -----------------------------------------------------------------------

    /// Get the `last_seen` timestamp from the `meta` table.
    /// Returns `0` when no row exists.
    pub fn get_last_seen(&self) -> Result<i64, StoreError> {
        let conn = self.conn();
        let val: Option<String> = conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'last_seen'",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(StoreError::GetLastSeen)?
            .flatten();

        let ts = match val {
            Some(s) => s.parse::<i64>().unwrap_or(0),
            None => 0,
        };
        Ok(ts)
    }

    /// Update the `last_seen` timestamp in the `meta` table.
    pub fn set_last_seen(&self, ts: i64) -> Result<(), StoreError> {
        let conn = self.conn();
        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('last_seen', ?1)",
            params![ts.to_string()],
        )
        .map_err(StoreError::SetLastSeen)?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Lifecycle
    // -----------------------------------------------------------------------

    fn decrypt_private_key(&self, mut order: AcmeOrder) -> AcmeOrder {
        if let (Some(encrypted), Some(key)) = (&order.private_key_pem, &self.acme_encryption_key) {
            if let Ok(plain) = decrypt_aes_gcm(key, encrypted) {
                order.private_key_pem = Some(plain);
            } else {
                tracing::error!(order_id = %order.id, "failed to decrypt private_key_pem, clearing");
                order.private_key_pem = None;
            }
        }
        order
    }

    /// Close the database connection.
    pub fn close(self) -> Result<(), StoreError> {
        let conn = self.conn.into_inner().unwrap_or_else(|e| {
            tracing::error!("SQLite mutex poisoned on close: {}", e);
            e.into_inner()
        });
        conn.close().map_err(|(_, e)| StoreError::Close(e))
    }

    // -----------------------------------------------------------------------
    // Listing
    // -----------------------------------------------------------------------

    /// Return all non-deleted records ordered by `created_at DESC`.
    pub fn list_all_records(&self) -> Result<Vec<EventRecord>, StoreError> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT event_id, npub, pubkey, name, record_type, ttl, rdata, zone, created_at, processed_at, deleted
                 FROM events WHERE deleted = 0
                 ORDER BY created_at DESC",
            )
            .map_err(StoreError::ListAllRecords)?;

        let records = stmt
            .query_map([], scan_event_row)
            .map_err(StoreError::ListAllRecords)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::ScanRecord)?;

        Ok(records)
    }

    pub fn get_records_by_domain(&self, domain: &str) -> Result<Vec<EventRecord>, StoreError> {
        let conn = self.conn();
        let pattern = format!("{domain}.%");
        let mut stmt = conn
            .prepare(
                "SELECT event_id, npub, pubkey, name, record_type, ttl, rdata, zone, created_at, processed_at, deleted
                 FROM events WHERE (name || '.' || npub || '.' || zone) LIKE ?1 AND deleted = 0
                 ORDER BY created_at DESC",
            )
            .map_err(StoreError::ListAllRecords)?;

        let records = stmt
            .query_map(params![pattern], scan_event_row)
            .map_err(StoreError::ListAllRecords)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::ScanRecord)?;

        Ok(records)
    }

    // -----------------------------------------------------------------------
    // Delegations
    // -----------------------------------------------------------------------

    /// Store a delegation event (`INSERT OR REPLACE`).
    pub fn save_delegation(
        &self,
        event_id: &str,
        domain: &str,
        zone: &str,
        npub: &str,
        pubkey: &str,
        valid_from: i64,
        valid_until: i64,
        renew_by: i64,
        registrar_pubkey: &str,
    ) -> Result<(), StoreError> {
        self.save_delegation_with_price(
            event_id,
            domain,
            zone,
            npub,
            pubkey,
            valid_from,
            valid_until,
            renew_by,
            registrar_pubkey,
            0,
        )
    }

    /// Store a delegation with a locked renewal price (`INSERT OR REPLACE`).
    pub fn save_delegation_with_price(
        &self,
        event_id: &str,
        domain: &str,
        zone: &str,
        npub: &str,
        pubkey: &str,
        valid_from: i64,
        valid_until: i64,
        renew_by: i64,
        registrar_pubkey: &str,
        renewal_price: i64,
    ) -> Result<(), StoreError> {
        let conn = self.conn();
        conn.execute(
            "INSERT OR REPLACE INTO delegations (event_id, domain, zone, npub, pubkey, valid_from, valid_until, renew_by, registrar_pubkey, renewal_price, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'active', unixepoch())",
            params![event_id, domain, zone, npub, pubkey, valid_from, valid_until, renew_by, registrar_pubkey, renewal_price],
        )
        .map_err(|e| StoreError::SaveDelegation(domain.to_string(), zone.to_string(), e))?;

        info!(
            domain = domain,
            zone = zone,
            npub = npub,
            renewal_price = renewal_price,
            "delegation saved"
        );
        Ok(())
    }

    /// Return the active (valid, non-expired) delegation for a domain in a zone.
    /// Returns `None` when no matching delegation exists.
    pub fn get_active_delegation(
        &self,
        domain: &str,
        zone: &str,
    ) -> Result<Option<DelegationRecord>, StoreError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT event_id, domain, zone, npub, pubkey, valid_from, valid_until, renew_by, registrar_pubkey, renewal_price, status, created_at, processed_at
                 FROM delegations
                 WHERE domain = ?1 AND zone = ?2 AND valid_from <= ?3 AND valid_until > ?4 AND status = 'active'
                 ORDER BY created_at DESC
                 LIMIT 1",
            )
            .map_err(|e| StoreError::GetActiveDelegation(domain.to_string(), zone.to_string(), e))?;

        let result = stmt
            .query_row(params![domain, zone, now, now], scan_delegation_row)
            .optional()
            .map_err(|e| {
                StoreError::GetActiveDelegation(domain.to_string(), zone.to_string(), e)
            })?;

        Ok(result)
    }

    /// Return any delegation for a domain in a zone, regardless of status.
    pub fn get_delegation(
        &self,
        domain: &str,
        zone: &str,
    ) -> Result<Option<DelegationRecord>, StoreError> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT event_id, domain, zone, npub, pubkey, valid_from, valid_until, renew_by, registrar_pubkey, renewal_price, status, created_at, processed_at
                 FROM delegations
                 WHERE domain = ?1 AND zone = ?2
                 ORDER BY created_at DESC
                 LIMIT 1",
            )
            .map_err(|e| StoreError::GetActiveDelegation(domain.to_string(), zone.to_string(), e))?;

        let result = stmt
            .query_row(params![domain, zone], scan_delegation_row)
            .optional()
            .map_err(|e| {
                StoreError::GetActiveDelegation(domain.to_string(), zone.to_string(), e)
            })?;

        Ok(result)
    }

    /// Renew a delegation: update `valid_until`, `renew_by`, `event_id`, and reset status to 'active'.
    pub fn renew_delegation(
        &self,
        domain: &str,
        zone: &str,
        new_valid_until: i64,
        new_renew_by: i64,
        event_id: &str,
    ) -> Result<(), StoreError> {
        let conn = self.conn();
        conn.execute(
            "UPDATE delegations SET valid_until = ?1, renew_by = ?2, event_id = ?3, status = 'active'
             WHERE domain = ?4 AND zone = ?5",
            params![new_valid_until, new_renew_by, event_id, domain, zone],
        )
        .map_err(|e| StoreError::SaveDelegation(domain.to_string(), zone.to_string(), e))?;

        info!(
            domain = domain,
            zone = zone,
            new_valid_until = new_valid_until,
            "delegation renewed"
        );
        Ok(())
    }

    /// Return all active delegations for an npub.
    pub fn get_delegations_by_pubkey(
        &self,
        npub: &str,
    ) -> Result<Vec<DelegationRecord>, StoreError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT event_id, domain, zone, npub, pubkey, valid_from, valid_until, renew_by, registrar_pubkey, renewal_price, status, created_at, processed_at
                 FROM delegations
                 WHERE npub = ?1 AND valid_from <= ?2 AND valid_until > ?3 AND status = 'active'
                 ORDER BY created_at DESC",
            )
            .map_err(|e| StoreError::GetDelegationsByPubkey(npub.to_string(), e))?;

        let records = stmt
            .query_map(params![npub, now, now], scan_delegation_row)
            .map_err(|e| StoreError::GetDelegationsByPubkey(npub.to_string(), e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::ScanRecord)?;

        Ok(records)
    }

    /// Check if a name is available (no active or grace delegation exists) in a zone.
    pub fn is_name_available(&self, name: &str, zone: &str) -> Result<bool, StoreError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let conn = self.conn();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM delegations
                 WHERE domain = ?1 AND zone = ?2 AND (
                     (status = 'active' AND valid_until > ?3) OR
                     status = 'grace'
                 )",
                params![name, zone, now],
                |row| row.get(0),
            )
            .map_err(|e| StoreError::GetActiveDelegation(name.to_string(), zone.to_string(), e))?;

        Ok(count == 0)
    }

    /// Get all delegations that have passed their `valid_until` but are not yet expired.
    pub fn get_delegations_past_valid_until(&self) -> Result<Vec<DelegationRecord>, StoreError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT event_id, domain, zone, npub, pubkey, valid_from, valid_until, renew_by, registrar_pubkey, renewal_price, status, created_at, processed_at
                 FROM delegations
                 WHERE valid_until <= ?1 AND status != 'expired'
                 ORDER BY valid_until ASC",
            )
            .map_err(StoreError::ListAllRecords)?;

        let records = stmt
            .query_map(params![now], scan_delegation_row)
            .map_err(StoreError::ListAllRecords)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::ScanRecord)?;

        Ok(records)
    }

    /// Get test delegations whose `expires_at` has passed and are not yet expired.
    ///
    /// Returns rows where `test_mint = 1 AND expires_at IS NOT NULL AND
    /// expires_at <= now AND status != 'expired'`. Used by the test record
    /// cleanup cron to sweep `testing*` registrations past their TTL.
    pub fn get_test_delegations_expired(&self) -> Result<Vec<DelegationRecord>, StoreError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT event_id, domain, zone, npub, pubkey, valid_from, valid_until, renew_by, registrar_pubkey, renewal_price, status, created_at, processed_at
                 FROM delegations
                 WHERE test_mint = 1 AND expires_at IS NOT NULL AND expires_at <= ?1 AND status != 'expired'
                 ORDER BY expires_at ASC",
            )
            .map_err(StoreError::ListAllRecords)?;

        let records = stmt
            .query_map(params![now], scan_delegation_row)
            .map_err(StoreError::ListAllRecords)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::ScanRecord)?;

        Ok(records)
    }

    /// Set a delegation's status to 'grace'.
    pub fn mark_delegation_grace(&self, domain: &str, zone: &str) -> Result<(), StoreError> {
        let conn = self.conn();
        conn.execute(
            "UPDATE delegations SET status = 'grace' WHERE domain = ?1 AND zone = ?2 AND status = 'active'",
            params![domain, zone],
        )
        .map_err(|e| StoreError::SaveDelegation(domain.to_string(), zone.to_string(), e))?;
        Ok(())
    }

    /// Set a delegation's status to 'expired'.
    pub fn mark_delegation_expired(&self, domain: &str, zone: &str) -> Result<(), StoreError> {
        let conn = self.conn();
        conn.execute(
            "UPDATE delegations SET status = 'expired' WHERE domain = ?1 AND zone = ?2 AND status IN ('active', 'grace')",
            params![domain, zone],
        )
        .map_err(|e| StoreError::SaveDelegation(domain.to_string(), zone.to_string(), e))?;
        Ok(())
    }

    /// Soft-delete all non-deleted DNS records for a given npub + zone.
    /// Called when a delegation expires so its records stop being served.
    /// Returns the number of rows affected.
    pub fn soft_delete_records_by_npub_zone(
        &self,
        npub: &str,
        zone: &str,
    ) -> Result<usize, StoreError> {
        let conn = self.conn();
        let affected = conn
            .execute(
                "UPDATE events SET deleted = 1 WHERE npub = ?1 AND zone = ?2 AND deleted = 0",
                params![npub, zone],
            )
            .map_err(|e| StoreError::MarkDeleted(format!("{npub}-{zone}"), e))?;
        Ok(affected)
    }

    /// Store the registrar pubkey for a zone (`INSERT OR REPLACE`).
    pub fn save_registrar_key(
        &self,
        zone: &str,
        pubkey_hex: &str,
        npub: &str,
        source: &str,
        event_id: &str,
    ) -> Result<(), StoreError> {
        let conn = self.conn();
        conn.execute(
            "INSERT OR REPLACE INTO registrar_keys (zone, pubkey_hex, npub, source, event_id, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, unixepoch())",
            params![zone, pubkey_hex, npub, source, event_id],
        )
        .map_err(|e| StoreError::SaveRegistrarKey(zone.to_string(), e))?;

        info!(zone = zone, pubkey = pubkey_hex, "registrar key saved");
        Ok(())
    }

    /// Return the registrar pubkey hex for a zone, or empty string if not found.
    pub fn get_registrar_key(&self, zone: &str) -> Result<String, StoreError> {
        let conn = self.conn();
        let result: Option<String> = conn
            .query_row(
                "SELECT pubkey_hex FROM registrar_keys WHERE zone = ?1",
                params![zone],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| StoreError::GetRegistrarKey(zone.to_string(), e))?
            .flatten();

        Ok(result.unwrap_or_default())
    }

    // -----------------------------------------------------------------------
    // Record existence check
    // -----------------------------------------------------------------------

    /// Check if a record already exists for the given npub+type+name+zone.
    pub fn has_record(
        &self,
        npub: &str,
        record_type: &str,
        name: &str,
        zone: &str,
    ) -> Result<bool, StoreError> {
        let conn = self.conn();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM events
                 WHERE npub = ?1 AND record_type = ?2 AND name = ?3 AND zone = ?4 AND deleted = 0",
                params![npub, record_type, name, zone],
                |row| row.get(0),
            )
            .map_err(StoreError::HasRecord)?;

        Ok(count > 0)
    }

    // -----------------------------------------------------------------------
    // ACME orders
    // -----------------------------------------------------------------------

    pub fn save_acme_order(
        &self,
        id: &str,
        domain: &str,
        npub: &str,
        status: &str,
        csr_der: Option<&str>,
        environment: Option<&str>,
    ) -> Result<(), StoreError> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO acme_orders (id, domain, npub, status, csr_der, environment) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, domain, npub, status, csr_der, environment],
        )
        .map_err(|e| StoreError::SaveAcmeOrder(id.to_string(), e))?;
        Ok(())
    }

    pub fn update_acme_order_status(
        &self,
        id: &str,
        status: &str,
        certificate_pem: Option<&str>,
        private_key_pem: Option<&str>,
        error: Option<&str>,
    ) -> Result<(), StoreError> {
        let encrypted_key = match (private_key_pem, &self.acme_encryption_key) {
            (Some(pem), Some(key)) => Some(encrypt_aes_gcm(key, pem).map_err(|_| {
                StoreError::UpdateAcmeOrder(
                    id.to_string(),
                    rusqlite::Error::InvalidParameterName("encryption failed".into()),
                )
            })?),
            (Some(pem), None) => Some(pem.to_string()),
            (None, _) => None,
        };
        let conn = self.conn();
        conn.execute(
            "UPDATE acme_orders SET status = ?1, certificate_pem = ?2, private_key_pem = ?3, error = ?4, updated_at = unixepoch() WHERE id = ?5",
            params![status, certificate_pem, encrypted_key, error, id],
        )
        .map_err(|e| StoreError::UpdateAcmeOrder(id.to_string(), e))?;
        Ok(())
    }

    pub fn get_acme_order(&self, id: &str) -> Result<Option<AcmeOrder>, StoreError> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id, domain, npub, status, certificate_pem, private_key_pem, error, csr_der, environment, created_at, updated_at
                 FROM acme_orders WHERE id = ?1",
            )
            .map_err(|e| StoreError::GetAcmeOrder(id.to_string(), e))?;

        let result = stmt
            .query_row(params![id], scan_acme_order_row)
            .optional()
            .map_err(|e| StoreError::GetAcmeOrder(id.to_string(), e))?;

        Ok(result.map(|o| self.decrypt_private_key(o)))
    }

    pub fn clear_acme_private_key(&self, id: &str) -> Result<(), StoreError> {
        let conn = self.conn();
        conn.execute(
            "UPDATE acme_orders SET private_key_pem = NULL WHERE id = ?1",
            params![id],
        )
        .map_err(|e| StoreError::UpdateAcmeOrder(id.to_string(), e))?;
        Ok(())
    }

    pub fn list_acme_orders_by_npub(&self, npub: &str) -> Result<Vec<AcmeOrder>, StoreError> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id, domain, npub, status, certificate_pem, private_key_pem, error, csr_der, environment, created_at, updated_at
                 FROM acme_orders WHERE npub = ?1
                 ORDER BY created_at DESC",
            )
            .map_err(|e| StoreError::ListAcmeOrdersByNpub(npub.to_string(), e))?;

        let records = stmt
            .query_map(params![npub], scan_acme_order_row)
            .map_err(|e| StoreError::ListAcmeOrdersByNpub(npub.to_string(), e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::ScanAcmeOrder)?;

        Ok(records
            .into_iter()
            .map(|o| self.decrypt_private_key(o))
            .collect())
    }

    pub fn save_acme_order_log(
        &self,
        order_id: &str,
        stage: &str,
        message: &str,
        details: Option<&str>,
    ) -> Result<(), StoreError> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO acme_order_logs (order_id, stage, message, details) VALUES (?1, ?2, ?3, ?4)",
            params![order_id, stage, message, details],
        )
        .map_err(|e| StoreError::SaveAcmeOrderLog(order_id.to_string(), e))?;
        Ok(())
    }

    pub fn get_acme_order_logs(&self, order_id: &str) -> Result<Vec<AcmeOrderLog>, StoreError> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id, order_id, stage, message, details, created_at
                 FROM acme_order_logs WHERE order_id = ?1
                 ORDER BY id ASC",
            )
            .map_err(|e| StoreError::GetAcmeOrderLogs(order_id.to_string(), e))?;

        let records = stmt
            .query_map(params![order_id], scan_acme_order_log_row)
            .map_err(|e| StoreError::GetAcmeOrderLogs(order_id.to_string(), e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::ScanAcmeOrderLog)?;

        Ok(records)
    }

    // -----------------------------------------------------------------------
    // Meta (generic key/value)
    // -----------------------------------------------------------------------

    pub fn get_meta(&self, key: &str) -> Result<Option<String>, StoreError> {
        let conn = self.conn();
        let result: Option<String> = conn
            .query_row(
                "SELECT value FROM meta WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()
            .map_err(StoreError::GetMeta)?
            .flatten();
        Ok(result)
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<(), StoreError> {
        let conn = self.conn();
        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
            params![key, value],
        )
        .map_err(StoreError::SetMeta)?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // acme-dns registrations
    // -----------------------------------------------------------------------

    pub fn save_acme_dns_registration(
        &self,
        subdomain: &str,
        username: &str,
        password: &str,
        npub: &str,
        zone: &str,
        fulldomain: &str,
    ) -> Result<(), StoreError> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO acme_dns_registrations (subdomain, username, password, npub, zone, fulldomain)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![subdomain, username, password, npub, zone, fulldomain],
        )
        .map_err(|e| StoreError::SaveAcmeDnsRegistration(subdomain.to_string(), e))?;
        Ok(())
    }

    pub fn get_acme_dns_registration_by_username(
        &self,
        username: &str,
    ) -> Result<Option<AcmeDnsRegistration>, StoreError> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT subdomain, username, password, npub, zone, fulldomain, txt_value, txt_value_prev, created_at, updated_at
                 FROM acme_dns_registrations WHERE username = ?1",
            )
            .map_err(|e| StoreError::GetAcmeDnsRegistration(username.to_string(), e))?;

        let result = stmt
            .query_row(params![username], scan_acme_dns_registration_row)
            .optional()
            .map_err(|e| StoreError::GetAcmeDnsRegistration(username.to_string(), e))?;

        Ok(result)
    }

    pub fn update_acme_dns_txt(&self, subdomain: &str, txt_value: &str) -> Result<(), StoreError> {
        let conn = self.conn();
        conn.execute(
            "UPDATE acme_dns_registrations SET txt_value_prev = txt_value, txt_value = ?1, updated_at = unixepoch() WHERE subdomain = ?2",
            params![txt_value, subdomain],
        )
        .map_err(|e| StoreError::UpdateAcmeDnsTxt(subdomain.to_string(), e))?;
        Ok(())
    }
}

fn run_migrations(conn: &Connection) -> Result<(), StoreError> {
    let alter_statements = [
        "ALTER TABLE delegations ADD COLUMN mint_url TEXT",
        "ALTER TABLE delegations ADD COLUMN test_mint INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE delegations ADD COLUMN expires_at INTEGER",
        "ALTER TABLE delegations ADD COLUMN epp_auth_info_encrypted BLOB",
        "ALTER TABLE events ADD COLUMN mint_url TEXT",
        "ALTER TABLE events ADD COLUMN test_mint INTEGER NOT NULL DEFAULT 0",
    ];

    for sql in &alter_statements {
        match conn.execute(sql, []) {
            Ok(_) => info!("migration applied: {}", sql),
            Err(rusqlite::Error::ExecuteReturnedResults) => {}
            Err(e) if e.to_string().contains("duplicate column") => {}
            Err(e) if e.to_string().contains("already exists") => {}
            Err(e) => return Err(StoreError::Schema(e)),
        }
    }

    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_delegations_test_mint ON delegations(test_mint) WHERE test_mint = 1;
         CREATE INDEX IF NOT EXISTS idx_delegations_expires ON delegations(expires_at) WHERE expires_at IS NOT NULL;
         CREATE INDEX IF NOT EXISTS idx_events_test_mint ON events(test_mint) WHERE test_mint = 1;",
    )
    .map_err(StoreError::Schema)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Schema constant — matches Go exactly
// ---------------------------------------------------------------------------

const SCHEMA: &str = r"
    CREATE TABLE IF NOT EXISTS events (
        event_id TEXT NOT NULL,
        npub TEXT NOT NULL,
        pubkey TEXT NOT NULL,
        name TEXT NOT NULL,
        record_type TEXT NOT NULL,
        ttl INTEGER NOT NULL,
        rdata TEXT NOT NULL,
        zone TEXT NOT NULL DEFAULT 'nodns.shop',
        created_at INTEGER NOT NULL,
        processed_at INTEGER NOT NULL DEFAULT (unixepoch()),
        deleted INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (event_id, record_type, name, zone)
    );

    CREATE INDEX IF NOT EXISTS idx_events_pubkey ON events(pubkey);
    CREATE INDEX IF NOT EXISTS idx_events_pubkey_type ON events(pubkey, record_type);
    CREATE INDEX IF NOT EXISTS idx_events_created ON events(created_at);

    CREATE TABLE IF NOT EXISTS meta (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );

    INSERT OR IGNORE INTO meta (key, value) VALUES ('last_seen', '0');

    CREATE TABLE IF NOT EXISTS delegations (
        event_id TEXT NOT NULL,
        domain TEXT NOT NULL,
        zone TEXT NOT NULL,
        npub TEXT NOT NULL,
        pubkey TEXT NOT NULL,
        valid_from INTEGER NOT NULL,
        valid_until INTEGER NOT NULL,
        renew_by INTEGER NOT NULL,
        registrar_pubkey TEXT NOT NULL,
        renewal_price INTEGER NOT NULL DEFAULT 0,
        status TEXT NOT NULL DEFAULT 'active',
        created_at INTEGER NOT NULL,
        processed_at INTEGER NOT NULL DEFAULT (unixepoch()),
        PRIMARY KEY (domain, zone)
    );

    CREATE INDEX IF NOT EXISTS idx_delegations_zone ON delegations(zone);
    CREATE INDEX IF NOT EXISTS idx_delegations_npub ON delegations(npub);

    CREATE TABLE IF NOT EXISTS registrar_keys (
        zone TEXT PRIMARY KEY,
        pubkey_hex TEXT NOT NULL,
        npub TEXT NOT NULL,
        source TEXT NOT NULL DEFAULT 'nostr',
        event_id TEXT,
        updated_at INTEGER NOT NULL DEFAULT (unixepoch())
    );

    CREATE TABLE IF NOT EXISTS acme_orders (
        id TEXT PRIMARY KEY,
        domain TEXT NOT NULL,
        npub TEXT NOT NULL,
        status TEXT NOT NULL DEFAULT 'pending',
        certificate_pem TEXT,
        private_key_pem TEXT,
        error TEXT,
        csr_der TEXT,
        created_at INTEGER NOT NULL DEFAULT (unixepoch()),
        updated_at INTEGER NOT NULL DEFAULT (unixepoch())
    );

    CREATE INDEX IF NOT EXISTS idx_acme_orders_npub ON acme_orders(npub);

    CREATE TABLE IF NOT EXISTS acme_order_logs (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        order_id TEXT NOT NULL,
        stage TEXT NOT NULL,
        message TEXT NOT NULL,
        details TEXT,
        created_at INTEGER NOT NULL DEFAULT (unixepoch()),
        FOREIGN KEY (order_id) REFERENCES acme_orders(id)
    );

    CREATE INDEX IF NOT EXISTS idx_acme_order_logs_order ON acme_order_logs(order_id);

    CREATE TABLE IF NOT EXISTS acme_dns_registrations (
        subdomain TEXT PRIMARY KEY,
        username TEXT NOT NULL UNIQUE,
        password TEXT NOT NULL UNIQUE,
        npub TEXT NOT NULL,
        zone TEXT NOT NULL,
        fulldomain TEXT NOT NULL,
        txt_value TEXT,
        txt_value_prev TEXT,
        created_at INTEGER NOT NULL DEFAULT (unixepoch()),
        updated_at INTEGER NOT NULL DEFAULT (unixepoch())
    );

    CREATE INDEX IF NOT EXISTS idx_acme_dns_reg_username ON acme_dns_registrations(username);
    CREATE INDEX IF NOT EXISTS idx_acme_dns_reg_npub ON acme_dns_registrations(npub);

    CREATE TABLE IF NOT EXISTS epp_orders (
        id TEXT PRIMARY KEY,
        delegation_domain TEXT NOT NULL,
        delegation_zone TEXT NOT NULL,
        operation TEXT NOT NULL,
        epp_cltrid TEXT NOT NULL,
        epp_svtrid TEXT,
        epp_result_code INTEGER,
        epp_result_msg TEXT,
        request_xml TEXT,
        response_xml TEXT,
        created_at INTEGER NOT NULL DEFAULT (unixepoch()),
        completed_at INTEGER,
        FOREIGN KEY (delegation_domain, delegation_zone) REFERENCES delegations(domain, zone)
    );

    CREATE INDEX IF NOT EXISTS idx_epp_orders_delegation ON epp_orders(delegation_domain, delegation_zone);
    CREATE INDEX IF NOT EXISTS idx_epp_orders_cltrid ON epp_orders(epp_cltrid);
";

// ---------------------------------------------------------------------------
// Row scanners
// ---------------------------------------------------------------------------

/// Scan a single `events` row into an `EventRecord`.
fn scan_event_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<EventRecord> {
    let deleted: i32 = row.get(10)?;
    let processed_at: i64 = row.get(9)?;

    Ok(EventRecord {
        event_id: row.get(0)?,
        npub: row.get(1)?,
        pubkey: row.get(2)?,
        name: row.get(3)?,
        record_type: row.get(4)?,
        ttl: row.get(5)?,
        rdata: row.get(6)?,
        zone: row.get(7)?,
        created_at: row.get(8)?,
        processed_at,
        deleted: deleted != 0,
    })
}

/// Scan a single `delegations` row into a `DelegationRecord`.
fn scan_delegation_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DelegationRecord> {
    Ok(DelegationRecord {
        event_id: row.get(0)?,
        domain: row.get(1)?,
        zone: row.get(2)?,
        npub: row.get(3)?,
        pubkey: row.get(4)?,
        valid_from: row.get(5)?,
        valid_until: row.get(6)?,
        renew_by: row.get(7)?,
        registrar_pubkey: row.get(8)?,
        renewal_price: row.get(9)?,
        status: row.get(10)?,
        created_at: row.get(11)?,
        processed_at: row.get(12)?,
    })
}

fn scan_acme_order_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AcmeOrder> {
    Ok(AcmeOrder {
        id: row.get(0)?,
        domain: row.get(1)?,
        npub: row.get(2)?,
        status: row.get(3)?,
        certificate_pem: row.get(4)?,
        private_key_pem: row.get(5)?,
        error: row.get(6)?,
        csr_der: row.get(7)?,
        environment: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn scan_acme_order_log_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AcmeOrderLog> {
    Ok(AcmeOrderLog {
        id: row.get(0)?,
        order_id: row.get(1)?,
        stage: row.get(2)?,
        message: row.get(3)?,
        details: row.get(4)?,
        created_at: row.get(5)?,
    })
}

fn scan_acme_dns_registration_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<AcmeDnsRegistration> {
    Ok(AcmeDnsRegistration {
        subdomain: row.get(0)?,
        username: row.get(1)?,
        password: row.get(2)?,
        npub: row.get(3)?,
        zone: row.get(4)?,
        fulldomain: row.get(5)?,
        txt_value: row.get(6)?,
        txt_value_prev: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key = derive_key("test-secret");
        let plaintext = "-----BEGIN PRIVATE KEY-----\nMIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQC=\n-----END PRIVATE KEY-----";
        let encrypted = encrypt_aes_gcm(&key, plaintext).unwrap();
        let decrypted = decrypt_aes_gcm(&key, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn wrong_key_returns_error() {
        let key_a = derive_key("secret-a");
        let key_b = derive_key("secret-b");
        let plaintext = "sensitive data";
        let encrypted = encrypt_aes_gcm(&key_a, plaintext).unwrap();
        assert!(decrypt_aes_gcm(&key_b, &encrypted).is_err());
    }

    #[test]
    fn corrupted_ciphertext_returns_error() {
        let key = derive_key("test-secret");
        let encrypted = encrypt_aes_gcm(&key, "hello").unwrap();
        let corrupted = &encrypted[..encrypted.len() - 4];
        assert!(decrypt_aes_gcm(&key, corrupted).is_err());
    }

    #[test]
    fn encrypt_produces_different_ciphertexts() {
        let key = derive_key("test-secret");
        let plaintext = "same plaintext";
        let enc1 = encrypt_aes_gcm(&key, plaintext).unwrap();
        let enc2 = encrypt_aes_gcm(&key, plaintext).unwrap();
        assert_ne!(
            enc1, enc2,
            "random nonce should produce different ciphertexts"
        );
        assert_eq!(decrypt_aes_gcm(&key, &enc1).unwrap(), plaintext);
        assert_eq!(decrypt_aes_gcm(&key, &enc2).unwrap(), plaintext);
    }

    #[test]
    fn store_encrypt_decrypt_private_key_roundtrip() {
        let store = Store::new(":memory:", Some("my-encryption-secret")).unwrap();
        store.init().unwrap();

        let order_id = "test-order-encrypt-1";
        store
            .save_acme_order(order_id, "example.com", "npub123", "pending", None, None)
            .unwrap();

        let private_key = "-----BEGIN PRIVATE KEY-----\nSECRETKEYDATA\n-----END PRIVATE KEY-----";
        store
            .update_acme_order_status(order_id, "valid", Some("cert-pem"), Some(private_key), None)
            .unwrap();

        let order = store.get_acme_order(order_id).unwrap().unwrap();
        assert_eq!(order.private_key_pem.as_deref(), Some(private_key));
        assert_eq!(order.certificate_pem.as_deref(), Some("cert-pem"));
    }

    #[test]
    fn store_no_encryption_key_passes_through() {
        let store = Store::new(":memory:", None).unwrap();
        store.init().unwrap();

        let order_id = "test-order-noenc-1";
        store
            .save_acme_order(order_id, "example.com", "npub123", "pending", None, None)
            .unwrap();

        let private_key = "plain-key-data";
        store
            .update_acme_order_status(order_id, "valid", None, Some(private_key), None)
            .unwrap();

        let order = store.get_acme_order(order_id).unwrap().unwrap();
        assert_eq!(order.private_key_pem.as_deref(), Some(private_key));
    }

    #[test]
    fn is_name_available_when_no_delegation() {
        let store = Store::new(":memory:", None).unwrap();
        store.init().unwrap();

        assert!(store.is_name_available("alice", "nodns.shop").unwrap());
    }

    #[test]
    fn is_name_unavailable_when_active_delegation() {
        let store = Store::new(":memory:", None).unwrap();
        store.init().unwrap();

        store
            .save_delegation(
                "event1",
                "alice",
                "nodns.shop",
                "npub1abc",
                "pubkey1",
                0,
                9999999999,
                9999999999,
                "registrar1",
            )
            .unwrap();

        assert!(!store.is_name_available("alice", "nodns.shop").unwrap());
    }

    #[test]
    fn transition_active_to_grace_to_expired() {
        let store = Store::new(":memory:", None).unwrap();
        store.init().unwrap();

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        store
            .save_delegation(
                "event1",
                "alice",
                "nodns.shop",
                "npub1abc",
                "pubkey1",
                now - 100,
                now + 9999,
                now + 9999,
                "registrar1",
            )
            .unwrap();

        assert!(!store.is_name_available("alice", "nodns.shop").unwrap());

        store.mark_delegation_grace("alice", "nodns.shop").unwrap();
        assert!(!store.is_name_available("alice", "nodns.shop").unwrap());

        store
            .mark_delegation_expired("alice", "nodns.shop")
            .unwrap();
        assert!(store.is_name_available("alice", "nodns.shop").unwrap());
    }

    #[test]
    fn is_name_available_in_different_zone() {
        let store = Store::new(":memory:", None).unwrap();
        store.init().unwrap();

        store
            .save_delegation(
                "event1",
                "alice",
                "nodns.shop",
                "npub1abc",
                "pubkey1",
                0,
                9999999999,
                9999999999,
                "registrar1",
            )
            .unwrap();

        assert!(store.is_name_available("alice", "other.shop").unwrap());
    }

    #[test]
    fn save_delegation_with_price_stores_price() {
        let store = Store::new(":memory:", None).unwrap();
        store.init().unwrap();

        store
            .save_delegation_with_price(
                "event1",
                "alice",
                "nodns.shop",
                "npub1abc",
                "pubkey1",
                0,
                9999999999,
                9999999999,
                "registrar1",
                250,
            )
            .unwrap();

        let del = store
            .get_active_delegation("alice", "nodns.shop")
            .unwrap()
            .unwrap();
        assert_eq!(del.renewal_price, 250);
    }

    #[test]
    fn save_delegation_backward_compat_price_zero() {
        let store = Store::new(":memory:", None).unwrap();
        store.init().unwrap();

        store
            .save_delegation(
                "event1",
                "alice",
                "nodns.shop",
                "npub1abc",
                "pubkey1",
                0,
                9999999999,
                9999999999,
                "registrar1",
            )
            .unwrap();

        let del = store
            .get_active_delegation("alice", "nodns.shop")
            .unwrap()
            .unwrap();
        assert_eq!(del.renewal_price, 0);
    }

    #[test]
    fn save_delegation_with_price_overwrites() {
        let store = Store::new(":memory:", None).unwrap();
        store.init().unwrap();

        store
            .save_delegation(
                "event1",
                "alice",
                "nodns.shop",
                "npub1abc",
                "pubkey1",
                0,
                9999999999,
                9999999999,
                "registrar1",
            )
            .unwrap();

        store
            .save_delegation_with_price(
                "event2",
                "alice",
                "nodns.shop",
                "npub1abc",
                "pubkey1",
                0,
                9999999999,
                9999999999,
                "registrar1",
                500,
            )
            .unwrap();

        let del = store
            .get_active_delegation("alice", "nodns.shop")
            .unwrap()
            .unwrap();
        assert_eq!(del.event_id, "event2");
        assert_eq!(del.renewal_price, 500);
    }

    #[test]
    fn save_delegation_defaults_to_active_status() {
        let store = Store::new(":memory:", None).unwrap();
        store.init().unwrap();

        store
            .save_delegation(
                "event1",
                "alice",
                "nodns.shop",
                "npub1abc",
                "pubkey1",
                0,
                9999999999,
                9999999999,
                "registrar1",
            )
            .unwrap();

        let del = store
            .get_delegation("alice", "nodns.shop")
            .unwrap()
            .unwrap();
        assert_eq!(del.status, "active");
    }

    #[test]
    fn mark_delegation_grace_updates_status() {
        let store = Store::new(":memory:", None).unwrap();
        store.init().unwrap();

        store
            .save_delegation(
                "event1",
                "alice",
                "nodns.shop",
                "npub1abc",
                "pubkey1",
                0,
                1,
                1,
                "registrar1",
            )
            .unwrap();

        store.mark_delegation_grace("alice", "nodns.shop").unwrap();

        let del = store
            .get_delegation("alice", "nodns.shop")
            .unwrap()
            .unwrap();
        assert_eq!(del.status, "grace");
    }

    #[test]
    fn mark_delegation_expired_updates_status() {
        let store = Store::new(":memory:", None).unwrap();
        store.init().unwrap();

        store
            .save_delegation(
                "event1",
                "alice",
                "nodns.shop",
                "npub1abc",
                "pubkey1",
                0,
                1,
                1,
                "registrar1",
            )
            .unwrap();

        store.mark_delegation_grace("alice", "nodns.shop").unwrap();
        store
            .mark_delegation_expired("alice", "nodns.shop")
            .unwrap();

        let del = store
            .get_delegation("alice", "nodns.shop")
            .unwrap()
            .unwrap();
        assert_eq!(del.status, "expired");
    }

    #[test]
    fn is_name_available_when_grace_delegation() {
        let store = Store::new(":memory:", None).unwrap();
        store.init().unwrap();

        store
            .save_delegation(
                "event1",
                "alice",
                "nodns.shop",
                "npub1abc",
                "pubkey1",
                0,
                1,
                1,
                "registrar1",
            )
            .unwrap();

        store.mark_delegation_grace("alice", "nodns.shop").unwrap();

        assert!(!store.is_name_available("alice", "nodns.shop").unwrap());
    }

    #[test]
    fn is_name_available_when_status_expired() {
        let store = Store::new(":memory:", None).unwrap();
        store.init().unwrap();

        store
            .save_delegation(
                "event1",
                "alice",
                "nodns.shop",
                "npub1abc",
                "pubkey1",
                0,
                9999999999,
                9999999999,
                "registrar1",
            )
            .unwrap();

        store.mark_delegation_grace("alice", "nodns.shop").unwrap();
        store
            .mark_delegation_expired("alice", "nodns.shop")
            .unwrap();

        assert!(store.is_name_available("alice", "nodns.shop").unwrap());
    }

    #[test]
    fn get_active_delegation_excludes_grace() {
        let store = Store::new(":memory:", None).unwrap();
        store.init().unwrap();

        store
            .save_delegation(
                "event1",
                "alice",
                "nodns.shop",
                "npub1abc",
                "pubkey1",
                0,
                9999999999,
                9999999999,
                "registrar1",
            )
            .unwrap();

        store.mark_delegation_grace("alice", "nodns.shop").unwrap();

        assert!(store
            .get_active_delegation("alice", "nodns.shop")
            .unwrap()
            .is_none());
    }

    #[test]
    fn get_delegation_returns_any_status() {
        let store = Store::new(":memory:", None).unwrap();
        store.init().unwrap();

        store
            .save_delegation(
                "event1",
                "alice",
                "nodns.shop",
                "npub1abc",
                "pubkey1",
                0,
                9999999999,
                9999999999,
                "registrar1",
            )
            .unwrap();

        store.mark_delegation_grace("alice", "nodns.shop").unwrap();

        let del = store
            .get_delegation("alice", "nodns.shop")
            .unwrap()
            .unwrap();
        assert_eq!(del.status, "grace");
    }

    #[test]
    fn get_delegations_past_valid_until_returns_only_past() {
        let store = Store::new(":memory:", None).unwrap();
        store.init().unwrap();

        store
            .save_delegation(
                "event1",
                "alice",
                "nodns.shop",
                "npub1abc",
                "pubkey1",
                0,
                1,
                1,
                "registrar1",
            )
            .unwrap();

        store
            .save_delegation(
                "event2",
                "bob",
                "nodns.shop",
                "npub1def",
                "pubkey2",
                0,
                9999999999,
                9999999999,
                "registrar1",
            )
            .unwrap();

        let past = store.get_delegations_past_valid_until().unwrap();
        assert_eq!(past.len(), 1);
        assert_eq!(past[0].domain, "alice");
    }

    #[test]
    fn get_delegations_past_valid_until_excludes_already_expired() {
        let store = Store::new(":memory:", None).unwrap();
        store.init().unwrap();

        store
            .save_delegation(
                "event1",
                "alice",
                "nodns.shop",
                "npub1abc",
                "pubkey1",
                0,
                1,
                1,
                "registrar1",
            )
            .unwrap();

        store.mark_delegation_grace("alice", "nodns.shop").unwrap();
        store
            .mark_delegation_expired("alice", "nodns.shop")
            .unwrap();

        let past = store.get_delegations_past_valid_until().unwrap();
        assert!(past.is_empty());
    }

    // ── Renewal store tests ──

    #[test]
    fn renew_delegation_updates_valid_until() {
        let store = Store::new(":memory:", None).unwrap();
        store.init().unwrap();

        store
            .save_delegation_with_price(
                "event1",
                "alice",
                "nodns.shop",
                "npub1abc",
                "pubkey1",
                1000,
                2000,
                1700,
                "registrar1",
                250,
            )
            .unwrap();

        store
            .renew_delegation("alice", "nodns.shop", 3000, 2700, "event2")
            .unwrap();

        let del = store
            .get_delegation("alice", "nodns.shop")
            .unwrap()
            .unwrap();
        assert_eq!(del.valid_until, 3000);
        assert_eq!(del.renew_by, 2700);
        assert_eq!(del.event_id, "event2");
        assert_eq!(del.status, "active");
        assert_eq!(del.renewal_price, 250);
    }

    #[test]
    fn renew_delegation_resets_grace_to_active() {
        let store = Store::new(":memory:", None).unwrap();
        store.init().unwrap();

        store
            .save_delegation_with_price(
                "event1",
                "alice",
                "nodns.shop",
                "npub1abc",
                "pubkey1",
                1000,
                2000,
                1700,
                "registrar1",
                250,
            )
            .unwrap();

        store.mark_delegation_grace("alice", "nodns.shop").unwrap();
        let del = store
            .get_delegation("alice", "nodns.shop")
            .unwrap()
            .unwrap();
        assert_eq!(del.status, "grace");

        store
            .renew_delegation("alice", "nodns.shop", 3000, 2700, "event2")
            .unwrap();

        let del = store
            .get_delegation("alice", "nodns.shop")
            .unwrap()
            .unwrap();
        assert_eq!(del.status, "active");
        assert_eq!(del.valid_until, 3000);
    }

    #[test]
    fn renew_delegation_preserves_owner() {
        let store = Store::new(":memory:", None).unwrap();
        store.init().unwrap();

        store
            .save_delegation_with_price(
                "event1",
                "alice",
                "nodns.shop",
                "npub1abc",
                "pubkey1",
                1000,
                2000,
                1700,
                "registrar1",
                250,
            )
            .unwrap();

        store
            .renew_delegation("alice", "nodns.shop", 3000, 2700, "event2")
            .unwrap();

        let del = store
            .get_delegation("alice", "nodns.shop")
            .unwrap()
            .unwrap();
        assert_eq!(del.npub, "npub1abc");
        assert_eq!(del.pubkey, "pubkey1");
    }
}
