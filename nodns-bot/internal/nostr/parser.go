package nostr

import (
	"errors"
	"fmt"
	"net"
	"strconv"
	"strings"

	"github.com/nbd-wtf/go-nostr"
)

const KindDNSRecord = 11111

const DefaultTTL uint32 = 3600

// DNSRecord represents a parsed DNS record from a Nostr record tag.
type DNSRecord struct {
	Type  string // DNS record type: A, AAAA, CNAME, TXT, MX, SRV
	Name  string // Record name: "@" for root, subdomain name
	TTL   uint32 // TTL in seconds
	RData string // Zone-file format rdata string
}

// Delegation represents a parsed delegation tag.
type Delegation struct {
	Domain    string // e.g., "alice.cv"
	Npub      string // npub receiving delegation
	ValidFrom int64  // unix timestamp
	ValidUntil int64 // unix timestamp
	RenewBy   int64  // unix timestamp
}

// RegistrarKey represents a parsed registrar key publication.
type RegistrarKey struct {
	Zone      string // e.g., "cv", "nodns.shop"
	PubkeyHex string // nostr pubkey hex
}

// Payment represents a parsed payment proof tag.
type Payment struct {
	Method  string // "cashu" or "zap"
	Token   string // cashu token or zap receipt event ID
	MintURL string // cashu mint URL (empty for zap)
	Amount  int64  // sats
}

// ParsedEvent contains all parsed data from a kind 11111 event.
type ParsedEvent struct {
	Records    []DNSRecord
	Delegation *Delegation   // nil if not a delegation event
	Registrar  *RegistrarKey // nil if not a registrar event
	Payments   []Payment
}

// privateNets are private/reserved IP networks that should be blocked.
var privateNets = []net.IPNet{
	parseCIDR("10.0.0.0/8"),
	parseCIDR("172.16.0.0/12"),
	parseCIDR("192.168.0.0/16"),
	parseCIDR("127.0.0.0/8"),
	parseCIDR("169.254.0.0/16"),
	parseCIDR("0.0.0.0/8"),
	parseCIDR("100.64.0.0/10"),
	parseCIDR("fc00::/7"),
	parseCIDR("fe80::/10"),
	parseCIDR("::1/128"),
}

func parseCIDR(s string) net.IPNet {
	_, n, err := net.ParseCIDR(s)
	if err != nil {
		panic(err)
	}
	return *n
}

// ClassifyEvent parses all tags and classifies the event.
// Content is allowed to be non-empty (it can be a description string).
func ClassifyEvent(event *nostr.Event, allowedTypes []string, blockPrivateIP bool, maxTXTLength int) (*ParsedEvent, error) {
	if event == nil {
		return nil, errors.New("nil event")
	}
	if event.Kind != KindDNSRecord {
		return nil, fmt.Errorf("expected kind %d, got %d", KindDNSRecord, event.Kind)
	}

	result := &ParsedEvent{}

	for i, tag := range event.Tags {
		if len(tag) == 0 {
			continue
		}
		switch tag[0] {
		case "record":
			rec, err := ParseRecordTag(tag, allowedTypes, blockPrivateIP, maxTXTLength)
			if err != nil {
				return nil, fmt.Errorf("tag %d: %w", i, err)
			}
			result.Records = append(result.Records, rec)

		case "delegation":
			if result.Delegation != nil {
				return nil, fmt.Errorf("tag %d: duplicate delegation tag", i)
			}
			d, err := ParseDelegationTag(tag)
			if err != nil {
				return nil, fmt.Errorf("tag %d: %w", i, err)
			}
			result.Delegation = &d

		case "registrar":
			if result.Registrar != nil {
				return nil, fmt.Errorf("tag %d: duplicate registrar tag", i)
			}
			r, err := ParseRegistrarTag(tag)
			if err != nil {
				return nil, fmt.Errorf("tag %d: %w", i, err)
			}
			result.Registrar = &r
		}
	}

	payments, err := ParsePaymentTags(event.Tags)
	if err != nil {
		return nil, err
	}
	result.Payments = payments

	if len(result.Records) == 0 && result.Delegation == nil && result.Registrar == nil {
		return nil, errors.New("no recognized tags found (need record, delegation, or registrar)")
	}

	return result, nil
}

// ParseEvent parses a kind 11111 Nostr event and extracts DNS records.
// This is the backward-compatible entry point that enforces the original
// content-must-be-empty constraint. New callers should use ClassifyEvent.
func ParseEvent(event *nostr.Event, allowedTypes []string, blockPrivateIP bool, maxTXTLength int) ([]DNSRecord, error) {
	if event.Content != "" {
		return nil, errors.New("content must be empty string")
	}
	parsed, err := ClassifyEvent(event, allowedTypes, blockPrivateIP, maxTXTLength)
	if err != nil {
		return nil, err
	}
	if len(parsed.Records) == 0 {
		return nil, errors.New("no record tags found")
	}
	return parsed.Records, nil
}

