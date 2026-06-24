import { Command } from "commander";
import { decode as nip19Decode } from "nostr-tools/nip19";
import { hexToBytes } from "nostr-tools/utils";
import { getPublicKey } from "nostr-tools/pure";
import { fetchEvents } from "../lib/nostr.js";
import type { DnsRecord } from "../lib/types.js";

function extractRecords(tags: string[][]): DnsRecord[] {
  const records: DnsRecord[] = [];
  for (const tag of tags) {
    if (tag.length >= 5 && tag[0] === "record") {
      records.push({
        type: tag[1],
        name: tag[2],
        ttl: parseInt(tag[3], 10) || 3600,
        rdata: tag[4],
      });
    }
  }
  return records;
}

function resolvePubkeyHex(npubOrHex: string): string {
  if (npubOrHex.startsWith("npub1")) {
    const decoded = nip19Decode(npubOrHex);
    if (decoded.type !== "npub") throw new Error("Invalid npub");
    return decoded.data;
  }
  return npubOrHex;
}

export const listCommand = new Command("list")
  .description("List your DNS records from relays")
  .option("--npub <npub>", "List records for this npub (no --sec needed)")
  .action(async (opts, cmd: Command) => {
    const o = cmd.optsWithGlobals();
    const relay = o.relay ?? "wss://relay.cashu.email";
    const relays = [relay];
    const sec: string | undefined = o.sec;

    let pubkeyHex: string;

    if (opts.npub as string | undefined) {
      try {
        pubkeyHex = resolvePubkeyHex(opts.npub as string);
      } catch {
        console.error("Error: invalid npub or hex pubkey");
        process.exit(1);
      }
    } else {
      if (!sec) {
        console.error("Error: no secret key provided. Use --sec, or --npub to list another key's records");
        process.exit(1);
      }
      let sk: Uint8Array;
      if (sec.startsWith("nsec1")) {
        const decoded = nip19Decode(sec);
        if (decoded.type !== "nsec") {
          console.error("Error: invalid nsec");
          process.exit(1);
        }
        sk = decoded.data as Uint8Array;
      } else {
        sk = hexToBytes(sec);
      }
      pubkeyHex = getPublicKey(sk);
    }

    let events;
    try {
      events = await fetchEvents(relays, pubkeyHex);
    } catch (e) {
      console.error(`Error fetching events: ${e instanceof Error ? e.message : String(e)}`);
      process.exit(1);
    }

    if (events.length === 0) {
      console.error("No records found.");
      return;
    }

    const allRecords: DnsRecord[] = [];
    for (const ev of events) {
      allRecords.push(...extractRecords(ev.tags));
    }

    if (allRecords.length === 0) {
      console.error("No DNS records found in events.");
      return;
    }

    console.log(
      `${"NAME".padEnd(20)} ${"TYPE".padEnd(6)} ${"TTL".padEnd(8)} DATA`,
    );
    console.log("-".repeat(60));

    for (const r of allRecords) {
      const displayName = r.name || "@";
      console.log(
        `${displayName.padEnd(20)} ${r.type.padEnd(6)} ${String(r.ttl).padEnd(8)} ${r.rdata}`,
      );
    }
  });
