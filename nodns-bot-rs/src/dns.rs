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