// ParseDelegationTag parses a delegation tag.
// Format: ["delegation", DOMAIN, NPUB, VALID_FROM, VALID_UNTIL, RENEW_BY]
func ParseDelegationTag(tag nostr.Tag) (Delegation, error) {
	if len(tag) < 6 {
		return Delegation{}, fmt.Errorf("delegation tag must have 6 elements, got %d", len(tag))
	}
	if tag[0] != "delegation" {
		return Delegation{}, errors.New("first element must be 'delegation'")
	}
	if tag[1] == "" {
		return Delegation{}, errors.New("delegation domain cannot be empty")
	}
	if tag[2] == "" {
		return Delegation{}, errors.New("delegation npub cannot be empty")
	}

	validFrom, err := strconv.ParseInt(tag[3], 10, 64)
	if err != nil {
		return Delegation{}, fmt.Errorf("invalid valid_from %q: %w", tag[3], err)
	}
	validUntil, err := strconv.ParseInt(tag[4], 10, 64)
	if err != nil {
		return Delegation{}, fmt.Errorf("invalid valid_until %q: %w", tag[4], err)
	}
	renewBy, err := strconv.ParseInt(tag[5], 10, 64)
	if err != nil {
		return Delegation{}, fmt.Errorf("invalid renew_by %q: %w", tag[5], err)
	}

	return Delegation{
		Domain:     tag[1],
		Npub:       tag[2],
		ValidFrom:  validFrom,
		ValidUntil: validUntil,
		RenewBy:    renewBy,
	}, nil
}

// ParseRegistrarTag parses a registrar key publication tag.
// Format: ["registrar", ZONE, PUBKEY_HEX]
func ParseRegistrarTag(tag nostr.Tag) (RegistrarKey, error) {
	if len(tag) < 3 {
		return RegistrarKey{}, fmt.Errorf("registrar tag must have 3 elements, got %d", len(tag))
	}
	if tag[0] != "registrar" {
		return RegistrarKey{}, errors.New("first element must be 'registrar'")
	}
	if tag[1] == "" {
		return RegistrarKey{}, errors.New("registrar zone cannot be empty")
	}
	if tag[2] == "" {
		return RegistrarKey{}, errors.New("registrar pubkey hex cannot be empty")
	}
	return RegistrarKey{
		Zone:      tag[1],
		PubkeyHex: tag[2],
	}, nil
}

// ParsePaymentTags parses all cashu and zap payment tags from the event.
func ParsePaymentTags(tags nostr.Tags) ([]Payment, error) {
	var payments []Payment
	for _, tag := range tags {
		if len(tag) < 3 {
			continue
		}
		switch tag[0] {
		case "cashu":
			if len(tag) < 4 {
				return nil, fmt.Errorf("cashu tag must have 4 elements, got %d", len(tag))
			}
			amount, err := strconv.ParseInt(tag[3], 10, 64)
			if err != nil {
				return nil, fmt.Errorf("invalid cashu amount %q: %w", tag[3], err)
			}
			payments = append(payments, Payment{
				Method:  "cashu",
				Token:   tag[1],
				MintURL: tag[2],
				Amount:  amount,
			})

		case "zap":
			if len(tag) < 3 {
				return nil, fmt.Errorf("zap tag must have at least 3 elements, got %d", len(tag))
			}
			amount, err := strconv.ParseInt(tag[2], 10, 64)
			if err != nil {
				return nil, fmt.Errorf("invalid zap amount %q: %w", tag[2], err)
			}
			payments = append(payments, Payment{
				Method: "zap",
				Token:  tag[1],
				Amount: amount,
			})
		}
	}
	return payments, nil
}

// ParseRecordTag parses a single record tag in either 5-element or 11-element format.
func ParseRecordTag(tag nostr.Tag, allowedTypes []string, blockPrivateIP bool, maxTXTLength int) (DNSRecord, error) {
	if len(tag) == 0 || tag[0] != "record" {
		return DNSRecord{}, errors.New("first element must be 'record'")
	}

	allowedSet := make(map[string]bool, len(allowedTypes))
	for _, t := range allowedTypes {
		allowedSet[strings.ToUpper(t)] = true
	}

	switch len(tag) {
	case 5:
		return parseNewFormat(tag, allowedSet, blockPrivateIP, maxTXTLength)
	case 11:
		return parseLegacyFormat(tag, allowedSet, blockPrivateIP, maxTXTLength)
	default:
		return DNSRecord{}, fmt.Errorf("record tag must have 5 or 11 elements, got %d", len(tag))
	}
}

