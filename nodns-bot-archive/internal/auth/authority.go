package auth

import (
	"fmt"
	"log/slog"
	"strings"
	"time"

	"github.com/nbd-wtf/go-nostr/nip19"
	botnostr "nodns-bot/internal/nostr"
	"nodns-bot/internal/store"
)

type AuthorityChecker struct {
	store      *store.Store
	logger     *slog.Logger
	configKeys map[string]string
}

func NewAuthorityChecker(st *store.Store, configKeys map[string]string, logger *slog.Logger) *AuthorityChecker {
	return &AuthorityChecker{
		store:      st,
		logger:     logger.With("component", "auth"),
		configKeys: configKeys,
	}
}

// CheckAuthority verifies that pubkeyHex has authority to manage DNS for fqdn in zone.
// npub1*.zone names are always allowed. Custom names require an active delegation.
func (ac *AuthorityChecker) CheckAuthority(fqdn, zone, pubkeyHex string) error {
	fqdn = strings.TrimSuffix(fqdn, ".")

	npub, err := nip19.EncodePublicKey(pubkeyHex)
	if err != nil {
		return fmt.Errorf("encoding pubkey to npub: %w", err)
	}

	zoneSuffix := "." + zone
	if strings.HasSuffix(fqdn, zoneSuffix) {
		prefix := strings.TrimSuffix(fqdn, zoneSuffix)
		if strings.HasPrefix(prefix, "npub1") {
			if prefix == npub {
				return nil
			}
			return fmt.Errorf("npub name %s does not match signer npub %s", prefix, npub)
		}
	}

	domain := fqdn
	if strings.Contains(fqdn, ".") {
		parts := strings.SplitN(fqdn, ".", 2)
		domain = parts[len(parts)-1]
		if domain != zone {
			return fmt.Errorf("domain %q does not belong to zone %q", fqdn, zone)
		}
		domain = parts[0]
	}

	delegation, err := ac.store.GetActiveDelegation(domain, zone)
	if err != nil {
		return fmt.Errorf("checking delegation for %s.%s: %w", domain, zone, err)
	}
	if delegation == nil {
		return fmt.Errorf("no active delegation for %s.%s", domain, zone)
	}

	if delegation.Npub != npub {
		return fmt.Errorf("delegation for %s.%s assigned to %s, not signer %s", domain, zone, delegation.Npub, npub)
	}

	return nil
}

// IsRegistrar checks if pubkeyHex is the authorized registrar for zone.
func (ac *AuthorityChecker) IsRegistrar(zone, pubkeyHex string) (bool, error) {
	dbKey, err := ac.store.GetRegistrarKey(zone)
	if err != nil {
		return false, fmt.Errorf("getting registrar key for %s: %w", zone, err)
	}
	if dbKey != "" {
		return dbKey == pubkeyHex, nil
	}
	if ac.configKeys != nil {
		if cfgKey, ok := ac.configKeys[zone]; ok {
			return cfgKey == pubkeyHex, nil
		}
	}
	return false, nil
}

// ValidateDelegation verifies a delegation event is properly signed by the zone's registrar.
func (ac *AuthorityChecker) ValidateDelegation(delegation botnostr.Delegation, zone, signerPubkey string) error {
	now := time.Now().Unix()

	if delegation.ValidFrom > now {
		return fmt.Errorf("delegation valid_from %d is in the future (now %d)", delegation.ValidFrom, now)
	}
	if delegation.ValidUntil <= now {
		return fmt.Errorf("delegation valid_until %d has expired (now %d)", delegation.ValidUntil, now)
	}
	if delegation.ValidUntil <= delegation.ValidFrom {
		return fmt.Errorf("delegation valid_until %d must be after valid_from %d", delegation.ValidUntil, delegation.ValidFrom)
	}

	domain := delegation.Domain
	domain = strings.TrimSuffix(domain, ".")
	zoneSuffix := "." + zone
	if !strings.HasSuffix(domain, zoneSuffix) && domain != zone {
		return fmt.Errorf("delegation domain %q is not within zone %q", delegation.Domain, zone)
	}

	isRegistrar, err := ac.IsRegistrar(zone, signerPubkey)
	if err != nil {
		return fmt.Errorf("checking registrar status: %w", err)
	}
	if !isRegistrar {
		return fmt.Errorf("signer %s is not the registrar for zone %s", signerPubkey, zone)
	}

	return nil
}
