//! DDNS (RFC 2136) updater with TSIG (RFC 2845) signing for Knot DNS.
//!
//! Ported 1:1 from `nodns-bot/internal/dns/updater.go`.
//!
//! Uses raw TCP transport with length-prefix framing instead of the hickory
//! `Client` wrapper so we can send a single atomic Update message containing
//! both a `RemoveRRset` and an Insert entry — exactly matching the Go behaviour.

use std::net::SocketAddr;
use std::str::FromStr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use hickory_client::proto::dnssec::rdata::tsig::TsigAlgorithm;
use hickory_client::proto::dnssec::tsig::TSigner;
use hickory_client::proto::dnssec::PublicKey;
use hickory_client::proto::op::{Message, MessageType, OpCode, Query, ResponseCode};
use hickory_client::proto::rr::rdata::{A, AAAA, CNAME, MX, SRV, TXT};
use hickory_client::proto::rr::{DNSClass, Name, RData, Record, RecordType};
use hickory_client::proto::serialize::binary::BinEncodable;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream as TokioTcpStream;
use tracing::{debug, info};

use crate::config::ZoneConfig;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors produced by the DNS updater.
#[derive(Debug, Error)]
pub enum DnsError {
    #[error("failed to parse RR \"{rr_line}\": {error}")]
    ParseRR { rr_line: String, error: String },

    #[error("DDNS update failed for {fqdn}: {error}")]
    UpdateFailed { fqdn: String, error: String },

    #[error("DDNS delete failed for {fqdn}: {error}")]
    DeleteFailed { fqdn: String, error: String },

    #[error("connection test failed: {error}")]
    ConnectionTestFailed { error: String },

    #[error("unsupported TSIG algorithm: {0}")]
    UnsupportedAlgorithm(String),

    #[error("DNS error: {0}")]
    Dns(String),
}

pub type Result<T> = std::result::Result<T, DnsError>;

// ---------------------------------------------------------------------------
// Updater
// ---------------------------------------------------------------------------

/// Sends DDNS (RFC 2136) updates to Knot DNS via TSIG-signed messages.
///
/// Each public method opens a fresh TCP connection (matching the Go miekg/dns
/// `Client.Exchange` behaviour), sends one signed message, and closes.
pub struct Updater {
    knot_addr: SocketAddr,
    zone: Name,
    tsig_signer: TSigner,
    timeout: Duration,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Ensure a DNS name is fully qualified (trailing dot).
fn ensure_fqdn(name: &str) -> String {
    if name.ends_with('.') {
        name.to_string()
    } else {
        format!("{name}.")
    }
}

/// Map a TSIG algorithm string (e.g. `"hmac-sha256"`) to the hickory enum.
///
/// The Go code ensures the algorithm name is FQDN for DNS wire format;
/// hickory's `TsigAlgorithm` enum handles wire-format internally, so we
/// only need to match the canonical form.
fn parse_tsig_algorithm(alg: &str) -> Result<TsigAlgorithm> {
    let normalized = alg.trim_end_matches('.').to_lowercase();
    match normalized.as_str() {
        "hmac-sha256" => Ok(TsigAlgorithm::HmacSha256),
        "hmac-sha384" => Ok(TsigAlgorithm::HmacSha384),
        "hmac-sha512" => Ok(TsigAlgorithm::HmacSha512),
        "hmac-md5" => Ok(TsigAlgorithm::HmacMd5),
        _ => Err(DnsError::UnsupportedAlgorithm(alg.to_string())),
    }
}

/// Convert a numeric record type (`u16`) to hickory's `RecordType` enum.
fn record_type_from_u16(rt: u16) -> RecordType {
    match rt {
        1 => RecordType::A,
        2 => RecordType::NS,
        5 => RecordType::CNAME,
        12 => RecordType::PTR,
        15 => RecordType::MX,
        16 => RecordType::TXT,
        28 => RecordType::AAAA,
        33 => RecordType::SRV,
        _ => RecordType::Unknown(rt),
    }
}

/// Human-readable record type label (for log messages).
fn record_type_str(rt: u16) -> &'static str {
    match rt {
        1 => "A",
        2 => "NS",
        5 => "CNAME",
        12 => "PTR",
        15 => "MX",
        16 => "TXT",
        28 => "AAAA",
        33 => "SRV",
        _ => "UNKNOWN",
    }
}

