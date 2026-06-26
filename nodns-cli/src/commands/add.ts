import { Command } from "commander";
import { signAndPublish, buildRecordTag, buildCashuTag, decodeSec } from "../lib/nostr.js";
import { checkZone } from "../lib/zones.js";
import { validateRecord } from "../lib/validation.js";
import { createP2pkTokenWithRefund } from "../lib/p2pk.js";
import { DEFAULT_MINT_URL } from "../lib/constants.js";
import type { ZoneInfo } from "../lib/types.js";

function sleep(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms));
}

async function runZoneGate(
  zone: string,
  relays: string[],
  skip: boolean,
  force: boolean,
): Promise<ZoneInfo | null> {
  const outcome = await checkZone(zone, relays, skip);

  if (outcome.result === "not-opted-in") {
    console.error(`✗ Zone '${zone}' has NOT opted in.`);
    console.error(`  No _nodns.${zone} TXT record found. DNS records will not resolve.`);
    console.error(`  Publish a TXT record at _nodns.${zone} with: v=2;npub=YOUR_HEX_PUBKEY`);
    process.exit(1);
  }

  if (outcome.result === "unverified") {
    console.error(`⚠ Zone '${zone}' TXT record found but verification failed.`);
    console.error(`  TXT npub: ${outcome.info.npub}`);
    if (!force) {
      console.error("  Continuing in 3s... (Ctrl+C to abort)");
      await sleep(3000);
    }
    return outcome.info;
  }

  if (outcome.result === "testnet") {
    console.error(`⚠ Zone '${zone}' is on TESTNET.`);
    console.error(`  Records may be temporary. This zone is for testing only.`);
    if (!force) {
      console.error("  Continuing in 3s... (Ctrl+C to abort)");
      await sleep(3000);
    }
    return outcome.info;
  }

  const npubPreview = outcome.info.npub
    ? `${outcome.info.npub.slice(0, 16)}...`
    : "(unknown)";
  console.error(`✓ Zone '${zone}' verified (npub: ${npubPreview})`);
  return outcome.info;
}

export const addCommand = new Command("add")
  .description("Add a DNS record")
  .requiredOption("-t, --type <type>", "Record type (A, AAAA, CNAME, TXT, MX)")
  .requiredOption("-d, --data <data>", "Record data (IP, hostname, text)")
  .option("-n, --name <name>", "Subdomain name (empty for npub-derived)", "")
  .option("--ttl <seconds>", "TTL in seconds", "3600")
  .option("--dry-run", "Print event without publishing")
  .option("--force", "Skip testnet warning delay")
  .option("--refund-days <days>", "Days until Cashu refund is available", "7")
  .action(async (opts, cmd: Command) => {
    const o = cmd.optsWithGlobals();
    const relay = o.relay ?? "wss://relay.cashu.email";
    const relays = [relay];
    const zone = o.zone ?? "nodns.shop";
    const sec: string | undefined = o.sec;
    const skipZoneCheck: boolean = o.skipZoneCheck ?? false;

    const recordType = (opts.type as string).toUpperCase();
    const name = (opts.name as string) ?? "";
    const data = opts.data as string;
    const ttl = parseInt(opts.ttl as string, 10) || 3600;
    const dryRun: boolean = opts.dryRun ?? false;
    const force: boolean = opts.force ?? false;
    const refundDays: number = parseInt(opts.refundDays as string, 10) || 7;

    const validationError = validateRecord(recordType, name, data);
    if (validationError) {
      console.error(`Error: ${validationError}`);
      process.exit(1);
    }

    if (!sec) {
      console.error("Error: no secret key provided. Use --sec, NODNS_SECRET_KEY, or config file");
      process.exit(1);
    }

    const kp = decodeSec(sec);

    let zoneInfo: ZoneInfo | null = null;
    if (!dryRun && !skipZoneCheck) {
      zoneInfo = await runZoneGate(zone, relays, false, force);
    }

    const tags: string[][] = [buildRecordTag(recordType, name, data, ttl)];

    if (zoneInfo && !dryRun) {
      const isNpubName = name === "";
      const needsPayment =
        !(zoneInfo.npubNamesFree && isNpubName) &&
        zoneInfo.pricing !== undefined &&
        zoneInfo.pricing.createPrice > 0;

      if (needsPayment) {
        const mintUrl = zoneInfo.mintUrl ?? DEFAULT_MINT_URL;
        const price = zoneInfo.pricing!.createPrice;
        console.error(`\nCustom name requires ${price} sats payment via Cashu.`);
        console.error(`Creating P2PK-locked token with ${refundDays}-day refund condition.`);
        console.error(`Zone owner must claim within ${refundDays} days to confirm registration.`);

        const { token, refundDate, p2pk } = await createP2pkTokenWithRefund({
          zonePubkeyHex: zoneInfo.npub,
          userPubkeyHex: kp.pubkey,
          refundAfterDays: refundDays,
          amountSats: price,
          mintUrl,
        });

        if (p2pk) {
          console.error(`✓ P2PK token created. Refund eligible after: ${refundDate.toISOString()}`);
        } else {
          console.error(`✓ Token created (unlocked — mint lacks P2PK support).`);
        }
        console.error();

        tags.push(buildCashuTag(token, mintUrl, price));
      }
    }

    await signAndPublish(kp.secretKey, relays, tags, "", dryRun);
    if (!dryRun) {
      console.error(`\nRecord live at ${kp.npub}.${zone}`);
    }
  });
