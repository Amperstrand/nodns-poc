import { Command } from "commander";
import { parseZoneTxt, fetchDnsTxt } from "../lib/zones.js";
import { SimplePool } from "nostr-tools/pool";
import { ZONE_HANDLER_KIND } from "../lib/constants.js";
import type { NostrEvent } from "nostr-tools/pure";

function fmtPricing(info: { createPrice: number; updatePrice: number; deletePrice: number }): string {
  return `create=${info.createPrice} update=${info.updatePrice} delete=${info.deletePrice}`;
}

async function fetchHandlerDetails(
  relays: string[],
  pubkeyHex: string,
): Promise<NostrEvent | null> {
  const pool = new SimplePool();
  try {
    const events = await pool.querySync(
      relays,
      { kinds: [ZONE_HANDLER_KIND], authors: [pubkeyHex], limit: 1 },
      { maxWait: 6000 },
    );
    return events.length > 0 ? events[0] : null;
  } catch {
    return null;
  } finally {
    pool.close(relays);
  }
}

export const zoneCheckCommand = new Command("zone-check")
  .description("Check if a zone has opted in to nodns")
  .argument("[zone]", "Zone to check")
  .action(async (zoneArg: string | undefined, _opts, cmd: Command) => {
    const o = cmd.optsWithGlobals();
    const relay = o.relay ?? "wss://relay.cashu.email";
    const relays = [relay];
    const zone = zoneArg ?? o.zone ?? "nodns.shop";

    console.error(`Checking zone '${zone}'...\n`);

    const txt = await fetchDnsTxt(zone);
    if (!txt) {
      console.log(`✗ Zone '${zone}' has NOT opted in.`);
      console.log(`  No _nodns.${zone} TXT record found.`);
      console.log(`  DNS records published to this zone will NOT resolve.`);
      console.log(`\n  To opt in, publish a TXT record at _nodns.${zone}:`);
      console.log(`    v=2;npub=YOUR_HEX_PUBKEY`);
      return;
    }

    console.log(`TXT record found at _nodns.${zone}:`);
    console.log(`  ${txt}\n`);

    const parsed = parseZoneTxt(txt);
    const npubHex = parsed["npub"] ?? "";
    const testnet = parsed["testnet"] === "1" || parsed["testnet"] === "true";
    const npubNamesFree = parsed["npub_free"] === undefined
      ? true
      : parsed["npub_free"] === "true" || parsed["npub_free"] === "1";

    const createPrice = parseInt(parsed["create"] ?? "0", 10) || 0;
    const updatePrice = parseInt(parsed["update"] ?? "0", 10) || 0;
    const deletePrice = parseInt(parsed["delete"] ?? "0", 10) || 0;
    const hasPricing = createPrice > 0 || updatePrice > 0 || deletePrice > 0;

    let mintUrl = parsed["mint"];
    if (mintUrl && !mintUrl.startsWith("http")) {
      mintUrl = `https://${mintUrl}`;
    }

    console.log("Zone details:");
    console.log(`  npub:       ${npubHex || "(not set)"}`);
    console.log(`  testnet:    ${testnet ? "yes" : "no"}`);
    console.log(`  npub free:  ${npubNamesFree ? "yes" : "no"}`);
    if (hasPricing) {
      console.log(`  pricing:    ${fmtPricing({ createPrice, updatePrice, deletePrice })}`);
    }
    if (mintUrl) {
      console.log(`  mint:       ${mintUrl}`);
    }

    if (!npubHex) {
      console.log(`\n⚠ Zone '${zone}' TXT found but missing npub field. Verification failed.`);
      return;
    }

    console.error(`\nChecking Nostr handler event (kind ${ZONE_HANDLER_KIND})...`);
    const handler = await fetchHandlerDetails(relays, npubHex);

    if (handler && handler.pubkey.toLowerCase() === npubHex.toLowerCase()) {
      console.log(`\n✓ Zone '${zone}' VERIFIED`);
      console.log(`  Handler event found: ${handler.id}`);
      console.log(`  Signer matches TXT npub: yes`);
      if (testnet) {
        console.log(`\n⚠ WARNING: Zone '${zone}' is on TESTNET.`);
        console.log(`  Records may be temporary. This zone is for testing only.`);
      }
    } else if (handler) {
      console.log(`\n⚠ Zone '${zone}' UNVERIFIED`);
      console.log(`  Handler event found but signer does not match TXT npub.`);
      console.log(`  TXT npub:     ${npubHex}`);
      console.log(`  Event signer: ${handler.pubkey}`);
    } else {
      console.log(`\n⚠ Zone '${zone}' UNVERIFIED`);
      console.log(`  No handler event found from npub ${npubHex.slice(0, 16)}...`);
      console.log(`  The TXT record exists but the zone operator has not published`);
      console.log(`  a kind ${ZONE_HANDLER_KIND} Nostr event.`);
    }
  });