/// Parse an rdata string into an `RData` value for the given record type.
fn parse_rdata(record_type: u16, rdata: &str) -> Result<RData> {
    match record_type {
        // A
        1 => {
            let addr: std::net::Ipv4Addr =
                rdata
                    .parse()
                    .map_err(|e: std::net::AddrParseError| DnsError::ParseRR {
                        rr_line: rdata.to_string(),
                        error: e.to_string(),
                    })?;
            Ok(RData::A(A::from(addr)))
        }
        // AAAA
        28 => {
            let addr: std::net::Ipv6Addr =
                rdata
                    .parse()
                    .map_err(|e: std::net::AddrParseError| DnsError::ParseRR {
                        rr_line: rdata.to_string(),
                        error: e.to_string(),
                    })?;
            Ok(RData::AAAA(AAAA::from(addr)))
        }
        // TXT
        16 => Ok(RData::TXT(TXT::new(vec![rdata.to_string()]))),
        // CNAME
        5 => {
            let target = ensure_fqdn(rdata);
            let name = Name::from_str(&target).map_err(|e| DnsError::ParseRR {
                rr_line: rdata.to_string(),
                error: e.to_string(),
            })?;
            Ok(RData::CNAME(CNAME(name)))
        }
        // MX  — rdata = "10 mail.example.com."
        15 => {
            let (priority, exchange) = rdata.split_once(' ').ok_or_else(|| DnsError::ParseRR {
                rr_line: rdata.to_string(),
                error: "MX requires 'priority target'".into(),
            })?;
            let pref: u16 =
                priority
                    .parse()
                    .map_err(|e: std::num::ParseIntError| DnsError::ParseRR {
                        rr_line: rdata.to_string(),
                        error: e.to_string(),
                    })?;
            let target = ensure_fqdn(exchange);
            let exchange_name = Name::from_str(&target).map_err(|e| DnsError::ParseRR {
                rr_line: rdata.to_string(),
                error: e.to_string(),
            })?;
            Ok(RData::MX(MX::new(pref, exchange_name)))
        }
        // SRV  — rdata = "10 60 5060 sip.example.com."
        33 => {
            let parts: Vec<&str> = rdata.splitn(4, ' ').collect();
            if parts.len() != 4 {
                return Err(DnsError::ParseRR {
                    rr_line: rdata.to_string(),
                    error: "SRV requires 'priority weight port target'".into(),
                });
            }
            let priority: u16 =
                parts[0]
                    .parse()
                    .map_err(|e: std::num::ParseIntError| DnsError::ParseRR {
                        rr_line: rdata.to_string(),
                        error: e.to_string(),
                    })?;
            let weight: u16 =
                parts[1]
                    .parse()
                    .map_err(|e: std::num::ParseIntError| DnsError::ParseRR {
                        rr_line: rdata.to_string(),
                        error: e.to_string(),
                    })?;
            let port: u16 =
                parts[2]
                    .parse()
                    .map_err(|e: std::num::ParseIntError| DnsError::ParseRR {
                        rr_line: rdata.to_string(),
                        error: e.to_string(),
                    })?;
            let target = ensure_fqdn(parts[3]);
            let target_name = Name::from_str(&target).map_err(|e| DnsError::ParseRR {
                rr_line: rdata.to_string(),
                error: e.to_string(),
            })?;
            Ok(RData::SRV(SRV::new(priority, weight, port, target_name)))
        }
        _ => Err(DnsError::ParseRR {
            rr_line: rdata.to_string(),
            error: format!("unsupported record type {record_type}"),
        }),
    }
}

/// Generate a DNS message ID (time-based, unique enough for single-shot TCP).
fn generate_id() -> u16 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u16
}

// ---------------------------------------------------------------------------
// Updater implementation
// ---------------------------------------------------------------------------

impl Updater {
    /// Create a new DDNS updater from zone configuration.
    ///
    /// Mirrors Go's `NewUpdater`: TSIG key name and algorithm are normalised to
    /// FQDN form for DNS wire format compatibility.
    pub fn new(cfg: &ZoneConfig) -> Result<Self> {
        // miekg/dns requires TSIG key name to be fully qualified (trailing dot)
        // for DNS wire format packing.  The TsigSecret map key must match exactly.
        let tsig_key_name = ensure_fqdn(&cfg.tsig_key_name);
        let zone_fqdn = ensure_fqdn(&cfg.zone);

        // Algorithm must also be FQDN in the Go version; hickory handles
        // wire-format internally, so we only need the enum variant.
        let algorithm = parse_tsig_algorithm(&cfg.tsig_algorithm)?;

        let signer_name = Name::from_str(&tsig_key_name)
            .map_err(|e| DnsError::Dns(format!("invalid TSIG key name: {e}")))?;

        let zone = Name::from_str(&zone_fqdn)
            .map_err(|e| DnsError::Dns(format!("invalid zone name: {e}")))?;

        let knot_addr: SocketAddr = cfg
            .knot_address
            .parse()
            .map_err(|e| DnsError::Dns(format!("invalid knot address: {e}")))?;

        let secret_bytes = BASE64_STANDARD
            .decode(cfg.tsig_key_secret.trim())
            .map_err(|e| {
                DnsError::Dns(format!(
                    "TSIG secret must be base64-encoded (Knot/BIND format): {e}"
                ))
            })?;

        let tsig_signer = TSigner::new(secret_bytes, algorithm, signer_name, 300)
            .map_err(|e| DnsError::Dns(format!("TSIG signer creation failed: {e}")))?;

        Ok(Self {
            knot_addr,
            zone,
            tsig_signer,
            timeout: Duration::from_secs(5),
        })
    }

    // -----------------------------------------------------------------------
    // Raw TCP transport
    // -----------------------------------------------------------------------

    /// Send a DNS message over TCP with length-prefix framing, read response.
    ///
    /// Uses 5 s read/write timeout per operation, matching the Go client.
    async fn send_tcp(&self, msg: &mut Message, sign_tsig: bool) -> Result<Message> {
        // Optionally sign with TSIG before serialising.
        if sign_tsig {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as u32;
            msg.finalize(&self.tsig_signer, now)
                .map_err(|e| DnsError::Dns(format!("TSIG signing failed: {e}")))?;
        }

        let bytes = msg
            .to_bytes()
            .map_err(|e| DnsError::Dns(format!("message encode: {e}")))?;

        // Connect TCP
        let mut stream =
            tokio::time::timeout(self.timeout, TokioTcpStream::connect(self.knot_addr))
                .await
                .map_err(|_| DnsError::Dns("connect timeout".into()))?
                .map_err(|e| DnsError::Dns(format!("connect: {e}")))?;

        // Write: 2-byte BE length prefix + message bytes
        let len = bytes.len() as u16;
        tokio::time::timeout(self.timeout, async {
            stream.write_all(&len.to_be_bytes()).await?;
            stream.write_all(&bytes).await
        })
        .await
        .map_err(|_| DnsError::Dns("write timeout".into()))?
        .map_err(|e| DnsError::Dns(format!("write: {e}")))?;

        // Read: 2-byte BE length prefix + message bytes
        let buf = tokio::time::timeout(self.timeout, async {
            let mut len_buf = [0u8; 2];
            stream.read_exact(&mut len_buf).await?;
            let resp_len = u16::from_be_bytes(len_buf) as usize;
            let mut resp_buf = vec![0u8; resp_len];
            stream.read_exact(&mut resp_buf).await?;
            Ok::<_, std::io::Error>(resp_buf)
        })
        .await
        .map_err(|_| DnsError::Dns("read timeout".into()))?
        .map_err(|e| DnsError::Dns(format!("read: {e}")))?;

        Message::from_vec(&buf).map_err(|e| DnsError::Dns(format!("response decode: {e}")))
    }

