package main

import (
	"context"
	"encoding/json"
	"flag"
	"fmt"
	"log/slog"
	"net/http"
	"os"
	"os/signal"
	"strings"
	"sync/atomic"
	"syscall"
	"time"

	"github.com/nbd-wtf/go-nostr"
	"github.com/nbd-wtf/go-nostr/nip19"
	"nodns-bot/internal/auth"
	"nodns-bot/internal/config"
	botdns "nodns-bot/internal/dns"
	botnostr "nodns-bot/internal/nostr"
	"nodns-bot/internal/payment"
	"nodns-bot/internal/store"
)

type metrics struct {
	eventsProcessed atomic.Int64
	eventsRejected  atomic.Int64
	ddnsSuccesses   atomic.Int64
	ddnsFailures    atomic.Int64
}

func main() {
	configPath := flag.String("config", "config.toml", "path to config file")
	flag.Parse()

	cfg, err := config.Load(*configPath)
	if err != nil {
		fmt.Fprintf(os.Stderr, "failed to load config: %v\n", err)
		os.Exit(1)
	}

	logger := slog.New(slog.NewJSONHandler(os.Stdout, &slog.HandlerOptions{Level: slog.LevelInfo}))
	slog.SetDefault(logger)

	logger.Info("nodns-bot starting",
		"zone", cfg.Nostr.Zone,
		"relays", cfg.Nostr.Relays,
		"dns_zones", len(cfg.DNS.Zones),
	)

	// Open SQLite store
	st, err := store.NewStore(cfg.Store, logger)
	if err != nil {
		logger.Error("failed to open store", "error", err)
		os.Exit(1)
	}
	defer st.Close()

	if err := st.Init(); err != nil {
		logger.Error("failed to init store", "error", err)
		os.Exit(1)
	}

	authorityChecker := auth.NewAuthorityChecker(st, cfg.RegistrarKeys, logger)

	var paymentVerifier *payment.Verifier
	if cfg.Payment.Enabled {
		paymentVerifier = payment.NewVerifier(
			cfg.Payment.CashuMintURL,
			cfg.Payment.RequiredSats,
			cfg.Payment.UpdateFree,
			logger,
		)
	}

	// Create DNS updater(s) and test connections
	updaters := make(map[string]*botdns.Updater, len(cfg.DNS.Zones))
	for _, zc := range cfg.DNS.Zones {
		u := botdns.NewUpdater(zc, logger)
		if err := u.TestConnection(); err != nil {
			logger.Warn("Knot DNS connection test failed (will retry on updates)", "zone", zc.Zone, "error", err)
		}
		updaters[zc.Zone] = u
	}

	startTime := time.Now()
	var m metrics

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	// Health HTTP server
	mux := http.NewServeMux()
	mux.HandleFunc("/health", func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(map[string]any{
			"status":           "ok",
			"uptime_seconds":   int64(time.Since(startTime).Seconds()),
			"events_processed": m.eventsProcessed.Load(),
			"events_rejected":  m.eventsRejected.Load(),
			"ddns_successes":   m.ddnsSuccesses.Load(),
			"ddns_failures":    m.ddnsFailures.Load(),
		})
	})

	mux.HandleFunc("/api/records", func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		records, err := st.ListAllRecords()
		if err != nil {
			w.WriteHeader(http.StatusInternalServerError)
			json.NewEncoder(w).Encode(map[string]string{"error": err.Error()})
			return
		}
		type apiRecord struct {
			Npub      string `json:"npub"`
			Name      string `json:"name"`
			FQDN      string `json:"fqdn"`
			Type      string `json:"type"`
			TTL       uint32 `json:"ttl"`
			RData     string `json:"rdata"`
			CreatedAt int64  `json:"created_at"`
		}
		out := make([]apiRecord, 0, len(records))
		for _, rec := range records {
			name := rec.Name
			if name == "@" || name == "" {
				name = ""
			}
			fqdn := buildFQDN(rec.Npub, rec.Name, rec.Zone)
			out = append(out, apiRecord{
				Npub:      rec.Npub,
				Name:      name,
				FQDN:      fqdn,
				Type:      rec.RecordType,
				TTL:       rec.TTL,
				RData:     rec.RData,
				CreatedAt: rec.CreatedAt,
			})
		}
		json.NewEncoder(w).Encode(map[string]any{
			"records": out,
			"count":   len(out),
		})
	})

	httpServer := &http.Server{Addr: cfg.Server.Bind, Handler: mux}
	go func() {
		logger.Info("health server listening", "bind", cfg.Server.Bind)
		if err := httpServer.ListenAndServe(); err != nil && err != http.ErrServerClosed {
			logger.Error("health server error", "error", err)
		}
	}()

	// Create and start Nostr subscriber
	subscriber := botnostr.NewSubscriber(cfg.Nostr, st, logger)
	events, err := subscriber.Subscribe()
	if err != nil {
		logger.Error("failed to subscribe", "error", err)
		os.Exit(1)
	}

	// Signal handling
	sigCh := make(chan os.Signal, 1)
	signal.Notify(sigCh, syscall.SIGINT, syscall.SIGTERM)

	// Main event processing loop
	go func() {
		for {
			select {
			case <-ctx.Done():
				return
			case evt, ok := <-events:
				if !ok {
					logger.Info("event channel closed")
					cancel()
					return
				}
				processNostrEvent(evt, cfg, updaters, st, logger, &m, authorityChecker, paymentVerifier)
			}
		}
	}()

	// Wait for shutdown signal
	select {
	case sig := <-sigCh:
		logger.Info("received signal, shutting down", "signal", sig)
	case <-ctx.Done():
	}

	shutdownCtx, shutdownCancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer shutdownCancel()

	subscriber.Stop()
	httpServer.Shutdown(shutdownCtx)

	logger.Info("nodns-bot stopped",
		"uptime_seconds", int64(time.Since(startTime).Seconds()),
		"events_processed", m.eventsProcessed.Load(),
		"events_rejected", m.eventsRejected.Load(),
	)
}

