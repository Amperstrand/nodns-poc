package nostr

import (
	"testing"

	"github.com/nbd-wtf/go-nostr"
)

var defaultAllowed = []string{"A", "AAAA", "CNAME", "TXT", "MX", "SRV"}

func makeEvent(kind int, content string, tags nostr.Tags) *nostr.Event {
	return &nostr.Event{
		Kind:    kind,
		Content: content,
		Tags:    tags,
	}
}

func TestParseEvent_Kind(t *testing.T) {
	tags := nostr.Tags{
		{"record", "A", "@", "3600", "1.2.3.4"},
	}
	_, err := ParseEvent(makeEvent(99999, "", tags), nil, true, 512)
	if err == nil {
		t.Fatal("expected error for wrong kind")
	}
}

func TestParseEvent_NonEmptyContent(t *testing.T) {
	tags := nostr.Tags{
		{"record", "A", "@", "3600", "1.2.3.4"},
	}
	_, err := ParseEvent(makeEvent(KindDNSRecord, "hello", tags), nil, true, 512)
	if err == nil {
		t.Fatal("expected error for non-empty content")
	}
}

func TestParseEvent_NoRecordTags(t *testing.T) {
	tags := nostr.Tags{
		{"other", "value"},
	}
	_, err := ParseEvent(makeEvent(KindDNSRecord, "", tags), defaultAllowed, true, 512)
	if err == nil {
		t.Fatal("expected error for no record tags")
	}
}

// 5-element format tests

func TestParseNewFormat_A(t *testing.T) {
	rec, err := ParseRecordTag(
		nostr.Tag{"record", "A", "@", "3600", "1.2.3.4"},
		defaultAllowed, false, 512,
	)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if rec.Type != "A" {
		t.Errorf("Type = %q, want A", rec.Type)
	}
	if rec.Name != "@" {
		t.Errorf("Name = %q, want @", rec.Name)
	}
	if rec.TTL != 3600 {
		t.Errorf("TTL = %d, want 3600", rec.TTL)
	}
	if rec.RData != "1.2.3.4" {
		t.Errorf("RData = %q, want 1.2.3.4", rec.RData)
	}
}

func TestParseNewFormat_AAAA(t *testing.T) {
	rec, err := ParseRecordTag(
		nostr.Tag{"record", "AAAA", "@", "3600", "2001:db8::1"},
		defaultAllowed, false, 512,
	)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if rec.Type != "AAAA" {
		t.Errorf("Type = %q, want AAAA", rec.Type)
	}
	if rec.RData != "2001:db8::1" {
		t.Errorf("RData = %q, want 2001:db8::1", rec.RData)
	}
}

func TestParseNewFormat_CNAME(t *testing.T) {
	rec, err := ParseRecordTag(
		nostr.Tag{"record", "CNAME", "www", "3600", "example.com."},
		defaultAllowed, false, 512,
	)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if rec.Type != "CNAME" {
		t.Errorf("Type = %q, want CNAME", rec.Type)
	}
	if rec.Name != "www" {
		t.Errorf("Name = %q, want www", rec.Name)
	}
}

func TestParseNewFormat_TXT(t *testing.T) {
	rec, err := ParseRecordTag(
		nostr.Tag{"record", "TXT", "@", "3600", "v=spf1 include:_spf.google.com ~all"},
		defaultAllowed, false, 512,
	)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if rec.Type != "TXT" {
		t.Errorf("Type = %q, want TXT", rec.Type)
	}
}

func TestParseNewFormat_MX(t *testing.T) {
	rec, err := ParseRecordTag(
		nostr.Tag{"record", "MX", "@", "3600", "10 mail.example.com."},
		defaultAllowed, false, 512,
	)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if rec.Type != "MX" {
		t.Errorf("Type = %q, want MX", rec.Type)
	}
	if rec.RData != "10 mail.example.com." {
		t.Errorf("RData = %q, want 10 mail.example.com.", rec.RData)
	}
}

func TestParseNewFormat_SRV(t *testing.T) {
	rec, err := ParseRecordTag(
		nostr.Tag{"record", "SRV", "_sip._tcp", "3600", "10 5 5060 sip.example.com."},
		defaultAllowed, false, 512,
	)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if rec.Type != "SRV" {
		t.Errorf("Type = %q, want SRV", rec.Type)
	}
}

// 11-element legacy format tests

