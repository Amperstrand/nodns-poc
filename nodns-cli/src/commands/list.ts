import { Command } from "commander";
import { decode as nip19Decode } from "nostr-tools/nip19";
import { hexToBytes } from "nostr-tools/utils";
import { getPublicKey } from "nostr-tools/pure";
import { SimplePool } from "nostr-tools/pool";
import { fetchEvents } from "../lib/nostr.js";
import {
  countLeadingZeroBits,
  DEFAULT_POW_DIFFICULTY,
  POB_PROOF_KIND,
} from "../../../shared/pow.js";
import type { DnsRecord } from "../lib/types.js";

const POB_QUERY_MAX_WAIT = 10_000;

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

async function fetchPobProofs(
  relays: string[],
  eventIds: string[],
): Promise<Map<string, number>> {
  const proofMap = new Map<string, number>();
  if (eventIds.length === 0) return proofMap;
  const pool = new SimplePool();
  try {
    const events = await pool.querySync(
      relays,
      { kinds: [POB_PROOF_KIND], "#e": eventIds, limit: 100 },
      { maxWait: POB_QUERY_MAX_WAIT },
    );
    for (const ev of events) {
      const eTag = ev.tags.find((t) => t[0] === "e" && t.length >= 2);
      const nTag = ev.tags.find((t) => t[0] === "n" && t.length >= 5);
      if (eTag && nTag) {
        const leafValue = parseInt(nTag[4], 10);
        if (!isNaN(leafValue) && !proofMap.has(eTag[1])) {
          proofMap.set(eTag[1], leafValue);
        }
      }
    }
  } finally {
    pool.close(relays);
  }
  return proofMap;
}

interface RecordRow {
  pow: number;
  pob: number | null;
  eventIdShort: string;
  name: string;
  type: string;
  ttl: number;
  rdata: string;
}

export const listCommand = new Command("list")
  .description("List your DNS records from relays (shows PoW/PoB level per event)")
  .option("--npub <npub>", "List records for this npub (no --sec needed)")
  .option("--no-pob", "Skip Proof-of-Burn lookup (faster)")
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

    const eventIds = events.map((e) => e.id);
    const pobProofs = opts.pob
      ? await fetchPobProofs(relays, eventIds)
      : new Map<string, number>();

    const rows: RecordRow[] = [];
    for (const ev of events) {
      const pow = countLeadingZeroBits(ev.id);
      const pob = pobProofs.get(ev.id) ?? null;
      const records = extractRecords(ev.tags);
      for (const r of records) {
        rows.push({
          pow,
          pob,
          eventIdShort: ev.id.slice(0, 8),
          name: r.name || "@",
          type: r.type,
          ttl: r.ttl,
          rdata: r.rdata,
        });
      }
    }

    if (rows.length === 0) {
      console.error("No DNS records found in events.");
      return;
    }

    const header =
      `${"POW".padStart(3)} ${"POB".padStart(5)}  ${"EVENT".padEnd(8)}  ${"NAME".padEnd(20)} ${"TYPE".padEnd(6)} ${"TTL".padEnd(8)} DATA`;
    console.log(header);
    console.log("-".repeat(Math.max(header.length, 72)));

    for (const row of rows) {
      const powStr = String(row.pow).padStart(3);
      const pobStr = (row.pob !== null ? String(row.pob) : "-").padStart(5);
      console.log(
        `${powStr} ${pobStr}  ${row.eventIdShort}  ${row.name.padEnd(20)} ${row.type.padEnd(6)} ${String(row.ttl).padEnd(8)} ${row.rdata}`,
      );
    }

    const passCount = rows.filter(
      (r) => r.pow >= DEFAULT_POW_DIFFICULTY || r.pob !== null,
    ).length;
    console.log(
      `\n${rows.length} record(s) from ${events.length} event(s) — ${passCount} would pass PoW>=${DEFAULT_POW_DIFFICULTY} OR PoB gate`,
    );
  });
