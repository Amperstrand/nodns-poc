import { Command } from "commander";
import { decodeSec, bytesToHex } from "../lib/nostr.js";
import {
  checkRefundEligibility,
  reclaimExpiredToken,
} from "../lib/p2pk.js";
import { DEFAULT_MINT_URL } from "../lib/constants.js";

export const refundCommand = new Command("refund")
  .description("Check or reclaim Cashu tokens with expired locktime")
  .argument("<token>", "Cashu token string")
  .option("--claim", "Attempt to reclaim the token after locktime expiry")
  .option("--mint <url>", "Mint URL (auto-detected from token if possible)")
  .action(async (token: string, opts, cmd: Command) => {
    const o = cmd.optsWithGlobals();
    const sec: string | undefined = o.sec;
    const mintUrl: string = opts.mint ?? DEFAULT_MINT_URL;
    const wantClaim: boolean = opts.claim ?? false;

    let userPubkeyHex: string | undefined;
    let userPrivkeyHex: string | undefined;

    if (sec) {
      const kp = decodeSec(sec);
      userPubkeyHex = kp.pubkey;
      userPrivkeyHex = bytesToHex(kp.secretKey);
    }

    const result = checkRefundEligibility(token, userPubkeyHex);

    if (!result.isP2PK) {
      console.error("This token does not use P2PK locking.");
      console.error("No refund conditions found.");
      if (result.mint) {
        console.error(`Mint: ${result.mint}`);
      }
      process.exit(1);
    }

    console.error("P2PK Lock Details:");
    console.error(`  Locked to:    ${result.lockPubkey ?? "(unknown)"}`);
    console.error(`  Refund keys:  ${result.refundPubkeys.join(", ") || "(none)"}`);

    if (result.locktime !== undefined) {
      console.error(`  Locktime:     ${result.refundDate?.toISOString() ?? result.locktime}`);
    } else {
      console.error("  Locktime:     (permanent — no refund path)");
    }

    if (userPubkeyHex) {
      console.error(`  Your key:     ${result.userIsRefundKey ? "✓ matches refund key" : "✗ does NOT match refund key"}`);
    }

    if (result.locktime === undefined) {
      console.error("\nThis token has no locktime. Refund is not available.");
      process.exit(1);
    }

    if (!result.eligible) {
      const remaining = result.locktime - Math.floor(Date.now() / 1000);
      const days = Math.ceil(remaining / 86400);
      console.error(`\nRefund not yet available. Eligible in ~${days} day(s).`);
      console.error(`  Unlock date: ${result.refundDate?.toISOString()}`);
      process.exit(0);
    }

    console.error(`\n✓ Refund is ELIGIBLE (locktime expired).`);

    if (!wantClaim) {
      console.error("  Run with --claim to reclaim the sats to your wallet.");
      process.exit(0);
    }

    if (!userPrivkeyHex) {
      console.error("\nError: --claim requires a secret key. Use --sec to provide your nsec.");
      process.exit(1);
    }

    if (!result.userIsRefundKey) {
      console.error("\nError: your key does not match the refund key. Cannot reclaim.");
      process.exit(1);
    }

    const effectiveMint = result.mint ?? mintUrl;
    console.error(`\nReclaiming from mint: ${effectiveMint}`);

    try {
      const newToken = await reclaimExpiredToken(token, userPrivkeyHex, effectiveMint);
      console.error("✓ Token reclaimed successfully!");
      console.log(newToken);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      console.error(`✗ Reclaim failed: ${msg}`);
      process.exit(1);
    }
  });
