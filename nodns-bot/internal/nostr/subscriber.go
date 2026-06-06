package nostr

import (
	"context"
	"fmt"
	"log/slog"
	"sync"
	"time"

	"github.com/nbd-wtf/go-nostr"
	"nodns-bot/internal/config"
	"nodns-bot/internal/store"
)

// Subscriber manages connections to Nostr relays and forwards kind 11111 events.
type Subscriber struct {
	relays       []string
	zone         string
	logger       *slog.Logger
	store        *store.Store
	reconnectMin time.Duration
	reconnectMax time.Duration

	ctx    context.Context
	cancel context.CancelFunc
	wg     sync.WaitGroup
}

// NewSubscriber creates a new Nostr subscriber.
func NewSubscriber(cfg config.NostrConfig, s *store.Store, logger *slog.Logger) *Subscriber {
	ctx, cancel := context.WithCancel(context.Background())
	return &Subscriber{
		relays:       cfg.Relays,
		zone:         cfg.Zone,
		logger:       logger.With("component", "nostr-subscriber"),
		store:        s,
		reconnectMin: cfg.ReconnectMin,
		reconnectMax: cfg.ReconnectMax,
		ctx:          ctx,
		cancel:       cancel,
	}
}

// Subscribe connects to all configured relays and returns a channel of verified events.
func (sub *Subscriber) Subscribe() (<-chan *nostr.Event, error) {
	events := make(chan *nostr.Event, 256)

	lastSeen, err := sub.store.GetLastSeen()
	if err != nil {
		sub.logger.Warn("failed to get last_seen, starting from 0", "error", err)
		lastSeen = 0
	}

	since := nostr.Timestamp(lastSeen)
	filter := nostr.Filter{
		Kinds: []int{KindDNSRecord},
		Since: &since,
	}

	sub.logger.Info("subscribing to relays",
		"relays", sub.relays,
		"since", lastSeen,
		"zone", sub.zone,
	)

	for _, relayURL := range sub.relays {
		sub.wg.Add(1)
		go sub.maintainConnection(relayURL, filter, events)
	}

	return events, nil
}

// Stop shuts down all relay connections.
func (sub *Subscriber) Stop() {
	sub.logger.Info("stopping subscriber")
	sub.cancel()
	sub.wg.Wait()
}

// maintainConnection keeps a persistent connection to a single relay with exponential backoff.
func (sub *Subscriber) maintainConnection(relayURL string, filter nostr.Filter, events chan<- *nostr.Event) {
	defer sub.wg.Done()

	backoff := sub.reconnectMin

	for {
		select {
		case <-sub.ctx.Done():
			return
		default:
		}

		err := sub.runSubscription(relayURL, filter, events)
		if err == nil {
			return
		}

		sub.logger.Warn("relay subscription ended, reconnecting",
			"relay", relayURL,
			"error", err,
			"backoff", backoff,
		)

		select {
		case <-sub.ctx.Done():
			return
		case <-time.After(backoff):
			if backoff < sub.reconnectMax {
				backoff *= 2
			}
		}
	}
}

// runSubscription connects to a relay, subscribes, and forwards events until error or cancellation.
func (sub *Subscriber) runSubscription(relayURL string, filter nostr.Filter, events chan<- *nostr.Event) error {
	relay, err := nostr.RelayConnect(sub.ctx, relayURL)
	if err != nil {
		return fmt.Errorf("relay connect %s: %w", relayURL, err)
	}
	defer relay.Close()

	sub.logger.Info("connected to relay", "relay", relayURL)

	subCtx, subCancel := context.WithCancel(sub.ctx)
	defer subCancel()

	subscription, err := relay.Subscribe(subCtx, []nostr.Filter{filter})
	if err != nil {
		return fmt.Errorf("subscribe %s: %w", relayURL, err)
	}

	for {
		select {
		case <-subCtx.Done():
			return subCtx.Err()

		case evt, ok := <-subscription.Events:
			if !ok {
				return fmt.Errorf("events channel closed for %s", relayURL)
			}
			sub.logger.Debug("received event",
				"relay", relayURL,
				"event_id", evt.ID,
				"pubkey", evt.PubKey,
			)

			// Verify signature
			ok, err := evt.CheckSignature()
			if err != nil {
				sub.logger.Warn("signature check error",
					"event_id", evt.ID,
					"error", err,
				)
				continue
			}
			if !ok {
				sub.logger.Warn("invalid signature",
					"event_id", evt.ID,
					"pubkey", evt.PubKey,
				)
				continue
			}

			select {
			case events <- evt:
			case <-subCtx.Done():
				return subCtx.Err()
			}

		case <-subscription.EndOfStoredEvents:
			sub.logger.Debug("EOSE received", "relay", relayURL)

		case reason, ok := <-subscription.ClosedReason:
			if !ok {
				return fmt.Errorf("closed reason channel closed for %s", relayURL)
			}
			return fmt.Errorf("subscription closed by %s: %s", relayURL, reason)
		}
	}
}