func TestParseLegacyFormat_A(t *testing.T) {
	rec, err := ParseRecordTag(
		nostr.Tag{"record", "A", "@", "192.168.1.1", "", "", "", "", "", "", "3600"},
		defaultAllowed, false, 512,
	)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if rec.Type != "A" {
		t.Errorf("Type = %q, want A", rec.Type)
	}
	if rec.RData != "192.168.1.1" {
		t.Errorf("RData = %q, want 192.168.1.1", rec.RData)
	}
	if rec.TTL != 3600 {
		t.Errorf("TTL = %d, want 3600", rec.TTL)
	}
}

func TestParseLegacyFormat_MX(t *testing.T) {
	rec, err := ParseRecordTag(
		nostr.Tag{"record", "MX", "@", "10", "mail.example.com", "", "", "", "", "", "3600"},
		defaultAllowed, false, 512,
	)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if rec.Type != "MX" {
		t.Errorf("Type = %q, want MX", rec.Type)
	}
	if rec.RData != "10 mail.example.com" {
		t.Errorf("RData = %q, want '10 mail.example.com'", rec.RData)
	}
}

func TestParseLegacyFormat_SRV(t *testing.T) {
	rec, err := ParseRecordTag(
		nostr.Tag{"record", "SRV", "_sip._tcp", "10", "5", "5060", "sip.example.com", "", "", "", "3600"},
		defaultAllowed, false, 512,
	)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if rec.Type != "SRV" {
		t.Errorf("Type = %q, want SRV", rec.Type)
	}
	if rec.RData != "10 5 5060 sip.example.com" {
		t.Errorf("RData = %q, want '10 5 5060 sip.example.com'", rec.RData)
	}
}

// Validation tests

func TestInvalidType(t *testing.T) {
	_, err := ParseRecordTag(
		nostr.Tag{"record", "FAKE", "@", "3600", "value"},
		defaultAllowed, false, 512,
	)
	if err == nil {
		t.Fatal("expected error for unsupported type")
	}
}

func TestInvalidIPv4(t *testing.T) {
	_, err := ParseRecordTag(
		nostr.Tag{"record", "A", "@", "3600", "not.an.ip"},
		defaultAllowed, false, 512,
	)
	if err == nil {
		t.Fatal("expected error for invalid IPv4")
	}
}

func TestInvalidIPv6(t *testing.T) {
	_, err := ParseRecordTag(
		nostr.Tag{"record", "AAAA", "@", "3600", "not:::ipv6:::address"},
		defaultAllowed, false, 512,
	)
	if err == nil {
		t.Fatal("expected error for invalid IPv6")
	}
}

func TestPrivateIP_Blocked(t *testing.T) {
	tests := []struct {
		name string
		ip   string
	}{
		{"RFC1918-10", "10.0.0.1"},
		{"RFC1918-172", "172.16.0.1"},
		{"RFC1918-192", "192.168.1.1"},
		{"loopback", "127.0.0.1"},
		{"link-local", "169.254.1.1"},
		{"zero", "0.0.0.1"},
		{"CGN", "100.64.0.1"},
		{"ULA-IPv6", "fc00::1"},
		{"link-local-IPv6", "fe80::1"},
		{"loopback-IPv6", "::1"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			rtype := "A"
			if len(tt.ip) > 15 || containsColon(tt.ip) {
				rtype = "AAAA"
			}
			_, err := ParseRecordTag(
				nostr.Tag{"record", rtype, "@", "3600", tt.ip},
				defaultAllowed, true, 512,
			)
			if err == nil {
				t.Errorf("expected private IP %s to be blocked", tt.ip)
			}
		})
	}
}

func TestPrivateIP_Allowed(t *testing.T) {
	_, err := ParseRecordTag(
		nostr.Tag{"record", "A", "@", "3600", "192.168.1.1"},
		defaultAllowed, false, 512,
	)
	if err != nil {
		t.Fatalf("private IP should be allowed when blockPrivateIP=false: %v", err)
	}
}

func TestMX_MissingFields(t *testing.T) {
	_, err := ParseRecordTag(
		nostr.Tag{"record", "MX", "@", "3600", "10"},
		defaultAllowed, false, 512,
	)
	if err == nil {
		t.Fatal("expected error for MX with only priority")
	}
}

func TestMX_InvalidPriority(t *testing.T) {
	_, err := ParseRecordTag(
		nostr.Tag{"record", "MX", "@", "3600", "abc mail.example.com."},
		defaultAllowed, false, 512,
	)
	if err == nil {
		t.Fatal("expected error for invalid MX priority")
	}
}

func TestSRV_MissingFields(t *testing.T) {
	_, err := ParseRecordTag(
		nostr.Tag{"record", "SRV", "@", "3600", "10 5 5060"},
		defaultAllowed, false, 512,
	)
	if err == nil {
		t.Fatal("expected error for SRV with only 3 fields")
	}
}

