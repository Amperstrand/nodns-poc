package store

import (
	"database/sql"
	"fmt"
	"log/slog"
	"strings"
	"time"

	_ "modernc.org/sqlite"
	"nodns-bot/internal/config"
)

// EventRecord represents a stored DNS event record.
type EventRecord struct {
	EventID     string
	Npub        string
	Pubkey      string
	Name        string
	RecordType  string
	TTL         uint32
	RData       string
	Zone        string
	CreatedAt   int64
	ProcessedAt int64
	Deleted     bool
}

// DelegationRecord represents a stored delegation.
type DelegationRecord struct {
	EventID         string
	Domain          string
	Zone            string
	Npub            string
	Pubkey          string
	ValidFrom       int64
	ValidUntil      int64
	RenewBy         int64
	RegistrarPubkey string
	CreatedAt       int64
	ProcessedAt     int64
}

// Store provides SQLite persistence for processed events.
type Store struct {
	db     *sql.DB
	logger *slog.Logger
}

// NewStore opens (or creates) the SQLite database.
func NewStore(cfg config.StoreConfig, logger *slog.Logger) (*Store, error) {
	db, err := sql.Open("sqlite", cfg.Path)
	if err != nil {
		return nil, fmt.Errorf("opening sqlite %s: %w", cfg.Path, err)
	}

	// Enable WAL mode for better concurrent read performance
	if _, err := db.Exec("PRAGMA journal_mode=WAL"); err != nil {
		db.Close()
		return nil, fmt.Errorf("setting WAL mode: %w", err)
	}

	return &Store{
		db:     db,
		logger: logger.With("component", "store"),
	}, nil
}

// Init creates the database schema.
func (s *Store) Init() error {
	schema := `
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
		PRIMARY KEY (event_id, record_type, name)
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
	`
	if _, err := s.db.Exec(schema); err != nil {
		return fmt.Errorf("creating schema: %w", err)
	}
	s.logger.Info("database initialized")
	return nil
}