func processNostrEvent(
	evt *nostr.Event,
	cfg *config.Config,
	updaters map[string]*botdns.Updater,
	st *store.Store,
	logger *slog.Logger,
	m *metrics,
	authorityChecker *auth.AuthorityChecker,
	paymentVerifier *payment.Verifier,
) {
	eventID := evt.ID
	pubkey := evt.PubKey
	createdAt := int64(evt.CreatedAt)

	elog := logger.With("event_id", eventID, "pubkey", pubkey)

	npub, err := nip19.EncodePublicKey(pubkey)
	if err != nil {
		elog.Error("failed to encode pubkey to npub", "error", err)
		m.eventsRejected.Add(1)
		return
	}

	parsed, err := botnostr.ClassifyEvent(
		evt,
		cfg.Policy.AllowedTypes,
		cfg.Policy.BlockPrivateIP,
		cfg.Policy.MaxTXTLength,
	)
	if err != nil {
		elog.Warn("event parse failed", "error", err)
		m.eventsRejected.Add(1)
		return
	}

	switch {
	case parsed.Delegation != nil:
		processDelegation(parsed.Delegation, eventID, pubkey, npub, createdAt, cfg, st, logger, authorityChecker, elog, m)
	case parsed.Registrar != nil:
		processRegistrar(parsed.Registrar, eventID, pubkey, npub, createdAt, st, authorityChecker, elog, m)
	default:
		processDNSUpdate(parsed, eventID, pubkey, npub, createdAt, cfg, updaters, st, logger, authorityChecker, elog, m, paymentVerifier)
	}

	if err := st.SetLastSeen(createdAt); err != nil {
		elog.Error("failed to update last_seen", "error", err)
	}
}

func processDelegation(
	delegation *botnostr.Delegation,
	eventID, pubkey, npub string,
	createdAt int64,
	cfg *config.Config,
	st *store.Store,
	logger *slog.Logger,
	authorityChecker *auth.AuthorityChecker,
	elog *slog.Logger,
	m *metrics,
) {
	elog = elog.With("domain", delegation.Domain, "delegate_npub", delegation.Npub)

	zones := configuredZones(cfg)
	domain := strings.TrimSuffix(delegation.Domain, ".")
	var matchedZone string
	for _, zone := range zones {
		if strings.HasSuffix(domain, "."+zone) || domain == zone {
			matchedZone = zone
			break
		}
	}
	if matchedZone == "" {
		elog.Warn("delegation domain does not match any configured zone", "domain", delegation.Domain)
		m.eventsRejected.Add(1)
		return
	}

	if err := authorityChecker.ValidateDelegation(*delegation, matchedZone, pubkey); err != nil {
		elog.Warn("delegation validation failed", "error", err)
		m.eventsRejected.Add(1)
		return
	}

	parts := strings.SplitN(domain, "."+matchedZone, 2)
	domainName := parts[0]

	if err := st.SaveDelegation(
		eventID, domainName, matchedZone,
		delegation.Npub, pubkey,
		delegation.ValidFrom, delegation.ValidUntil, delegation.RenewBy,
		pubkey,
	); err != nil {
		elog.Error("failed to save delegation", "error", err)
		m.eventsRejected.Add(1)
		return
	}

	elog.Info("delegation processed")
	m.eventsProcessed.Add(1)
}

