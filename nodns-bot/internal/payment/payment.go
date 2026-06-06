package payment

import (
	"fmt"

	botnostr "nodns-bot/internal/nostr"
	"nodns-bot/internal/store"
)

// CheckEventPayment verifies payment requirements for a DNS update event.
// For each record in the event:
//   - If it's a new record (not in DB), payment is required
//   - If it's an update (already exists), payment may be free (configurable)
//   - Payment can be Cashu token or Zap receipt
//
// Returns error if payment is required but missing/invalid.
func CheckEventPayment(
	payments []botnostr.Payment,
	npub string,
	records []botnostr.DNSRecord,
	zone string,
	st *store.Store,
	verifier *Verifier,
) error {
	if verifier == nil {
		return nil
	}

	// Count records that need payment
	var newRecordCount int
	for _, rec := range records {
		exists, err := st.HasRecord(npub, rec.Type, rec.Name, zone)
		if err != nil {
			return fmt.Errorf("checking record existence: %w", err)
		}
		if !verifier.ShouldRequirePayment(exists) {
			continue
		}
		newRecordCount++
	}

	if newRecordCount == 0 {
		return nil
	}

	totalRequired := int64(newRecordCount) * verifier.requiredAmt

	// Look for Cashu payments
	var totalVerified int64
	for _, p := range payments {
		if p.Method != "cashu" {
			continue
		}
		if err := verifier.VerifyPayment(p.Token, totalRequired-totalVerified); err != nil {
			verifier.logger.Warn("cashu token verification failed",
				"error", err,
				"mint", p.MintURL,
			)
			continue
		}
		// Decode again to get the amount (already validated inside VerifyPayment)
		totalVerified += p.Amount
		if totalVerified >= totalRequired {
			return nil
		}
	}

	return fmt.Errorf("insufficient payment: verified %d sats, need %d sats for %d new record(s)",
		totalVerified, totalRequired, newRecordCount)
}