// SaveEvent persists a processed event record.
func (s *Store) SaveEvent(eventID, npub, pubkey, name, recordType string, ttl uint32, rdata, zone string, createdAt int64) error {
	_, err := s.db.Exec(`
		INSERT OR REPLACE INTO events (event_id, npub, pubkey, name, record_type, ttl, rdata, zone, created_at)
		VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		eventID, npub, pubkey, name, recordType, ttl, rdata, zone, createdAt,
	)
	if err != nil {
		return fmt.Errorf("saving event %s: %w", eventID, err)
	}
	return nil
}

// MarkDeleted soft-deletes an event.
func (s *Store) MarkDeleted(eventID string) error {
	_, err := s.db.Exec(`UPDATE events SET deleted = 1 WHERE event_id = ?`, eventID)
	if err != nil {
		return fmt.Errorf("marking event %s deleted: %w", eventID, err)
	}
	return nil
}

// GetEvent retrieves a specific event by ID.
func (s *Store) GetEvent(eventID string) (*EventRecord, error) {
	row := s.db.QueryRow(`
		SELECT event_id, npub, pubkey, name, record_type, ttl, rdata, zone, created_at, processed_at, deleted
		FROM events WHERE event_id = ?`, eventID)

	var rec EventRecord
	var deleted int
	var processedAt sql.NullInt64

	err := row.Scan(&rec.EventID, &rec.Npub, &rec.Pubkey, &rec.Name, &rec.RecordType,
		&rec.TTL, &rec.RData, &rec.Zone, &rec.CreatedAt, &processedAt, &deleted)
	if err != nil {
		if err == sql.ErrNoRows {
			return nil, nil
		}
		return nil, fmt.Errorf("getting event %s: %w", eventID, err)
	}

	rec.ProcessedAt = processedAt.Int64
	rec.Deleted = deleted != 0

	return &rec, nil
}

// GetRecordsByPubkey returns all non-deleted records for a pubkey.
func (s *Store) GetRecordsByPubkey(pubkey string) ([]EventRecord, error) {
	rows, err := s.db.Query(`
		SELECT event_id, npub, pubkey, name, record_type, ttl, rdata, zone, created_at, processed_at, deleted
		FROM events WHERE pubkey = ? AND deleted = 0`, pubkey)
	if err != nil {
		return nil, fmt.Errorf("querying records for pubkey %s: %w", pubkey, err)
	}
	defer rows.Close()

	return scanRecords(rows)
}

// RecordCountByPubkey returns the number of active records for a pubkey.
func (s *Store) RecordCountByPubkey(pubkey string) (int, error) {
	var count int
	err := s.db.QueryRow(`SELECT COUNT(*) FROM events WHERE pubkey = ? AND deleted = 0`, pubkey).Scan(&count)
	if err != nil {
		return 0, fmt.Errorf("counting records for pubkey %s: %w", pubkey, err)
	}
	return count, nil
}

// EventsInLastMinute returns the number of events processed for a pubkey in the last 60 seconds.
func (s *Store) EventsInLastMinute(pubkey string) (int, error) {
	cutoff := time.Now().Unix() - 60
	var count int
	err := s.db.QueryRow(
		`SELECT COUNT(*) FROM events WHERE pubkey = ? AND processed_at > ?`, pubkey, cutoff,
	).Scan(&count)
	if err != nil {
		return 0, fmt.Errorf("counting recent events for pubkey %s: %w", pubkey, err)
	}
	return count, nil
}

// GetLastSeen returns the last_seen timestamp from meta.
func (s *Store) GetLastSeen() (int64, error) {
	var val string
	err := s.db.QueryRow(`SELECT value FROM meta WHERE key = 'last_seen'`).Scan(&val)
	if err != nil {
		if err == sql.ErrNoRows {
			return 0, nil
		}
		return 0, fmt.Errorf("getting last_seen: %w", err)
	}
	var ts int64
	fmt.Sscanf(val, "%d", &ts)
	return ts, nil
}

// SetLastSeen updates the last_seen timestamp in meta.
func (s *Store) SetLastSeen(ts int64) error {
	_, err := s.db.Exec(`INSERT OR REPLACE INTO meta (key, value) VALUES ('last_seen', ?)`,
		fmt.Sprintf("%d", ts))
	if err != nil {
		return fmt.Errorf("setting last_seen: %w", err)
	}
	return nil
}

// Close closes the database connection.
func (s *Store) Close() error {
	return s.db.Close()
}

// ListAllRecords returns all non-deleted records, ordered by created_at descending.
func (s *Store) ListAllRecords() ([]EventRecord, error) {
	rows, err := s.db.Query(`
		SELECT event_id, npub, pubkey, name, record_type, ttl, rdata, zone, created_at, processed_at, deleted
		FROM events WHERE deleted = 0
		ORDER BY created_at DESC`)
	if err != nil {
		return nil, fmt.Errorf("listing all records: %w", err)
	}
	defer rows.Close()
	return scanRecords(rows)
}

// SaveDelegation stores a delegation event.
func (s *Store) SaveDelegation(eventID, domain, zone, npub, pubkey string, validFrom, validUntil, renewBy int64, registrarPubkey string) error {
	_, err := s.db.Exec(`
		INSERT OR REPLACE INTO delegations (event_id, domain, zone, npub, pubkey, valid_from, valid_until, renew_by, registrar_pubkey, created_at)
		VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, unixepoch())`,
		eventID, domain, zone, npub, pubkey, validFrom, validUntil, renewBy, registrarPubkey,
	)
	if err != nil {
		return fmt.Errorf("saving delegation %s/%s: %w", domain, zone, err)
	}
	s.logger.Info("delegation saved", "domain", domain, "zone", zone, "npub", npub)
	return nil
}

// GetActiveDelegation returns the active delegation for a domain in a zone.
// Returns nil if no valid, non-expired delegation exists.
func (s *Store) GetActiveDelegation(domain, zone string) (*DelegationRecord, error) {
	now := time.Now().Unix()
	row := s.db.QueryRow(`
		SELECT event_id, domain, zone, npub, pubkey, valid_from, valid_until, renew_by, registrar_pubkey, created_at, processed_at
		FROM delegations
		WHERE domain = ? AND zone = ? AND valid_from <= ? AND valid_until > ?
		ORDER BY created_at DESC
		LIMIT 1`, domain, zone, now, now)

	var rec DelegationRecord
	err := row.Scan(&rec.EventID, &rec.Domain, &rec.Zone, &rec.Npub, &rec.Pubkey,
		&rec.ValidFrom, &rec.ValidUntil, &rec.RenewBy, &rec.RegistrarPubkey,
		&rec.CreatedAt, &rec.ProcessedAt)
	if err != nil {
		if err == sql.ErrNoRows {
			return nil, nil
		}
		return nil, fmt.Errorf("getting delegation %s/%s: %w", domain, zone, err)
	}
	return &rec, nil
}

// GetDelegationsByPubkey returns all active delegations for a npub.
func (s *Store) GetDelegationsByPubkey(npub string) ([]DelegationRecord, error) {
	now := time.Now().Unix()
	rows, err := s.db.Query(`
		SELECT event_id, domain, zone, npub, pubkey, valid_from, valid_until, renew_by, registrar_pubkey, created_at, processed_at
		FROM delegations
		WHERE npub = ? AND valid_from <= ? AND valid_until > ?
		ORDER BY created_at DESC`, npub, now, now)
	if err != nil {
		return nil, fmt.Errorf("querying delegations for npub %s: %w", npub, err)
	}
	defer rows.Close()

	var records []DelegationRecord
	for rows.Next() {
		var rec DelegationRecord
		err := rows.Scan(&rec.EventID, &rec.Domain, &rec.Zone, &rec.Npub, &rec.Pubkey,
			&rec.ValidFrom, &rec.ValidUntil, &rec.RenewBy, &rec.RegistrarPubkey,
			&rec.CreatedAt, &rec.ProcessedAt)
		if err != nil {
			return nil, fmt.Errorf("scanning delegation: %w", err)
		}
		records = append(records, rec)
	}
	return records, rows.Err()
}

// SaveRegistrarKey stores the registrar pubkey for a zone.
func (s *Store) SaveRegistrarKey(zone, pubkeyHex, npub, source, eventID string) error {
	_, err := s.db.Exec(`
		INSERT OR REPLACE INTO registrar_keys (zone, pubkey_hex, npub, source, event_id, updated_at)
		VALUES (?, ?, ?, ?, ?, unixepoch())`,
		zone, pubkeyHex, npub, source, eventID,
	)
	if err != nil {
		return fmt.Errorf("saving registrar key for %s: %w", zone, err)
	}
	s.logger.Info("registrar key saved", "zone", zone, "pubkey", pubkeyHex)
	return nil
}

// GetRegistrarKey returns the registrar pubkey hex for a zone.
// Returns empty string if no key is found.
func (s *Store) GetRegistrarKey(zone string) (string, error) {
	var pubkeyHex string
	err := s.db.QueryRow(`SELECT pubkey_hex FROM registrar_keys WHERE zone = ?`, zone).Scan(&pubkeyHex)
	if err != nil {
		if err == sql.ErrNoRows {
			return "", nil
		}
		return "", fmt.Errorf("getting registrar key for %s: %w", zone, err)
	}
	return pubkeyHex, nil
}

// HasRecord checks if a record already exists for the given npub+type+name+zone.
func (s *Store) HasRecord(npub, recordType, name, zone string) (bool, error) {
	var count int
	err := s.db.QueryRow(`
		SELECT COUNT(*) FROM events
		WHERE npub = ? AND record_type = ? AND name = ? AND zone = ? AND deleted = 0`,
		npub, recordType, name, zone,
	).Scan(&count)
	if err != nil {
		return false, fmt.Errorf("checking record existence: %w", err)
	}
	return count > 0, nil
}

// GetZoneForDomain determines which configured zone a domain belongs to.
// It checks if the domain ends with any of the zone suffixes.
func GetZoneForDomain(domain string, zones []string) (string, string, bool) {
	domain = strings.TrimSuffix(domain, ".")
	for _, zone := range zones {
		suffix := "." + zone
		if strings.HasSuffix(domain, suffix) {
			parts := strings.SplitN(domain, suffix, 2)
			return parts[0], zone, true
		}
		if domain == zone {
			return "", zone, true
		}
	}
	return "", "", false
}

func scanRecords(rows *sql.Rows) ([]EventRecord, error) {
	var records []EventRecord
	for rows.Next() {
		var rec EventRecord
		var deleted int
		var processedAt sql.NullInt64

		err := rows.Scan(&rec.EventID, &rec.Npub, &rec.Pubkey, &rec.Name, &rec.RecordType,
			&rec.TTL, &rec.RData, &rec.Zone, &rec.CreatedAt, &processedAt, &deleted)
		if err != nil {
			return nil, fmt.Errorf("scanning record: %w", err)
		}

		rec.ProcessedAt = processedAt.Int64
		rec.Deleted = deleted != 0
		records = append(records, rec)
	}
	return records, rows.Err()
}