func TestSRV_InvalidPort(t *testing.T) {
	_, err := ParseRecordTag(
		nostr.Tag{"record", "SRV", "@", "3600", "10 5 abc sip.example.com."},
		defaultAllowed, false, 512,
	)
	if err == nil {
		t.Fatal("expected error for invalid SRV port")
	}
}

func TestEmptyTTL_Defaults(t *testing.T) {
	rec, err := ParseRecordTag(
		nostr.Tag{"record", "A", "@", "", "1.2.3.4"},
		defaultAllowed, false, 512,
	)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if rec.TTL != DefaultTTL {
		t.Errorf("TTL = %d, want default %d", rec.TTL, DefaultTTL)
	}
}

func TestZeroTTL_Defaults(t *testing.T) {
	rec, err := ParseRecordTag(
		nostr.Tag{"record", "A", "@", "0", "1.2.3.4"},
		defaultAllowed, false, 512,
	)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if rec.TTL != DefaultTTL {
		t.Errorf("TTL = %d, want default %d", rec.TTL, DefaultTTL)
	}
}

func TestEmptyName_Defaults(t *testing.T) {
	rec, err := ParseRecordTag(
		nostr.Tag{"record", "A", "", "3600", "1.2.3.4"},
		defaultAllowed, false, 512,
	)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if rec.Name != "@" {
		t.Errorf("Name = %q, want @", rec.Name)
	}
}

func TestTXT_LengthLimit(t *testing.T) {
	longTXT := make([]byte, 600)
	for i := range longTXT {
		longTXT[i] = 'a'
	}
	_, err := ParseRecordTag(
		nostr.Tag{"record", "TXT", "@", "3600", string(longTXT)},
		defaultAllowed, false, 512,
	)
	if err == nil {
		t.Fatal("expected error for TXT exceeding max length")
	}
}

func TestTXT_UnderLimit(t *testing.T) {
	txt := make([]byte, 500)
	for i := range txt {
		txt[i] = 'a'
	}
	_, err := ParseRecordTag(
		nostr.Tag{"record", "TXT", "@", "3600", string(txt)},
		defaultAllowed, false, 512,
	)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestInvalidElementCount(t *testing.T) {
	_, err := ParseRecordTag(
		nostr.Tag{"record", "A", "@", "3600"},
		defaultAllowed, false, 512,
	)
	if err == nil {
		t.Fatal("expected error for 4-element tag")
	}
}

func TestParseEvent_Full5Element(t *testing.T) {
	tags := nostr.Tags{
		{"record", "A", "@", "3600", "1.2.3.4"},
		{"record", "AAAA", "@", "3600", "2001:db8::1"},
		{"record", "TXT", "@", "3600", "hello"},
	}
	records, err := ParseEvent(makeEvent(KindDNSRecord, "", tags), defaultAllowed, false, 512)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(records) != 3 {
		t.Fatalf("got %d records, want 3", len(records))
	}
}

func TestParseEvent_Full11Element(t *testing.T) {
	tags := nostr.Tags{
		{"record", "A", "@", "1.2.3.4", "", "", "", "", "", "", "3600"},
		{"record", "MX", "@", "10", "mail.example.com", "", "", "", "", "", "3600"},
	}
	records, err := ParseEvent(makeEvent(KindDNSRecord, "", tags), defaultAllowed, false, 512)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(records) != 2 {
		t.Fatalf("got %d records, want 2", len(records))
	}
	if records[0].RData != "1.2.3.4" {
		t.Errorf("first record RData = %q, want 1.2.3.4", records[0].RData)
	}
	if records[1].RData != "10 mail.example.com" {
		t.Errorf("second record RData = %q, want '10 mail.example.com'", records[1].RData)
	}
}

func TestDisallowedType(t *testing.T) {
	allowed := []string{"A"}
	_, err := ParseRecordTag(
		nostr.Tag{"record", "AAAA", "@", "3600", "2001:db8::1"},
		allowed, false, 512,
	)
	if err == nil {
		t.Fatal("expected error for disallowed type")
	}
}

func TestCNAME_EmptyTarget(t *testing.T) {
	_, err := ParseRecordTag(
		nostr.Tag{"record", "CNAME", "www", "3600", ""},
		defaultAllowed, false, 512,
	)
	if err == nil {
		t.Fatal("expected error for CNAME with empty target")
	}
}

func containsColon(s string) bool {
	for _, c := range s {
		if c == ':' {
			return true
		}
	}
	return false
}