func processRegistrar(
	registrar *botnostr.RegistrarKey,
	eventID, pubkey, npub string,
	createdAt int64,
	st *store.Store,
	authorityChecker *auth.AuthorityChecker,
	elog *slog.Logger,
	m *metrics,
) {
	elog = elog.With("zone", registrar.Zone, "registrar_pubkey", registrar.PubkeyHex)

	isRegistrar, err := authorityChecker.IsRegistrar(registrar.Zone, pubkey)
	if err != nil {
		elog.Error("failed to check registrar status", "error", err)
		m.eventsRejected.Add(1)
		return
	}
	if !isRegistrar {
		elog.Warn("unauthorized registrar key publication", "signer", pubkey)
		m.eventsRejected.Add(1)
		return
	}

	if err := st.SaveRegistrarKey(registrar.Zone, registrar.PubkeyHex, npub, "nostr", eventID); err != nil {
		elog.Error("failed to save registrar key", "error", err)
		m.eventsRejected.Add(1)
		return
	}

	elog.Info("registrar key processed")
	m.eventsProcessed.Add(1)
}

func processDNSUpdate(
	parsed *botnostr.ParsedEvent,
	eventID, pubkey, npub string,
	createdAt int64,
	cfg *config.Config,
	updaters map[string]*botdns.Updater,
	st *store.Store,
	logger *slog.Logger,
	authorityChecker *auth.AuthorityChecker,
	elog *slog.Logger,
	m *metrics,
	paymentVerifier *payment.Verifier,
) {
	if len(parsed.Records) == 0 {
		elog.Warn("no record tags in event")
		m.eventsRejected.Add(1)
		return
	}

	if cfg.Payment.Enabled {
		for zoneName := range updaters {
			if err := payment.CheckEventPayment(
				parsed.Payments, npub, parsed.Records, zoneName, st, paymentVerifier,
			); err != nil {
				elog.Warn("payment verification failed", "error", err)
				m.eventsRejected.Add(1)
				return
			}
		}
	}

	count, err := st.RecordCountByPubkey(pubkey)
	if err != nil {
		elog.Error("failed to count records", "error", err)
		m.eventsRejected.Add(1)
		return
	}
	if count+len(parsed.Records) > cfg.Policy.MaxRecords {
		elog.Warn("exceeds max records",
			"current", count,
			"new", len(parsed.Records),
			"max", cfg.Policy.MaxRecords,
		)
		m.eventsRejected.Add(1)
		return
	}

	recentCount, err := st.EventsInLastMinute(pubkey)
	if err != nil {
		elog.Error("failed to check rate limit", "error", err)
		m.eventsRejected.Add(1)
		return
	}
	if recentCount >= cfg.Policy.RateLimit {
		elog.Warn("rate limited", "events_last_min", recentCount, "limit", cfg.Policy.RateLimit)
		m.eventsRejected.Add(1)
		return
	}

	allOK := true
	for zoneName, updater := range updaters {
		for _, rec := range parsed.Records {
			fqdn := buildFQDN(npub, rec.Name, zoneName)

			if err := authorityChecker.CheckAuthority(fqdn, zoneName, pubkey); err != nil {
				elog.Warn("authority check failed",
					"fqdn", fqdn,
					"zone", zoneName,
					"error", err,
				)
				allOK = false
				continue
			}

			recordType := botdns.TypeStringToUint(rec.Type)

			if err := updater.UpdateRecord(fqdn, rec.TTL, recordType, rec.RData); err != nil {
				elog.Error("DDNS update failed",
					"fqdn", fqdn,
					"type", rec.Type,
					"rdata", rec.RData,
					"zone", zoneName,
					"error", err,
				)
				m.ddnsFailures.Add(1)
				allOK = false
				continue
			}

			m.ddnsSuccesses.Add(1)
			elog.Debug("DDNS update applied", "fqdn", fqdn, "type", rec.Type, "zone", zoneName)

			if err := st.SaveEvent(eventID, npub, pubkey, rec.Name, rec.Type, rec.TTL, rec.RData, zoneName, createdAt); err != nil {
				elog.Error("failed to save event", "zone", zoneName, "error", err)
			}
		}
	}

	if allOK {
		m.eventsProcessed.Add(1)
	}
}

func configuredZones(cfg *config.Config) []string {
	zones := make([]string, 0, len(cfg.DNS.Zones))
	for _, zc := range cfg.DNS.Zones {
		zones = append(zones, zc.Zone)
	}
	return zones
}

// buildFQDN constructs a fully qualified domain name: [name.]npub1xxx.zone.
func buildFQDN(npub, name, zone string) string {
	if name == "@" || name == "" {
		return fmt.Sprintf("%s.%s.", npub, zone)
	}
	return fmt.Sprintf("%s.%s.%s.", name, npub, zone)
}