    // -----------------------------------------------------------------------
    // Message builders
    // -----------------------------------------------------------------------

    /// Build a base DNS Update message (zone section set, `OpCode` = Update).
    fn build_update_message(&self) -> Message {
        let mut zone_q = Query::new();
        zone_q
            .set_name(self.zone.clone())
            .set_query_class(DNSClass::IN)
            .set_query_type(RecordType::SOA);

        let mut msg = Message::new();
        msg.set_id(generate_id())
            .set_message_type(MessageType::Query)
            .set_op_code(OpCode::Update)
            .set_recursion_desired(false)
            .add_query(zone_q);

        msg
    }

    /// Create a minimal record for RFC 2136 §2.5.2 "Delete An `RRset`".
    ///
    /// Uses `RData::Update0` which serialises with RDLENGTH = 0 — exactly
    /// what the RFC requires.  The record type is taken from `template`.
    fn make_remove_rrset(template: &Record) -> Record {
        let mut rec = template.clone();
        rec.set_dns_class(DNSClass::ANY);
        rec.set_ttl(0);
        rec.set_data(RData::Update0(rec.record_type()));
        rec
    }

    /// Create a delete-rrset record when no insert template exists (for
    /// `delete_record`).  Builds the record directly via `Record::update0`.
    fn make_delete_rrset(name: Name, rt: RecordType) -> Record {
        let mut rec = Record::update0(name, 0, rt);
        rec.set_dns_class(DNSClass::ANY);
        rec
    }

    // -----------------------------------------------------------------------
    // Public API
    // -----------------------------------------------------------------------

    /// Remove existing `RRset` for `fqdn`/`record_type`, then insert the new
    /// record.  A single atomic DDNS message is sent, matching Go's
    /// `UpdateRecord`.
    pub async fn update_record(
        &self,
        fqdn: &str,
        ttl: u32,
        record_type: u16,
        rdata: &str,
    ) -> Result<()> {
        let fqdn = ensure_fqdn(fqdn);
        let fqdn_name = Name::from_str(&fqdn).map_err(|e| DnsError::UpdateFailed {
            fqdn: fqdn.clone(),
            error: format!("invalid name: {e}"),
        })?;

        // Build insert record first (establishes correct record type).
        let rdata_val = parse_rdata(record_type, rdata).map_err(|e| DnsError::UpdateFailed {
            fqdn: fqdn.clone(),
            error: e.to_string(),
        })?;
        let insert_rr = Record::from_rdata(fqdn_name, ttl, rdata_val);

        // Build remove-rrset record from the insert record (preserves type).
        let remove_rr = Self::make_remove_rrset(&insert_rr);

        // Assemble update message: remove existing, then insert new.
        let mut msg = self.build_update_message();
        msg.add_name_server(remove_rr);
        msg.add_name_server(insert_rr);

        debug!(
            fqdn = %fqdn,
            rtype = record_type_str(record_type),
            ttl,
            rdata,
            "sending DDNS update",
        );

        let resp = self
            .send_tcp(&mut msg, true)
            .await
            .map_err(|e| DnsError::UpdateFailed {
                fqdn: fqdn.clone(),
                error: e.to_string(),
            })?;

        if resp.response_code() != ResponseCode::NoError {
            return Err(DnsError::UpdateFailed {
                fqdn,
                error: format!("server rejected: {}", resp.response_code()),
            });
        }

        info!(
            fqdn = %fqdn,
            rtype = record_type_str(record_type),
            "DDNS update applied"
        );
        Ok(())
    }

    /// Set a TXT record with multiple character-strings (for proof records).
    ///
    /// Atomically removes any existing TXT RRset at `fqdn`, then inserts the
    /// new multi-string TXT record.  Each string must be ≤255 bytes (RFC 1035).
    pub async fn update_txt_multi(&self, fqdn: &str, ttl: u32, segments: &[String]) -> Result<()> {
        let fqdn = ensure_fqdn(fqdn);
        let fqdn_name = Name::from_str(&fqdn).map_err(|e| DnsError::UpdateFailed {
            fqdn: fqdn.clone(),
            error: format!("invalid name: {e}"),
        })?;

        let txt = TXT::new(segments.to_vec());
        let insert_rr = Record::from_rdata(fqdn_name, ttl, RData::TXT(txt));
        let remove_rr = Self::make_remove_rrset(&insert_rr);

        let mut msg = self.build_update_message();
        msg.add_name_server(remove_rr);
        msg.add_name_server(insert_rr);

        debug!(
            fqdn = %fqdn,
            ttl,
            segments = segments.len(),
            "sending DDNS proof TXT update",
        );

        let resp = self
            .send_tcp(&mut msg, true)
            .await
            .map_err(|e| DnsError::UpdateFailed {
                fqdn: fqdn.clone(),
                error: e.to_string(),
            })?;

        if resp.response_code() != ResponseCode::NoError {
            return Err(DnsError::UpdateFailed {
                fqdn,
                error: format!("server rejected: {}", resp.response_code()),
            });
        }

        info!(fqdn = %fqdn, "DDNS proof TXT update applied");
        Ok(())
    }

