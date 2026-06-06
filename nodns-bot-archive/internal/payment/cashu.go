package payment

import (
	"encoding/hex"
	"fmt"
	"log/slog"

	"github.com/elnosh/gonuts/cashu"
	"github.com/elnosh/gonuts/cashu/nuts/nut07"
	"github.com/elnosh/gonuts/crypto"
	"github.com/elnosh/gonuts/wallet/client"
)

// Verifier validates Cashu tokens for anti-spam payment verification.
type Verifier struct {
	mintURL     string
	requiredAmt int64 // sats required per new record
	updateFree  bool  // if true, updates to existing records are free
	logger      *slog.Logger
}

// NewVerifier creates a new Cashu payment verifier.
func NewVerifier(mintURL string, requiredAmt int64, updateFree bool, logger *slog.Logger) *Verifier {
	return &Verifier{
		mintURL:     mintURL,
		requiredAmt: requiredAmt,
		updateFree:  updateFree,
		logger:      logger.With("component", "payment"),
	}
}

// ShouldRequirePayment determines if payment is needed for this operation.
// If updateFree is true and the record already exists, returns false.
// If payment is disabled (requiredAmt == 0), returns false.
func (v *Verifier) ShouldRequirePayment(isUpdate bool) bool {
	if v.requiredAmt == 0 {
		return false
	}
	if isUpdate && v.updateFree {
		return false
	}
	return true
}

// VerifyPayment validates a Cashu token against the required amount.
// It checks:
//  1. Token can be decoded (valid format)
//  2. Token mint matches configured mint URL
//  3. Total token amount >= required amount
//  4. All proofs are unspent (calls mint's /v1/checkstate)
//
// Returns error if any check fails.
func (v *Verifier) VerifyPayment(tokenString string, requiredAmount int64) error {
	token, err := cashu.DecodeToken(tokenString)
	if err != nil {
		return fmt.Errorf("failed to decode cashu token: %w", err)
	}

	// Check mint URL matches
	tokenMint := token.Mint()
	if tokenMint != v.mintURL {
		return fmt.Errorf("token mint %q does not match configured mint %q", tokenMint, v.mintURL)
	}

	// Check amount
	tokenAmount := token.Amount()
	if int64(tokenAmount) < requiredAmount {
		return fmt.Errorf("insufficient payment: got %d sats, need %d sats", tokenAmount, requiredAmount)
	}

	// Compute Y values (hash-to-curve of each proof secret) for checkstate
	proofs := token.Proofs()
	ys := make([]string, 0, len(proofs))
	for _, proof := range proofs {
		point, err := crypto.HashToCurve([]byte(proof.Secret))
		if err != nil {
			return fmt.Errorf("hash-to-curve failed for proof: %w", err)
		}
		ys = append(ys, hex.EncodeToString(point.SerializeCompressed()))
	}

	if len(ys) == 0 {
		return fmt.Errorf("token contains no proofs")
	}

	// Call mint checkstate endpoint
	resp, err := client.PostCheckProofState(tokenMint, nut07.PostCheckStateRequest{Ys: ys})
	if err != nil {
		v.logger.Error("mint checkstate request failed", "mint", tokenMint, "error", err)
		return fmt.Errorf("failed to check proof state at mint: %w", err)
	}

	// Verify all proofs are unspent
	for _, state := range resp.States {
		if state.State != nut07.Unspent {
			return fmt.Errorf("proof %s is %s (not unspent)", truncateY(state.Y), state.State)
		}
	}

	v.logger.Info("cashu token verified",
		"amount", tokenAmount,
		"proofs", len(proofs),
		"mint", tokenMint,
	)
	return nil
}

// truncateY shortens a Y hash for log/error messages.
func truncateY(y string) string {
	if len(y) > 12 {
		return y[:12] + "..."
	}
	return y
}
