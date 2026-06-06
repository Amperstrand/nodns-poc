package config

import (
	"fmt"
	"os"
	"time"

	"github.com/BurntSushi/toml"
)

type Config struct {
	Server        ServerConfig  `toml:"server"`
	Nostr         NostrConfig   `toml:"nostr"`
	DNS           DNSConfig     `toml:"dns"`
	Policy        PolicyConfig  `toml:"policy"`
	Store         StoreConfig   `toml:"store"`
	RegistrarKeys map[string]string `toml:"registrar_keys"`
	Payment       PaymentConfig `toml:"payment"`
}

type ServerConfig struct {
	Bind string `toml:"bind"`
}

type NostrConfig struct {
	Relays       []string      `toml:"relays"`
	Zone         string        `toml:"zone"`
	ReconnectMin time.Duration `toml:"reconnect_min"`
	ReconnectMax time.Duration `toml:"reconnect_max"`
}

// ZoneConfig holds DNS connection details for a single zone.
type ZoneConfig struct {
	KnotAddress   string `toml:"knot_address"`
	Zone          string `toml:"zone"`
	TSIGKeyName   string `toml:"tsig_key_name"`
	TSIGSecret    string `toml:"tsig_key_secret"`
	TSIGAlgorithm string `toml:"tsig_algorithm"`
	DefaultTTL    uint32 `toml:"default_ttl"`
	NegativeTTL   uint32 `toml:"negative_ttl"`
}

type DNSConfig struct {
	Zones []ZoneConfig `toml:"zones"`
	// Old fields kept for backward compatibility with single-zone configs.
	KnotAddress   string `toml:"knot_address"`
	Zone          string `toml:"zone"`
	TSIGKeyName   string `toml:"tsig_key_name"`
	TSIGSecret    string `toml:"tsig_key_secret"`
	TSIGAlgorithm string `toml:"tsig_algorithm"`
	DefaultTTL    uint32 `toml:"default_ttl"`
	NegativeTTL   uint32 `toml:"negative_ttl"`
}

type PolicyConfig struct {
	MaxRecords     int      `toml:"max_records"`
	RateLimit      int      `toml:"rate_limit"`
	AllowedTypes   []string `toml:"allowed_types"`
	BlockPrivateIP bool     `toml:"block_private_ip"`
	MaxTXTLength   int      `toml:"max_txt_length"`
}

type StoreConfig struct {
	Path string `toml:"path"`
}

type PaymentConfig struct {
	Enabled      bool   `toml:"enabled"`
	RequiredSats int64  `toml:"required_sats"`
	UpdateFree   bool   `toml:"update_free"`
	CashuMintURL string `toml:"cashu_mint_url"`
}

func Load(path string) (*Config, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("reading config file: %w", err)
	}

	var cfg Config
	if err := toml.Unmarshal(data, &cfg); err != nil {
		return nil, fmt.Errorf("parsing config: %w", err)
	}

	cfg.applyDefaults()

	if err := cfg.Validate(); err != nil {
		return nil, fmt.Errorf("invalid config: %w", err)
	}

	return &cfg, nil
}

func (c *Config) applyDefaults() {
	if c.Server.Bind == "" {
		c.Server.Bind = "127.0.0.1:9090"
	}
	if c.Nostr.ReconnectMin == 0 {
		c.Nostr.ReconnectMin = 1 * time.Second
	}
	if c.Nostr.ReconnectMax == 0 {
		c.Nostr.ReconnectMax = 60 * time.Second
	}
	if c.DNS.DefaultTTL == 0 {
		c.DNS.DefaultTTL = 3600
	}
	if c.DNS.NegativeTTL == 0 {
		c.DNS.NegativeTTL = 60
	}
	if c.DNS.TSIGAlgorithm == "" {
		c.DNS.TSIGAlgorithm = "hmac-sha256"
	}

	// Backward compat: if no [[dns.zones]] but old [dns] fields are set, synthesize one zone.
	if len(c.DNS.Zones) == 0 && c.DNS.Zone != "" {
		c.DNS.Zones = []ZoneConfig{{
			KnotAddress:   c.DNS.KnotAddress,
			Zone:          c.DNS.Zone,
			TSIGKeyName:   c.DNS.TSIGKeyName,
			TSIGSecret:    c.DNS.TSIGSecret,
			TSIGAlgorithm: c.DNS.TSIGAlgorithm,
			DefaultTTL:    c.DNS.DefaultTTL,
			NegativeTTL:   c.DNS.NegativeTTL,
		}}
	}

	for i := range c.DNS.Zones {
		z := &c.DNS.Zones[i]
		if z.DefaultTTL == 0 {
			z.DefaultTTL = c.DNS.DefaultTTL
		}
		if z.NegativeTTL == 0 {
			z.NegativeTTL = c.DNS.NegativeTTL
		}
		if z.TSIGAlgorithm == "" {
			z.TSIGAlgorithm = c.DNS.TSIGAlgorithm
		}
	}
	if c.Policy.MaxRecords == 0 {
		c.Policy.MaxRecords = 20
	}
	if c.Policy.RateLimit == 0 {
		c.Policy.RateLimit = 5
	}
	if len(c.Policy.AllowedTypes) == 0 {
		c.Policy.AllowedTypes = []string{"A", "AAAA", "CNAME", "TXT", "MX"}
	}
	if c.Policy.MaxTXTLength == 0 {
		c.Policy.MaxTXTLength = 512
	}
	if c.Store.Path == "" {
		c.Store.Path = "records.db"
	}
	if !c.Payment.Enabled {
		if c.Payment.RequiredSats == 0 {
			c.Payment.RequiredSats = 250
		}
		c.Payment.UpdateFree = true
	}
}

func (c *Config) Validate() error {
	if len(c.Nostr.Relays) == 0 {
		return fmt.Errorf("nostr.relays must contain at least one relay URL")
	}
	if c.Nostr.Zone == "" {
		return fmt.Errorf("nostr.zone is required")
	}
	if len(c.DNS.Zones) == 0 {
		return fmt.Errorf("at least one dns zone must be configured")
	}
	for i, z := range c.DNS.Zones {
		if z.Zone == "" {
			return fmt.Errorf("dns.zones[%d].zone is required", i)
		}
		if z.KnotAddress == "" {
			return fmt.Errorf("dns.zones[%d].knot_address is required", i)
		}
		if z.TSIGKeyName == "" {
			return fmt.Errorf("dns.zones[%d].tsig_key_name is required", i)
		}
		if z.TSIGSecret == "" {
			return fmt.Errorf("dns.zones[%d].tsig_key_secret is required", i)
		}
	}
	return nil
}