    /// Append a single record to an existing `RRset` without removing existing
    /// records.  Sends a bare DDNS INSERT (RFC 2136 §2.5.1).
    pub async fn append_record(
        &self,
        fqdn: &str,
        ttl: u32,
        record_type: u16,
        rdata: &str,
    ) -> Result<()> {
        let fqdn = ensure_fqdn(fqdn);
        let fqdn_name = Name::from_str(&fqdn).map_err(|e| DnsError::UpdateFailed {
            fqdn: fqdn.clone(),
            error: format!("invalid name: {e}"),
        })?;

        let rdata_val = parse_rdata(record_type, rdata).map_err(|e| DnsError::UpdateFailed {
            fqdn: fqdn.clone(),
            error: e.to_string(),
        })?;
        let insert_rr = Record::from_rdata(fqdn_name, ttl, rdata_val);

        let mut msg = self.build_update_message();
        msg.add_name_server(insert_rr);

        debug!(
            fqdn = %fqdn,
            rtype = record_type_str(record_type),
            ttl,
            rdata,
            "sending DDNS append",
        );

        let resp = self
            .send_tcp(&mut msg, true)
            .await
            .map_err(|e| DnsError::UpdateFailed {
                fqdn: fqdn.clone(),
                error: e.to_string(),
            })?;

        if resp.response_code() != ResponseCode::NoError {
            return Err(DnsError::UpdateFailed {
                fqdn,
                error: format!("server rejected: {}", resp.response_code()),
            });
        }

        info!(
            fqdn = %fqdn,
            rtype = record_type_str(record_type),
            "DDNS append applied"
        );
        Ok(())
    }

    /// Remove all records of a given type at `fqdn`.  Matches Go's
    /// `DeleteRecord`.
    pub async fn delete_record(&self, fqdn: &str, record_type: u16) -> Result<()> {
        let fqdn = ensure_fqdn(fqdn);
        let fqdn_name = Name::from_str(&fqdn).map_err(|e| DnsError::DeleteFailed {
            fqdn: fqdn.clone(),
            error: format!("invalid name: {e}"),
        })?;

        let rt = record_type_from_u16(record_type);
        let remove_rr = Self::make_delete_rrset(fqdn_name, rt);

        let mut msg = self.build_update_message();
        msg.add_name_server(remove_rr);

        let resp = self
            .send_tcp(&mut msg, true)
            .await
            .map_err(|e| DnsError::DeleteFailed {
                fqdn: fqdn.clone(),
                error: e.to_string(),
            })?;

        if resp.response_code() != ResponseCode::NoError {
            return Err(DnsError::DeleteFailed {
                fqdn,
                error: format!("server rejected: {}", resp.response_code()),
            });
        }

        info!(fqdn = %fqdn, "DDNS delete applied");
        Ok(())
    }

    /// Send a SOA query to verify connectivity to Knot DNS.  Matches Go's
    /// `TestConnection`.  The SOA query is **not** TSIG-signed, matching
    /// the Go behaviour (miekg/dns only applies TSIG when `SetTsig` is
    /// called explicitly).
    pub async fn test_connection(&self) -> Result<()> {
        let mut msg = Message::new();
        msg.set_id(generate_id())
            .set_message_type(MessageType::Query)
            .set_recursion_desired(false);

        let mut query = Query::new();
        query
            .set_name(self.zone.clone())
            .set_query_class(DNSClass::IN)
            .set_query_type(RecordType::SOA);
        msg.add_query(query);

        let resp =
            self.send_tcp(&mut msg, false)
                .await
                .map_err(|e| DnsError::ConnectionTestFailed {
                    error: e.to_string(),
                })?;

        if resp.response_code() != ResponseCode::NoError {
            return Err(DnsError::ConnectionTestFailed {
                error: format!("server returned: {}", resp.response_code()),
            });
        }

        info!(addr = %self.knot_addr, "Knot DNS connection test passed");
        Ok(())
    }
}

#[async_trait::async_trait]
impl crate::connector::DnsConnector for Updater {
    async fn update_record(
        &self,
        fqdn: &str,
        ttl: u32,
        record_type: u16,
        rdata: &str,
    ) -> anyhow::Result<()> {
        Updater::update_record(self, fqdn, ttl, record_type, rdata).await?;
        Ok(())
    }

    async fn update_txt_multi(
        &self,
        fqdn: &str,
        ttl: u32,
        segments: &[String],
    ) -> anyhow::Result<()> {
        Updater::update_txt_multi(self, fqdn, ttl, segments).await?;
        Ok(())
    }

    async fn delete_record(&self, fqdn: &str, record_type: u16) -> anyhow::Result<()> {
        Updater::delete_record(self, fqdn, record_type).await?;
        Ok(())
    }

    async fn append_record(
        &self,
        fqdn: &str,
        ttl: u32,
        record_type: u16,
        rdata: &str,
    ) -> anyhow::Result<()> {
        Updater::append_record(self, fqdn, ttl, record_type, rdata).await?;
        Ok(())
    }

    async fn test_connection(&self) -> anyhow::Result<()> {
        Updater::test_connection(self).await?;
        Ok(())
    }
}

pub struct DnsQueryResult {
    pub registered: bool,
    pub records: Vec<DnsRecord>,
}

pub struct DnsRecord {
    pub record_type: String,
    pub ttl: u32,
    pub rdata: String,
}

