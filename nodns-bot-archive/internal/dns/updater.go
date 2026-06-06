package dns

import (
	"fmt"
	"log/slog"
	"time"

	"github.com/miekg/dns"
	"nodns-bot/internal/config"
)

// Updater sends DDNS (RFC 2136) updates to Knot DNS via TSIG-signed messages.
type Updater struct {
	knotAddr string
	zone     string
	tsigKey  string
	tsigAlg  string
	client   *dns.Client
	logger   *slog.Logger
}

// NewUpdater creates a new DDNS updater.
func NewUpdater(cfg config.ZoneConfig, logger *slog.Logger) *Updater {
	// miekg/dns requires TSIG key name to be fully qualified (trailing dot)
	// for DNS wire format packing. The TsigSecret map key must match exactly.
	tsigKeyName := dns.Fqdn(cfg.TSIGKeyName)
	tsigAlg := cfg.TSIGAlgorithm
	if !dns.IsFqdn(tsigAlg) {
		tsigAlg = tsigAlg + "."
	}

	return &Updater{
		knotAddr: cfg.KnotAddress,
		zone:     dns.Fqdn(cfg.Zone),
		tsigKey:  tsigKeyName,
		tsigAlg:  tsigAlg,
		client: &dns.Client{
			Net:          "tcp",
			ReadTimeout:  5 * time.Second,
			WriteTimeout: 5 * time.Second,
			TsigSecret: map[string]string{
				tsigKeyName: cfg.TSIGSecret,
			},
		},
		logger: logger.With("component", "dns-updater"),
	}
}

// makeRemoveRR creates a minimal RR used only for RemoveRRset (just needs name + type).
func makeRemoveRR(fqdn string, recordType uint16) dns.RR {
	return &dns.RR_Header{
		Name:   fqdn,
		Rrtype: recordType,
		Class:  dns.ClassINET,
	}
}

// UpdateRecord removes existing records for the name+type and adds the new record.
func (u *Updater) UpdateRecord(fqdn string, ttl uint32, recordType uint16, rdata string) error {
	if !dns.IsFqdn(fqdn) {
		fqdn = dns.Fqdn(fqdn)
	}

	msg := new(dns.Msg)
	msg.SetUpdate(u.zone)
	msg.Id = dns.Id()

	removeRR := makeRemoveRR(fqdn, recordType)
	msg.RemoveRRset([]dns.RR{removeRR})

	typeStr := dns.TypeToString[recordType]
	zoneLine := fmt.Sprintf("%s %d IN %s %s", fqdn, ttl, typeStr, rdata)
	rr, err := dns.NewRR(zoneLine)
	if err != nil {
		return fmt.Errorf("failed to parse RR %q: %w", zoneLine, err)
	}

	msg.Insert([]dns.RR{rr})

	u.logger.Debug("sending DDNS update", "fqdn", fqdn, "type", typeStr, "ttl", ttl, "rdata", rdata)

	if err := u.sendDDNS(msg); err != nil {
		return fmt.Errorf("DDNS update failed for %s: %w", fqdn, err)
	}

	u.logger.Info("DDNS update applied", "fqdn", fqdn, "type", typeStr)
	return nil
}

// DeleteRecord removes all records of a given type at the given FQDN.
func (u *Updater) DeleteRecord(fqdn string, recordType uint16) error {
	if !dns.IsFqdn(fqdn) {
		fqdn = dns.Fqdn(fqdn)
	}

	msg := new(dns.Msg)
	msg.SetUpdate(u.zone)
	msg.Id = dns.Id()

	removeRR := makeRemoveRR(fqdn, recordType)
	msg.RemoveRRset([]dns.RR{removeRR})

	if err := u.sendDDNS(msg); err != nil {
		return fmt.Errorf("DDNS delete failed for %s: %w", fqdn, err)
	}

	u.logger.Info("DDNS delete applied", "fqdn", fqdn)
	return nil
}

func (u *Updater) sendDDNS(msg *dns.Msg) error {
	msg.SetTsig(u.tsigKey, u.tsigAlg, 300, time.Now().Unix())

	resp, _, err := u.client.Exchange(msg, u.knotAddr)
	if err != nil {
		return fmt.Errorf("DDNS exchange: %w", err)
	}

	if resp.Rcode != dns.RcodeSuccess {
		return fmt.Errorf("server rejected: %s", dns.RcodeToString[resp.Rcode])
	}
	return nil
}

// TestConnection verifies connectivity to Knot DNS by sending a SOA query.
func (u *Updater) TestConnection() error {
	msg := new(dns.Msg)
	msg.SetQuestion(u.zone, dns.TypeSOA)
	msg.RecursionDesired = false

	resp, _, err := u.client.Exchange(msg, u.knotAddr)
	if err != nil {
		return fmt.Errorf("connection test failed: %w", err)
	}

	if resp.Rcode != dns.RcodeSuccess {
		return fmt.Errorf("connection test returned: %s", dns.RcodeToString[resp.Rcode])
	}

	u.logger.Info("Knot DNS connection test passed", "addr", u.knotAddr)
	return nil
}

// TypeStringToUint converts a DNS record type string to its uint16 constant.
func TypeStringToUint(t string) uint16 {
	switch t {
	case "A":
		return dns.TypeA
	case "AAAA":
		return dns.TypeAAAA
	case "CNAME":
		return dns.TypeCNAME
	case "TXT":
		return dns.TypeTXT
	case "MX":
		return dns.TypeMX
	case "SRV":
		return dns.TypeSRV
	case "NS":
		return dns.TypeNS
	case "PTR":
		return dns.TypePTR
	default:
		return dns.TypeNone
	}
}