// parseNewFormat handles ["record", "TYPE", "name", "TTL", "rdata"].
func parseNewFormat(tag nostr.Tag, allowedTypes map[string]bool, blockPrivateIP bool, maxTXTLength int) (DNSRecord, error) {
	rtype := strings.ToUpper(tag[1])
	if rtype == "" {
		return DNSRecord{}, errors.New("record type cannot be empty")
	}
	if len(allowedTypes) > 0 && !allowedTypes[rtype] {
		return DNSRecord{}, fmt.Errorf("record type %q not allowed", rtype)
	}

	name := tag[2]
	if name == "" {
		name = "@"
	}

	rdata := tag[4]

	ttl := DefaultTTL
	if tag[3] != "" {
		parsed, err := strconv.ParseUint(tag[3], 10, 32)
		if err != nil {
			return DNSRecord{}, fmt.Errorf("invalid TTL %q: %w", tag[3], err)
		}
		ttl = uint32(parsed)
	}
	if ttl == 0 {
		ttl = DefaultTTL
	}

	rec := DNSRecord{
		Type:  rtype,
		Name:  name,
		RData: rdata,
		TTL:   ttl,
	}

	if err := validateRecord(rec, blockPrivateIP, maxTXTLength); err != nil {
		return DNSRecord{}, err
	}

	return rec, nil
}

// parseLegacyFormat handles 11-element format:
// ["record", "TYPE", "name", "pos1", "pos2", "pos3", "pos4", "pos5", "pos6", "pos7", "ttl"]
func parseLegacyFormat(tag nostr.Tag, allowedTypes map[string]bool, blockPrivateIP bool, maxTXTLength int) (DNSRecord, error) {
	rtype := strings.ToUpper(tag[1])
	if rtype == "" {
		return DNSRecord{}, errors.New("record type cannot be empty")
	}
	if len(allowedTypes) > 0 && !allowedTypes[rtype] {
		return DNSRecord{}, fmt.Errorf("record type %q not allowed", rtype)
	}

	name := tag[2]
	if name == "" {
		name = "@"
	}

	// Reconstruct rdata from positions 3-9 (indices 3..9) by joining non-empty values
	var rdataParts []string
	for i := 3; i <= 9; i++ {
		if tag[i] != "" {
			rdataParts = append(rdataParts, tag[i])
		}
	}
	rdata := strings.Join(rdataParts, " ")

	// TTL from position 10 (index 10)
	ttl := DefaultTTL
	if tag[10] != "" {
		parsed, err := strconv.ParseUint(tag[10], 10, 32)
		if err != nil {
			return DNSRecord{}, fmt.Errorf("invalid TTL %q: %w", tag[10], err)
		}
		ttl = uint32(parsed)
	}
	if ttl == 0 {
		ttl = DefaultTTL
	}

	rec := DNSRecord{
		Type:  rtype,
		Name:  name,
		RData: rdata,
		TTL:   ttl,
	}

	if err := validateRecord(rec, blockPrivateIP, maxTXTLength); err != nil {
		return DNSRecord{}, err
	}

	return rec, nil
}

// validateRecord performs type-specific validation on a parsed record.
func validateRecord(rec DNSRecord, blockPrivateIP bool, maxTXTLength int) error {
	if rec.RData == "" && rec.Type != "TXT" {
		return fmt.Errorf("%s record requires rdata", rec.Type)
	}

	if rec.Type == "TXT" && maxTXTLength > 0 && len(rec.RData) > maxTXTLength {
		return fmt.Errorf("TXT record exceeds max length %d: got %d", maxTXTLength, len(rec.RData))
	}

	fields := strings.Fields(rec.RData)

	switch rec.Type {
	case "A":
		ip := net.ParseIP(rec.RData)
		if ip == nil || ip.To4() == nil {
			return fmt.Errorf("invalid IPv4 address: %s", rec.RData)
		}
		if blockPrivateIP && isPrivateIP(ip) {
			return fmt.Errorf("private IP address blocked: %s", rec.RData)
		}

	case "AAAA":
		ip := net.ParseIP(rec.RData)
		if ip == nil {
			return fmt.Errorf("invalid IPv6 address: %s", rec.RData)
		}
		if blockPrivateIP && isPrivateIP(ip) {
			return fmt.Errorf("private IP address blocked: %s", rec.RData)
		}

	case "CNAME", "NS", "PTR":
		if rec.RData == "" {
			return fmt.Errorf("%s record requires target domain", rec.Type)
		}

	case "TXT":
		// Any content allowed, length already checked above

	case "MX":
		if len(fields) < 2 {
			return errors.New("MX record requires: priority mailserver")
		}
		if _, err := strconv.ParseUint(fields[0], 10, 16); err != nil {
			return fmt.Errorf("invalid MX priority: %s", fields[0])
		}

	case "SRV":
		if len(fields) < 4 {
			return errors.New("SRV record requires: priority weight port target")
		}
		for i, fieldName := range []string{"priority", "weight", "port"} {
			if _, err := strconv.ParseUint(fields[i], 10, 16); err != nil {
				return fmt.Errorf("invalid SRV %s: %s", fieldName, fields[i])
			}
		}

	default:
		return fmt.Errorf("unsupported record type: %s", rec.Type)
	}

	return nil
}

// isPrivateIP checks if an IP is in a private/reserved range.
func isPrivateIP(ip net.IP) bool {
	for _, network := range privateNets {
		if network.Contains(ip) {
			return true
		}
	}
	return false
}
