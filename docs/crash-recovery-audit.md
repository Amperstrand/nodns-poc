# Crash Recovery Audit: nodns-bot-rs

## Summary

- **Operations audited**: 8
- **Critical risk**: 2
- **High risk**: 2
- **Medium risk**: 2
- **Low risk**: 2

This audit traces every stateful operation in the nodns bot to identify crash windows — the gaps between "start operation" and "complete operation" where a process crash leaves the system inconsistent. The reference model is coco's operation saga pattern (pending → processing → completed, with automatic restart recovery).

**Headline finding**: The event-processing pipeline (the bot's core) is largely self-healing because Nostr events are replayable on restart and DDNS updates are idempotent. However, the **ACME certificate flow has no crash recovery at all** — a crash mid-issuance orphans the order as `pending` forever, loses the generated private key, and can waste CA rate-limit budget. This is the single most serious gap and the only operation that would benefit from a coco-style operation saga.

---

## Critical findings

### 1. ACME certificate issuance — no restart recovery, cert + key loss on crash

**Location**: `acme.rs::run_acme_flow()` (lines 254–516), spawned fire-and-forget by `handlers/acme_order.rs::acme_order_handler` (line 146: `tokio::spawn`).

**Steps**:
1. Handler: `store.save_acme_order(id, "pending")` — SQLite write (`acme_order.rs:112`)
2. Handler: `tokio::spawn(request_certificate(...))` — fire-and-forget task (`acme_order.rs:146`)
3. `get_or_create_account` — external ACME account create, stores creds in `meta` (`acme.rs:264`)
4. `account.new_order` — external ACME order creation (`acme.rs:297`)
5. Per authz: `updater.update_record(_acme-challenge TXT)` — DDNS to Knot (`acme.rs:377`)
6. Per authz: `challenge.set_ready()` — external ACME signal (`acme.rs:389`)
7. `order.poll_ready` — external poll for validation result (`acme.rs:405`)
8. `order.finalize()` / `finalize_csr()` — external, **generates the private key in memory** (`acme.rs:434–445`)
9. `order.poll_certificate` — external, downloads cert chain (`acme.rs:449`)
10. Cleanup: `updater.delete_record(_acme-challenge TXT)` — DDNS delete (`acme.rs:485`)
11. `store.update_acme_order_status("issued", cert, key)` — SQLite write (`acme.rs:504`)

**External calls**: ACME CA (Let's Encrypt / ZeroSSL) over HTTPS; Knot DNS via DDNS TCP.

**Crash scenario**: The order row is created as `"pending"` in step 1 and is **only** updated to `"issued"` or `"failed"` at step 11 (or in the error path of `request_certificate`, `acme.rs:242`). There is **no intermediate persistence** of any stage. If the bot crashes, restarts, or is redeployed at any point between step 1 and step 11:

- The `tokio::spawn` task is killed immediately on process exit.
- The order row remains `"pending"` in SQLite permanently.
- **No code on startup queries for pending ACME orders.** `main.rs` has a `lease_expiry_task` (hourly sweep for delegation expiry) but no equivalent for ACME. Confirmed by reading `main.rs` lines 75–174 and 350–480 — there is no ACME recovery hook.

Specific crash windows and their consequences:

| Crash between | Result |
|---|---|
| Step 5 (TXT published) and step 6 (set_ready) | `_acme-challenge` TXT record orphaned in DNS; CA never validates; order stuck `"pending"` |
| Step 8 (finalize) and step 11 (store) | **Certificate issued by CA, private key generated in memory, both lost.** CA rate-limit budget consumed with nothing to show for it. |
| Step 6 (set_ready) and step 9 (poll_certificate) | CA may complete validation and issue, but bot never downloads/stores the cert. Order stuck `"pending"`. The issued cert is at the CA but unretrievable without the order state. |
| Anywhere after step 1 before step 11 | Orphaned `_acme-challenge` TXT accumulates in DNS zone |

**Impact**:
- **Money/rate-limit loss**: Let's Encrypt enforces a "5 duplicate certificates per week" limit and a 50 certs/registered-domain/week limit. Each crashed finalization permanently consumes one slot. ZeroSSL has similar quotas.
- **Key loss**: The private key from `order.finalize()` (`acme.rs:441`) exists only in the task's memory. A crash after step 8 means the key is irretrievable.
- **User impact**: The frontend polls `GET /api/acme/order/:id` and sees `"pending"` forever. No error, no retry, no progress. The user must manually re-request a certificate.
- **DNS pollution**: Orphaned `_acme-challenge` TXT records accumulate, which can confuse future validation attempts for the same domain.

**Current recovery**: **None.** Manual intervention only — the operator must delete the pending order row and the user must re-request. There is no startup sweep, no resumption logic, and no timeout that transitions stale `pending` orders to `failed`.

**Recommended fix**: **Pattern 1 (Operation saga) + Pattern 5 (Write-ahead logging).**

The ACME flow is a textbook case for an operation saga. The order row already exists — it just needs richer lifecycle states and a restart sweep.

**Implementation sketch**:

```rust
// 1. Extend the acme_orders status lifecycle:
//    pending → account_ready → challenge_published → verifying
//            → finalizing → issued / failed
//    (currently: pending → issued / failed, nothing in between)

// 2. Store the ACME order URL after step 4 so a restart can re-fetch it:
ALTER TABLE acme_orders ADD COLUMN acme_order_url TEXT;
ALTER TABLE acme_orders ADD COLUMN account_key_pem TEXT;  // encrypted, like private_key_pem

// 3. Persist intermediate state after each external call:
//    After step 4 (new_order):  UPDATE acme_orders SET status='account_ready', acme_order_url=?
//    After step 5 (TXT publish): UPDATE acme_orders SET status='challenge_published'
//    After step 8 (finalize):    UPDATE acme_orders SET status='finalizing'
//                                -- private key is now in memory, persist it IMMEDIATELY:
//                                UPDATE acme_orders SET private_key_pem=? (encrypted)

// 4. Add a startup recovery sweep in main.rs (mirrors lease_expiry_task):

async fn acme_recovery_task(acme: Arc<AcmeService>, store: Arc<Store>) {
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;
        let pending = match store.list_acme_orders_by_status("pending") {
            Ok(o) => o,
            Err(_) => continue,
        };
        for order in pending {
            // Re-fetch the ACME order state from the CA using the stored order URL.
            // If the CA says it's issued, download and store the cert.
            // If the CA says it failed, mark the order failed.
            // If no order URL (crashed before step 4), mark as failed (re-request needed).
            if let Err(e) = acme.resume_order(&order).await {
                tracing::warn!(order_id=%order.id, error=%e, "ACME resume failed");
            }
        }
    }
}

// 5. Key safety: persist the private key immediately after finalize(),
//    BEFORE poll_certificate(). This is the most critical single change —
//    it converts "key lost on crash" into "key safe, cert re-downloadable".
```

The most impactful minimal fix (without the full saga) is **step 5 alone**: move `store.update_acme_order_status` for the private key to right after `finalize()` returns, before `poll_certificate()`. This ensures the key survives a crash even if the cert download fails or the process dies. The cert can always be re-downloaded from the CA; the generated key cannot.

---

### 2. ACME private key not persisted until after full flow completes

**Location**: `acme.rs::run_acme_flow()`, lines 434–512.

> *Note: This is a sub-aspect of finding #1 but called out separately because it is the most dangerous single crash window and the easiest to fix in isolation.*

**Steps**:
1. `order.finalize()` — generates an ECDSA/RSA private key, returned as `private_key_pem` (`acme.rs:441`)
2. `order.poll_certificate()` — downloads the cert chain (`acme.rs:449`)
3. `updater.delete_record(_acme-challenge TXT)` — cleanup DDNS (`acme.rs:485`)
4. `store.update_acme_order_status("issued", cert_chain_pem, private_key_pem)` — **first and only** persistence of both cert and key (`acme.rs:504`)

**Crash scenario**: Crash between step 1 and step 4. The private key was generated, the CA has finalized the order (rate limit consumed), but the key exists only in the `private_key_pem` local variable. On crash, it is gone.

**Impact**: **Irrecoverable key loss.** The certificate was issued against this key. Without the key, the certificate is useless. The user must wait for the CA rate-limit window to expire and request a completely new certificate. For Let's Encrypt this is up to 7 days.

**Current recovery**: None. The key is held in memory across two external network calls (poll_certificate, delete_record) before being written to disk.

**Recommended fix**: **Pattern 5 (Write-ahead logging).** Persist the key immediately after generation, before any further network calls.

**Implementation sketch**:

```rust
// In run_acme_flow(), after finalize() returns the key, persist it immediately:

let private_key_pem = if let Some(csr) = csr_der {
    order.finalize_csr(csr).await.map_err(...)?;
    None
} else {
    Some(order.finalize().await.map_err(...)?)
};

// --- NEW: persist the key NOW, before poll_certificate and cleanup ---
if let Some(ref key) = private_key_pem {
    self.store.update_acme_order_status(
        order_id, "finalizing", None, Some(key), None
    ).map_err(|e| AcmeError::StoreError(e.to_string()))?;
    self.log_stage(order_id, "key_stored", "Private key persisted", None);
}
// --- key is now safe across crashes ---

let cert_chain_pem = order.poll_certificate(&RetryPolicy::default()).await...?;

// ... cleanup ...

self.store.update_acme_order_status(
    order_id, "issued", Some(&cert_chain_pem), None, None  // key already stored
).map_err(...)?;
```

This is a ~5-line change that eliminates the worst crash window in the system. If the process dies after this point, the key is in SQLite and the cert can be re-downloaded from the CA on restart (assuming finding #1's recovery sweep is also implemented) or manually.

---

## High findings

### 3. DNS record applied to Knot but not recorded in SQLite

**Location**: `event_processor.rs::process_dns_update()`, lines 790–822.

**Steps** (per record, per zone):
1. `payment::check_event_payment` — external Cashu mint `checkstate` (read-only HTTP) + store reads (`event_processor.rs:762`)
2. `authority.check_authority` — store read (`event_processor.rs:782`)
3. `updater.update_record(fqdn, ttl, rt, rdata)` — **DDNS UPDATE to Knot DNS** (`event_processor.rs:790`)
4. *(if TXT)* `updater.append_record` — second DDNS UPDATE for compact event TXT (`event_processor.rs:804`)
5. `store.save_event` — **SQLite write** (`event_processor.rs:810`)

**External calls**: Cashu mint HTTP; Knot DNS via DDNS TCP.

**Crash scenario**: Crash between step 3 (DDNS succeeds) and step 5 (SQLite write). The DNS record is now **live and resolvable globally** but the bot's SQLite database has no record of it.

**Impact**:
- `record_count_by_pubkey()` undercounts → user can publish more than `max_records` (policy bypass).
- The record does not appear in `GET /api/records` or the frontend dashboard → user-visible inconsistency.
- The record is not soft-deleted when the delegation expires (the lease_expiry_task queries SQLite, not Knot) → orphaned DNS record after delegation expiry.

**Current recovery**: **Partial self-healing via Nostr replay.** `set_last_seen(created_at)` is called only at the very end of `process_nostr_event` (`event_processor.rs:207`). The subscriber's restart filter uses `since(last_seen)` (`subscriber.rs:62–66`). So an event whose processing crashed before `set_last_seen` will be re-delivered on restart, re-processed, and the DDNS (idempotent replace) + save will complete.

**Caveat**: This self-healing depends on the relay still retaining the event. Nostr relays have variable retention. If the relay has evicted the event before the bot restarts, the record is permanently orphaned in DNS with no SQLite counterpart. This is unlikely for recent events but possible under long downtime + aggressive relay pruning.

Additionally, for **multiple records in one event**, a crash mid-loop leaves a partial set applied. The replay re-applies all of them idempotently, so this self-heals cleanly.

**Recommended fix**: **Pattern 5 (Write-ahead logging)** — invert the ordering to persist intent before the side effect, then reconcile.

The cleanest fix is to write the event to SQLite **first** (as "pending"), apply the DDNS, then mark "applied". But this is a significant refactor of the current fire-and-DDNS-then-save pattern. A lighter-touch alternative:

**Implementation sketch (lighter fix)**:

```rust
// Option A (preferred): Swap the ordering — save to SQLite BEFORE the DDNS,
// then apply DDNS. If DDNS fails, the record is "tracked but not live"
// and the next event or manual reconciliation fixes it. This is strictly
// better than "live but not tracked" because the truth-source (SQLite)
// is always consistent, and DNS is rebuildable from SQLite.

// In process_dns_update, swap lines 790 and 810:
if let Err(e) = store.save_event(event_id, npub, pubkey_hex, &rec.name,
    &rec.record_type, rec.ttl, &rec.rdata, zone_name, created_at) {
    error!(...);  // if save fails, don't apply DDNS at all
    continue;
}
// NOW apply DDNS (SQLite already has the record):
if let Err(e) = updater.update_record(&fqdn, rec.ttl, rt, &rec.rdata).await {
    error!(...);
    // record is in SQLite but not in DNS — add a reconciliation sweep
    continue;
}

// Option B (reconciliation sweep): a background task that diffs SQLite
// against Knot DNS and applies missing updates. Heavier to implement.
```

Option A is the right call: it's a ~10-line reorder that makes SQLite the authoritative state and DNS a derived view. The "save succeeds, DDNS fails" case already exists today (the error is logged but the event is considered processed), so this doesn't introduce a new failure mode — it just makes the more dangerous direction (DDNS without save) impossible.

---

### 4. ACME challenge TXT records orphaned on crash

**Location**: `acme.rs::run_acme_flow()`, lines 377–502.

**Steps**:
1. `updater.update_record(_acme-challenge.{domain}, TXT, dns_value)` — publishes challenge (`acme.rs:377`)
2. ... (validation, finalize, etc.) ...
3. `updater.delete_record(_acme-challenge.{domain}, TXT)` — cleanup (`acme.rs:485`)

**Crash scenario**: Crash between step 1 and step 3. The `_acme-challenge` TXT record remains in the DNS zone. Since there is no restart recovery (finding #1), it is never cleaned up.

**Impact**: DNS pollution. Accumulated `_acme-challenge` TXT records can:
- Confuse future ACME validation attempts (old challenge values visible).
- Trigger DNS lint warnings.
- In theory, leak information about past certificate requests.

This is not a money/security issue but is a persistent operational debt that grows with each crashed ACME flow.

**Current recovery**: None. No sweep for orphaned challenge records.

**Recommended fix**: **Pattern 1 (Operation saga)** — track challenge records in the `acme_dns_registrations` table (which already exists) or a dedicated `acme_challenges` table, and clean them up on restart.

**Implementation sketch**:

```sql
CREATE TABLE IF NOT EXISTS acme_challenges (
    order_id   TEXT NOT NULL,
    fqdn       TEXT NOT NULL,
    txt_value  TEXT NOT NULL,
    published  INTEGER NOT NULL DEFAULT 0,   -- 1 once DDNS applied
    cleaned    INTEGER NOT NULL DEFAULT 0,   -- 1 once delete applied
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    PRIMARY KEY (order_id, fqdn)
);
```

```rust
// Before publishing challenge TXT:
store.save_acme_challenge(order_id, &fqdn, &dns_value, published=false)?;

// After DDNS succeeds:
store.mark_challenge_published(order_id, &fqdn)?;

// After successful cleanup:
store.mark_challenge_cleaned(order_id, &fqdn)?;

// Startup sweep (part of the acme_recovery_task from finding #1):
let orphaned = store.list_uncleaned_challenges()?;
for ch in orphaned {
    if let Some(updater) = updaters.get(&ch.zone) {
        let _ = updater.delete_record(&ch.fqdn, 16).await;
        store.mark_challenge_cleaned(&ch.order_id, &ch.fqdn)?;
    }
}
```

---

## Medium findings

### 5. Lease expiry DNS cleanup — partial application self-heals on next tick

**Location**: `main.rs::lease_expiry_task()`, lines 80–174.

**Steps** (per expired delegation):
1. For each DNS record: `updater.delete_record(fqdn, rt)` — DDNS delete (`main.rs:130`)
2. `store.soft_delete_records_by_npub_zone` — SQLite soft-delete (`main.rs:140`)
3. `store.mark_delegation_expired` — SQLite status update (`main.rs:147`)

**Crash scenario**: Crash between step 1 (some DNS records deleted) and step 3 (delegation not marked expired). The DNS records are gone but SQLite still shows them active and the delegation as active/grace.

**Impact**: Temporary inconsistency. The delegation's records are deleted from DNS but the store thinks they're live. Users see records in the dashboard that don't resolve.

**Current recovery**: **Self-heals within one hour.** The `lease_expiry_task` runs every 3600 seconds (`main.rs:85`). On the next tick, the delegation is still past `valid_until` and not marked expired, so the task re-enters the loop. The DDNS deletes are re-applied (idempotent — deleting a non-existent RRset is a no-op at Knot), then the soft-delete and status update complete. Worst case: 1 hour of inconsistency.

**Recommended fix**: No fix needed — the hourly sweep provides adequate self-healing for this non-critical path. The one improvement worth noting is reversing the order (mark expired in SQLite first, then delete from DNS) so the truth-source is always consistent, but the current design is acceptable given the self-healing behavior.

### 6. Cashu payment verification is check-only (no melt/spend) — replayable, but double-spendable

**Location**: `payment.rs::verify_payment()`, lines 134–228; called from `event_processor.rs:422` (claim) and `event_processor.rs:617` (renewal) and `payment.rs:286` (DNS update).

**Steps**:
1. Decode Cashu token, verify mint URL matches (`payment.rs:140–163`)
2. Verify token amount ≥ required (`payment.rs:166–175`)
3. POST `/v1/checkstate` to mint — **read-only** check that proofs are unspent (`payment.rs:189–208`)
4. Proceed to apply the operation (delegation save / DNS update)

**Crash scenario**: The bot calls `checkstate` (read-only). It does **not** spend/melt the token. So:
- Crash between step 3 and step 4: the token was verified unspent but nothing was applied. On restart + replay, the event re-processes and the token passes `checkstate` again. **Good for recovery** — no funds lost, no "paid but not served."
- However, because the token is never spent, the **same token can be replayed across multiple events**. A user could publish the same valid Cashu token in 10 different claim events to 10 relays; all 10 would pass `checkstate` and create 10 delegations for the price of one token.

**Impact**: Not a crash-recovery data-loss issue, but a **payment integrity gap** that the crash-replay behavior exposes more broadly. The coco reference explicitly solves this with "proof reservation" (state → `reserved`) before the operation, then `spent` after. nodns has no such lifecycle.

**Current recovery**: N/A (this is a payment design issue, not a crash window).

**Recommended fix**: Out of scope for a pure crash-recovery audit, but noted: the system should either (a) melt/spend the token after successful verification, or (b) track spent Y-values in a `spent_proofs` table to reject replays. This is **Pattern 2 (Idempotency keys)** applied to payment proofs.

---

## Low findings

- **Delegation save (`process_delegation`, `event_processor.rs:248`)**: Single SQLite `INSERT OR REPLACE`. Genuinely atomic. No external calls between validation and persistence. No crash risk. The event replays on restart if needed.
- **Registrar key save (`process_registrar`, `event_processor.rs:296`)**: Single SQLite write. Atomic. No risk.
- **`set_last_seen` watermark (`event_processor.rs:207`)**: Updated at the end of each event. If it fails or the bot crashes before updating, the only effect is that the event is re-delivered on restart (which is safe — all operations are idempotent or replay-safe). Cosmetic.

---

## Operations confirmed safe (no false alarms)

The following were examined and found to be genuinely safe — included per the prompt's quality bar requirement to "honestly state 'no risk found' for operations that are genuinely atomic":

| Operation | Why it's safe |
|---|---|
| `parser::classify_event` | Pure function, no state mutation. |
| `auth::check_authority` | Read-only (store query + config lookup). No writes. |
| `auth::is_registrar` / `validate_delegation` | Read-only checks before any write. |
| `store::save_event` / `save_delegation` / `save_registrar_key` | Single-statement SQLite writes under `Mutex<Connection>`. Each is atomic per SQLite semantics. |
| `dns::Updater::update_record` | Single DDNS message containing both RemoveRRset + Insert (atomic at the Knot level per RFC 2136 §3.4.2.4). The two operations are in one message, not two. |
| `store` AES-256-GCM encrypt/decrypt of ACME keys | Pure crypto, no multi-step state. |
| NIP-05 verification (`nip05.rs`) | Read-only handler. |
| DNS TXT query (`dns::query_txt_records`) | Read-only DNS query. |

---

## Summary of recommended fixes

| # | Fix | Priority | Effort | Pattern |
|---|---|---|---|---|
| 1 | **Persist ACME private key immediately after `finalize()`**, before `poll_certificate()` and cleanup. ~5-line change. Eliminates irrecoverable key loss. | **Critical** | XS (hours) | Pattern 5 (WAL) |
| 2 | **Add startup ACME recovery sweep** — query `pending` ACME orders on restart, re-fetch order state from CA, resume or mark failed. Mirrors the existing `lease_expiry_task`. | **Critical** | M (1–2 days) | Pattern 1 (Saga) |
| 3 | **Track ACME challenge TXT records** in a table, clean up orphaned ones on restart. | High | S (half day) | Pattern 1 + WAL |
| 4 | **Reorder `process_dns_update`: save to SQLite before DDNS**, so the truth-source is always consistent and DNS is derived. ~10-line reorder. | High | XS (hours) | Pattern 5 (WAL) |
| 5 | Track intermediate ACME order states (`challenge_published`, `verifying`, `finalizing`) instead of `pending` → terminal only. Enables precise restart resumption. | Medium | S (half day) | Pattern 1 (Saga) |
| 6 | *(Optional, payment integrity)* Track spent Cashu proof Y-values or melt tokens after verification to prevent replay/double-spend. | Medium | M | Pattern 2 (Idempotency) |

**If only one fix is implemented, it should be fix #1** (persist the key after finalize). It is the smallest change with the largest impact: it converts the only irrecoverable-loss crash window into a recoverable one. Fix #2 (recovery sweep) is the natural follow-up that makes ACME fully crash-safe.

---

## Audit checklist

- [x] Every stateful operation is documented (payment, DNS update, auth, delegation, ACME, lease expiry)
- [x] Every operation with an external call is identified (Cashu mint HTTP, Knot DNS DDNS, ACME CA HTTPS)
- [x] Crash windows are specific (e.g., "between `updater.update_record` at line 790 and `store.save_event` at line 810")
- [x] Risk classification uses the criteria (CRITICAL = key/rate-limit loss; HIGH = user-visible inconsistency needing intervention; MEDIUM = self-healing within a tick; LOW = cosmetic)
- [x] Every CRITICAL and HIGH finding has a recommended fix with a pattern reference
- [x] Every fix has an implementation sketch (not just "use transactions")
- [x] The audit honestly states "no risk found" for operations that are genuinely atomic (table in "Operations confirmed safe")
