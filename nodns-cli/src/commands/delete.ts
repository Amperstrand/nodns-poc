import { Command } from "commander";
import { signAndPublish, buildDeleteTag, decodeSec } from "../lib/nostr.js";
import { checkZone } from "../lib/zones.js";
import { publishRelays } from "../lib/constants.js";

function sleep(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms));
}

export const deleteCommand = new Command("delete")
  .description("Delete a DNS record")
  .requiredOption("-t, --type <type>", "Record type (A, AAAA, CNAME, TXT, MX)")
  .option("-n, --name <name>", "Subdomain name (empty for npub-derived)", "")
  .option("--dry-run", "Print event without publishing")
  .option("--force", "Skip testnet warning delay")
  .option("--pow <bits>", "PoW difficulty in bits (0 to disable)", "20")
  .action(async (opts, cmd: Command) => {
    const o = cmd.optsWithGlobals();
    const relays = publishRelays(o.relay);
    const zone = o.zone ?? "nodns.shop";
    const sec: string | undefined = o.sec;
    const skipZoneCheck: boolean = o.skipZoneCheck ?? false;

    const recordType = (opts.type as string).toUpperCase();
    const name = (opts.name as string) ?? "";
    const dryRun: boolean = opts.dryRun ?? false;
    const force: boolean = opts.force ?? false;
    const powDifficulty: number = parseInt(opts.pow as string, 10);
    if (isNaN(powDifficulty) || powDifficulty < 0) {
      console.error("Error: --pow must be a non-negative integer");
      process.exit(1);
    }

    if (!dryRun && !skipZoneCheck) {
      const outcome = await checkZone(zone, relays, false);

      if (outcome.result === "not-opted-in") {
        console.error(`✗ Zone '${zone}' has NOT opted in.`);
        console.error(`  No _nodns.${zone} TXT record found.`);
        process.exit(1);
      }

      if (outcome.result === "unverified") {
        console.error(`⚠ Zone '${zone}' TXT found but verification failed.`);
        if (!force) {
          console.error("  Continuing in 3s... (Ctrl+C to abort)");
          await sleep(3000);
        }
      } else if (outcome.result === "testnet") {
        console.error(`⚠ Zone '${zone}' is on TESTNET.`);
        console.error(`  Records may be temporary.`);
        if (!force) {
          console.error("  Continuing in 3s... (Ctrl+C to abort)");
          await sleep(3000);
        }
      } else {
        const npubPreview = outcome.info.npub
          ? `${outcome.info.npub.slice(0, 16)}...`
          : "(unknown)";
        console.error(`✓ Zone '${zone}' verified (npub: ${npubPreview})`);
      }
    }

    const tags: string[][] = [buildDeleteTag(recordType, name)];

    if (!sec) {
      console.error("Error: no secret key provided. Use --sec, NODNS_SECRET_KEY, or config file");
      process.exit(1);
    }

    const kp = decodeSec(sec);
    await signAndPublish(kp.secretKey, relays, tags, "", dryRun, powDifficulty);
    if (!dryRun) {
      console.error(`\nDelete published for ${name || "npub-derived"}.${zone}`);
    }
  });