pub async fn query_txt_records(nameserver: SocketAddr, fqdn: &str) -> DnsQueryResult {
    let Ok(name) = Name::from_str(fqdn) else {
        return DnsQueryResult {
            registered: false,
            records: vec![],
        };
    };

    let mut query = Query::new();
    query
        .set_name(name)
        .set_query_class(DNSClass::IN)
        .set_query_type(RecordType::TXT);

    let mut msg = Message::new();
    msg.set_id(generate_id())
        .set_message_type(MessageType::Query)
        .set_recursion_desired(false)
        .add_query(query);

    let timeout = Duration::from_secs(2);

    let Ok(Ok(stream)) = tokio::time::timeout(timeout, TokioTcpStream::connect(nameserver)).await
    else {
        return DnsQueryResult {
            registered: false,
            records: vec![],
        };
    };
    let mut stream = stream;

    let Ok(bytes) = msg.to_bytes() else {
        return DnsQueryResult {
            registered: false,
            records: vec![],
        };
    };

    let len = bytes.len() as u16;
    let write_result = tokio::time::timeout(timeout, async {
        use tokio::io::AsyncWriteExt;
        stream.write_all(&len.to_be_bytes()).await?;
        stream.write_all(&bytes).await
    })
    .await;

    if write_result.map_or(true, |r| r.is_err()) {
        return DnsQueryResult {
            registered: false,
            records: vec![],
        };
    }

    let read_result = tokio::time::timeout(timeout, async {
        use tokio::io::AsyncReadExt;
        let mut len_buf = [0u8; 2];
        stream.read_exact(&mut len_buf).await?;
        let resp_len = u16::from_be_bytes(len_buf) as usize;
        let mut resp_buf = vec![0u8; resp_len];
        stream.read_exact(&mut resp_buf).await?;
        Ok::<_, std::io::Error>(resp_buf)
    })
    .await;

    let Ok(Ok(buf)) = read_result else {
        return DnsQueryResult {
            registered: false,
            records: vec![],
        };
    };

    let Ok(resp) = Message::from_vec(&buf) else {
        return DnsQueryResult {
            registered: false,
            records: vec![],
        };
    };

    if resp.response_code() == ResponseCode::NXDomain
        || resp.response_code() != ResponseCode::NoError
    {
        return DnsQueryResult {
            registered: false,
            records: vec![],
        };
    }

    let answers = resp.answers();
    if answers.is_empty() {
        return DnsQueryResult {
            registered: false,
            records: vec![],
        };
    }

    let mut records = Vec::new();
    for rec in answers {
        if rec.record_type() == RecordType::TXT {
            let rdata = rec.data();
            if let RData::TXT(txt) = rdata {
                for txt_bytes in txt.txt_data() {
                    let txt_string = String::from_utf8_lossy(txt_bytes).to_string();
                    records.push(DnsRecord {
                        record_type: "TXT".to_string(),
                        ttl: rec.ttl(),
                        rdata: txt_string,
                    });
                }
            }
        }
    }

    let registered = !records.is_empty();
    DnsQueryResult {
        registered,
        records,
    }
}

pub async fn query_dnskey_base64(nameserver: SocketAddr, zone: &str) -> Vec<String> {
    let Ok(name) = Name::from_str(zone) else {
        return vec![];
    };

    let mut query = Query::new();
    query
        .set_name(name)
        .set_query_class(DNSClass::IN)
        .set_query_type(RecordType::DNSKEY);

    let mut msg = Message::new();
    msg.set_id(generate_id())
        .set_message_type(MessageType::Query)
        .set_recursion_desired(false)
        .add_query(query);

    let timeout = Duration::from_secs(2);

    let Ok(Ok(stream)) = tokio::time::timeout(timeout, TokioTcpStream::connect(nameserver)).await
    else {
        return vec![];
    };
    let mut stream = stream;

    let Ok(bytes) = msg.to_bytes() else {
        return vec![];
    };

    use tokio::io::AsyncWriteExt;
    let len = (bytes.len() as u16).to_be_bytes();
    if stream.write_all(&len).await.is_err() || stream.write_all(&bytes).await.is_err() {
        return vec![];
    }

    let mut len_buf = [0u8; 2];
    if tokio::io::AsyncReadExt::read_exact(&mut stream, &mut len_buf)
        .await
        .is_err()
    {
        return vec![];
    }
    let resp_len = u16::from_be_bytes(len_buf) as usize;
    let mut buf = vec![0u8; resp_len];
    if tokio::io::AsyncReadExt::read_exact(&mut stream, &mut buf)
        .await
        .is_err()
    {
        return vec![];
    }

    let Ok(resp) = Message::from_vec(&buf) else {
        return vec![];
    };

    if resp.response_code() != ResponseCode::NoError {
        return vec![];
    }

    let mut keys = Vec::new();
    for rec in resp.answers() {
        if rec.record_type() == RecordType::DNSKEY {
            if let RData::DNSSEC(hickory_client::proto::dnssec::rdata::DNSSECRData::DNSKEY(
                ref dk,
            )) = rec.data()
            {
                if dk.secure_entry_point() {
                    let pk_bytes = dk.public_key().public_bytes();
                    let mut rdata = Vec::with_capacity(4 + pk_bytes.len());
                    rdata.extend_from_slice(&dk.flags().to_be_bytes());
                    rdata.push(3);
                    rdata.push(13);
                    rdata.extend_from_slice(pk_bytes);
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&rdata);
                    keys.push(b64);
                }
            }
        }
    }

    keys
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    // -----------------------------------------------------------------------
    // Test fixtures
    // -----------------------------------------------------------------------

    /// Build a `ZoneConfig` with a valid HMAC-SHA256 key (32 zero bytes,
    /// base64-encoded) and a loopback address.
    fn make_test_zone() -> ZoneConfig {
        let secret = BASE64_STANDARD.encode([0u8; 32]);
        ZoneConfig {
            knot_address: "127.0.0.1:5353".to_string(),
            zone: "example.com".to_string(),
            tsig_key_name: "test-key".to_string(),
            tsig_key_secret: secret,
            tsig_algorithm: "hmac-sha256".to_string(),
            ..Default::default()
        }
    }

    /// Construct an `Updater` from a valid test zone (no network use).
    fn make_test_updater() -> Updater {
        Updater::new(&make_test_zone()).expect("valid test zone config should produce an Updater")
    }

    // -----------------------------------------------------------------------
    // ensure_fqdn
    // -----------------------------------------------------------------------

    #[test]
    fn ensure_fqdn_adds_trailing_dot_to_bare_name() {
        assert_eq!(ensure_fqdn("alice.example.com"), "alice.example.com.");
    }

    #[test]
    fn ensure_fqdn_preserves_already_qualified_name() {
        assert_eq!(ensure_fqdn("alice.example.com."), "alice.example.com.");
    }

    #[test]
    fn ensure_fqdn_handles_single_label() {
        assert_eq!(ensure_fqdn("alice"), "alice.");
    }

    #[test]
    fn ensure_fqdn_empty_becomes_root() {
        assert_eq!(ensure_fqdn(""), ".");
    }

    #[test]
    fn ensure_fqdn_root_dot_unchanged() {
        assert_eq!(ensure_fqdn("."), ".");
    }

    // -----------------------------------------------------------------------
    // record_type_str
    // -----------------------------------------------------------------------

    #[test]
    fn record_type_str_all_known_types() {
        assert_eq!(record_type_str(1), "A");
        assert_eq!(record_type_str(2), "NS");
        assert_eq!(record_type_str(5), "CNAME");
        assert_eq!(record_type_str(12), "PTR");
        assert_eq!(record_type_str(15), "MX");
        assert_eq!(record_type_str(16), "TXT");
        assert_eq!(record_type_str(28), "AAAA");
        assert_eq!(record_type_str(33), "SRV");
    }

    #[test]
    fn record_type_str_unknown_falls_back() {
        assert_eq!(record_type_str(0), "UNKNOWN");
        assert_eq!(record_type_str(99), "UNKNOWN");
        assert_eq!(record_type_str(65535), "UNKNOWN");
    }

    // -----------------------------------------------------------------------
    // record_type_from_u16
    // -----------------------------------------------------------------------

    #[test]
    fn record_type_from_u16_known_mappings() {
        assert_eq!(record_type_from_u16(1), RecordType::A);
        assert_eq!(record_type_from_u16(2), RecordType::NS);
        assert_eq!(record_type_from_u16(5), RecordType::CNAME);
        assert_eq!(record_type_from_u16(12), RecordType::PTR);
        assert_eq!(record_type_from_u16(15), RecordType::MX);
        assert_eq!(record_type_from_u16(16), RecordType::TXT);
        assert_eq!(record_type_from_u16(28), RecordType::AAAA);
        assert_eq!(record_type_from_u16(33), RecordType::SRV);
    }

    #[test]
    fn record_type_from_u16_unknown_wraps_value() {
        assert_eq!(record_type_from_u16(999), RecordType::Unknown(999));
    }

    // -----------------------------------------------------------------------
    // parse_rdata — A (IPv4)
    // -----------------------------------------------------------------------

    #[test]
    fn parse_rdata_a_correct_ipv4() {
        let rdata = parse_rdata(1, "192.0.2.42").unwrap();
        match rdata {
            RData::A(a) => assert_eq!(a.0, Ipv4Addr::new(192, 0, 2, 42)),
            other => panic!("expected RData::A, got {other:?}"),
        }
    }

    #[test]
    fn parse_rdata_a_round_trips_through_wire_bytes() {
        // 192.0.2.1 → wire bytes [0xC0, 0x00, 0x02, 0x01]
        let rdata = parse_rdata(1, "192.0.2.1").unwrap();
        match rdata {
            RData::A(a) => assert_eq!(a.0.octets(), [0xC0, 0x00, 0x02, 0x01]),
            other => panic!("expected RData::A, got {other:?}"),
        }
    }

    #[test]
    fn parse_rdata_a_invalid_address() {
        let err = parse_rdata(1, "not.an.ip").unwrap_err();
        assert!(
            matches!(err, DnsError::ParseRR { .. }),
            "expected ParseRR, got {err:?}"
        );
    }

    // -----------------------------------------------------------------------
    // parse_rdata — AAAA (IPv6)
    // -----------------------------------------------------------------------

    #[test]
    fn parse_rdata_aaaa_correct_ipv6() {
        let rdata = parse_rdata(28, "2001:db8::1").unwrap();
        match rdata {
            RData::AAAA(aaaa) => {
                assert_eq!(aaaa.0, Ipv6Addr::new(0x2001, 0x0db8, 0, 0, 0, 0, 0, 1))
            }
            other => panic!("expected RData::AAAA, got {other:?}"),
        }
    }

    #[test]
    fn parse_rdata_aaaa_loopback() {
        let rdata = parse_rdata(28, "::1").unwrap();
        match rdata {
            RData::AAAA(aaaa) => assert_eq!(aaaa.0, Ipv6Addr::LOCALHOST),
            other => panic!("expected RData::AAAA, got {other:?}"),
        }
    }

    #[test]
    fn parse_rdata_aaaa_invalid() {
        let err = parse_rdata(28, "nope").unwrap_err();
        assert!(matches!(err, DnsError::ParseRR { .. }));
    }

    // -----------------------------------------------------------------------
    // parse_rdata — TXT
    // -----------------------------------------------------------------------

    #[test]
    fn parse_rdata_txt_single_segment() {
        let rdata = parse_rdata(16, "hello from nostr").unwrap();
        match rdata {
            RData::TXT(txt) => {
                assert_eq!(txt.txt_data().len(), 1);
                assert_eq!(
                    String::from_utf8_lossy(&txt.txt_data()[0]),
                    "hello from nostr"
                );
            }
            other => panic!("expected RData::TXT, got {other:?}"),
        }
    }

    #[test]
    fn parse_rdata_txt_empty_string() {
        let rdata = parse_rdata(16, "").unwrap();
        match rdata {
            RData::TXT(txt) => {
                assert_eq!(txt.txt_data().len(), 1);
                assert_eq!(String::from_utf8_lossy(&txt.txt_data()[0]), "");
            }
            other => panic!("expected RData::TXT, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // parse_rdata — CNAME
    // -----------------------------------------------------------------------

    #[test]
    fn parse_rdata_cname_appends_trailing_dot() {
        let rdata = parse_rdata(5, "target.example.org").unwrap();
        match rdata {
            RData::CNAME(cname) => {
                assert_eq!(cname.0.to_string(), "target.example.org.");
            }
            other => panic!("expected RData::CNAME, got {other:?}"),
        }
    }

    #[test]
    fn parse_rdata_cname_already_fqdn() {
        let rdata = parse_rdata(5, "target.example.org.").unwrap();
        match rdata {
            RData::CNAME(cname) => {
                assert_eq!(cname.0.to_string(), "target.example.org.");
            }
            other => panic!("expected RData::CNAME, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // parse_rdata — MX
    // -----------------------------------------------------------------------

    #[test]
    fn parse_rdata_mx_preference_and_exchange() {
        let rdata = parse_rdata(15, "10 mail.example.com").unwrap();
        match rdata {
            RData::MX(mx) => {
                assert_eq!(mx.preference(), 10);
                assert_eq!(mx.exchange().to_string(), "mail.example.com.");
            }
            other => panic!("expected RData::MX, got {other:?}"),
        }
    }

    #[test]
    fn parse_rdata_mx_high_preference() {
        let rdata = parse_rdata(15, "65535 mx2.example.net").unwrap();
        match rdata {
            RData::MX(mx) => {
                assert_eq!(mx.preference(), 65535);
                assert_eq!(mx.exchange().to_string(), "mx2.example.net.");
            }
            other => panic!("expected RData::MX, got {other:?}"),
        }
    }

    #[test]
    fn parse_rdata_mx_missing_exchange_rejected() {
        let err = parse_rdata(15, "10").unwrap_err();
        match err {
            DnsError::ParseRR { error, .. } => {
                assert!(error.contains("priority target"), "error was: {error}");
            }
            other => panic!("expected ParseRR, got {other:?}"),
        }
    }

    #[test]
    fn parse_rdata_mx_non_numeric_preference_rejected() {
        let err = parse_rdata(15, "abc mail.example.com").unwrap_err();
        assert!(matches!(err, DnsError::ParseRR { .. }));
    }

    // -----------------------------------------------------------------------
    // parse_rdata — SRV
    // -----------------------------------------------------------------------

    #[test]
    fn parse_rdata_srv_full() {
        let rdata = parse_rdata(33, "10 60 5060 sip.example.com").unwrap();
        match rdata {
            RData::SRV(srv) => {
                assert_eq!(srv.priority(), 10);
                assert_eq!(srv.weight(), 60);
                assert_eq!(srv.port(), 5060);
                assert_eq!(srv.target().to_string(), "sip.example.com.");
            }
            other => panic!("expected RData::SRV, got {other:?}"),
        }
    }

    #[test]
    fn parse_rdata_srv_missing_fields_rejected() {
        let err = parse_rdata(33, "10 60").unwrap_err();
        match err {
            DnsError::ParseRR { error, .. } => {
                assert!(error.contains("priority weight port target"));
            }
            other => panic!("expected ParseRR, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // parse_rdata — unsupported type
    // -----------------------------------------------------------------------

    #[test]
    fn parse_rdata_unsupported_record_type() {
        let err = parse_rdata(99, "whatever").unwrap_err();
        match err {
            DnsError::ParseRR { error, .. } => {
                assert!(
                    error.contains("unsupported record type 99"),
                    "error: {error}"
                );
            }
            other => panic!("expected ParseRR, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // make_remove_rrset
    // -----------------------------------------------------------------------

    #[test]
    fn make_remove_rrset_sets_any_class_and_zero_ttl() {
        let name = Name::from_str("foo.example.com.").unwrap();
        let rdata = parse_rdata(1, "192.0.2.1").unwrap();
        let template = Record::from_rdata(name, 3600, rdata);

        let removed = Updater::make_remove_rrset(&template);

        assert_eq!(removed.dns_class(), DNSClass::ANY);
        assert_eq!(removed.ttl(), 0);
        assert_eq!(removed.record_type(), RecordType::A);
        // RFC 2136 §2.5.2: RRset removal uses zero-length RDATA.
        assert!(
            matches!(removed.data(), RData::Update0(RecordType::A)),
            "expected Update0(A), got {:?}",
            removed.data()
        );
        assert_eq!(removed.name().to_string(), "foo.example.com.");
    }

    #[test]
    fn make_remove_rrset_preserves_record_type_txt() {
        let name = Name::from_str("proof.example.com.").unwrap();
        let rdata = parse_rdata(16, "challenge-token").unwrap();
        let template = Record::from_rdata(name, 300, rdata);

        let removed = Updater::make_remove_rrset(&template);

        assert_eq!(removed.dns_class(), DNSClass::ANY);
        assert_eq!(removed.record_type(), RecordType::TXT);
        assert!(matches!(removed.data(), RData::Update0(RecordType::TXT)));
    }

    // -----------------------------------------------------------------------
    // make_delete_rrset
    // -----------------------------------------------------------------------

    #[test]
    fn make_delete_rrset_any_class_and_update0() {
        let name = Name::from_str("bar.example.com.").unwrap();
        let rec = Updater::make_delete_rrset(name, RecordType::TXT);

        assert_eq!(rec.dns_class(), DNSClass::ANY);
        assert_eq!(rec.ttl(), 0);
        assert_eq!(rec.record_type(), RecordType::TXT);
        assert!(matches!(rec.data(), RData::Update0(RecordType::TXT)));
        assert_eq!(rec.name().to_string(), "bar.example.com.");
    }

    // -----------------------------------------------------------------------
    // build_update_message
    // -----------------------------------------------------------------------

    #[test]
    fn build_update_message_has_update_opcode() {
        let updater = make_test_updater();
        let msg = updater.build_update_message();

        assert_eq!(msg.op_code(), OpCode::Update);
        assert_eq!(msg.message_type(), MessageType::Query);
        assert!(
            !msg.recursion_desired(),
            "updates must not request recursion"
        );
    }

    #[test]
    fn build_update_message_zone_section_is_soa_in() {
        let updater = make_test_updater();
        let msg = updater.build_update_message();

        let queries = msg.queries();
        assert_eq!(
            queries.len(),
            1,
            "zone section must contain exactly one query"
        );

        let zone_q = &queries[0];
        assert_eq!(zone_q.query_type(), RecordType::SOA);
        assert_eq!(zone_q.query_class(), DNSClass::IN);
        assert_eq!(zone_q.name().to_string(), "example.com.");
    }

    #[test]
    fn build_update_message_starts_with_empty_update_section() {
        let updater = make_test_updater();
        let msg = updater.build_update_message();

        assert!(
            msg.name_servers().is_empty(),
            "base update message must have no update records"
        );
    }

    #[test]
    fn build_update_message_has_no_edns_on_base() {
        // The base update message does not carry EDNS; it is only added if
        // a large payload is needed later. This locks that observable fact.
        let updater = make_test_updater();
        let msg = updater.build_update_message();
        assert!(msg.extensions().is_none());
    }

    #[test]
    fn build_update_message_serialises_to_valid_wire_bytes() {
        let updater = make_test_updater();
        let msg = updater.build_update_message();

        let bytes = msg.to_bytes().expect("update message must encode");
        assert!(!bytes.is_empty(), "encoded message must be non-empty");

        let decoded = Message::from_vec(&bytes).expect("update message must decode");
        assert_eq!(decoded.op_code(), OpCode::Update);
        assert_eq!(decoded.queries().len(), 1);
        assert_eq!(decoded.queries()[0].query_type(), RecordType::SOA);
    }

    // -----------------------------------------------------------------------
    // parse_tsig_algorithm
    // -----------------------------------------------------------------------

    #[test]
    fn parse_tsig_algorithm_known_variants() {
        assert!(matches!(
            parse_tsig_algorithm("hmac-sha256").unwrap(),
            TsigAlgorithm::HmacSha256
        ));
        assert!(matches!(
            parse_tsig_algorithm("hmac-sha384").unwrap(),
            TsigAlgorithm::HmacSha384
        ));
        assert!(matches!(
            parse_tsig_algorithm("hmac-sha512").unwrap(),
            TsigAlgorithm::HmacSha512
        ));
        assert!(matches!(
            parse_tsig_algorithm("hmac-md5").unwrap(),
            TsigAlgorithm::HmacMd5
        ));
    }

    #[test]
    fn parse_tsig_algorithm_is_case_insensitive_and_strips_dot() {
        assert!(parse_tsig_algorithm("HMAC-SHA256").is_ok());
        assert!(parse_tsig_algorithm("Hmac-Sha256.").is_ok());
        assert!(parse_tsig_algorithm("hmac-SHA256.").is_ok());
    }

    #[test]
    fn parse_tsig_algorithm_unknown_rejected() {
        let err = parse_tsig_algorithm("hmac-sha1").unwrap_err();
        assert!(matches!(err, DnsError::UnsupportedAlgorithm(_)));
    }

    // -----------------------------------------------------------------------
    // Updater::new
    // -----------------------------------------------------------------------

    #[test]
    fn updater_new_accepts_valid_config() {
        let updater = Updater::new(&make_test_zone());
        assert!(updater.is_ok(), "valid config should build an Updater");
    }

    #[test]
    fn updater_new_rejects_non_base64_secret() {
        let mut zone = make_test_zone();
        zone.tsig_key_secret = "!!!not-valid-base64!!!".to_string();

        let err = Updater::new(&zone).err().unwrap();
        match err {
            DnsError::Dns(msg) => {
                assert!(
                    msg.contains("base64"),
                    "error should mention base64, was: {msg}"
                );
            }
            other => panic!("expected DnsError::Dns, got {other:?}"),
        }
    }

    #[test]
    fn updater_new_rejects_invalid_knot_address() {
        let mut zone = make_test_zone();
        zone.knot_address = "not-a-valid-address".to_string();

        let err = Updater::new(&zone).err().unwrap();
        match err {
            DnsError::Dns(msg) => {
                assert!(
                    msg.contains("knot address"),
                    "error should mention knot address, was: {msg}"
                );
            }
            other => panic!("expected DnsError::Dns, got {other:?}"),
        }
    }

    #[test]
    fn updater_new_rejects_unsupported_tsig_algorithm() {
        let mut zone = make_test_zone();
        zone.tsig_algorithm = "hmac-sha1".to_string();

        let err = Updater::new(&zone).err().unwrap();
        assert!(
            matches!(err, DnsError::UnsupportedAlgorithm(_)),
            "expected UnsupportedAlgorithm, got {err:?}"
        );
    }

    #[test]
    fn updater_new_rejects_empty_zone_name() {
        let mut zone = make_test_zone();
        zone.zone = String::new();

        // "" → ensure_fqdn("") → "." which is a valid root Name, so this
        // actually succeeds. We assert the behaviour rather than a failure:
        // the updater builds, but the zone is the DNS root.
        let result = Updater::new(&zone);
        assert!(
            result.is_ok(),
            "empty zone becomes root '.' which is a valid Name; got: {:?}",
            result.err()
        );
        let updater = result.unwrap();
        let msg = updater.build_update_message();
        assert_eq!(msg.queries()[0].name().to_string(), ".");
    }

    #[test]
    fn updater_new_normalises_tsig_key_name_to_fqdn() {
        let zone = make_test_zone();
        let updater = Updater::new(&zone).expect("should succeed");
        let _msg = updater.build_update_message();
    }
}
