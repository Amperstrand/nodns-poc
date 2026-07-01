import { Command } from "commander";
import * as dns from "node:dns/promises";
import { decode as nip19Decode } from "nostr-tools/nip19";
import { fetchEvents } from "../lib/nostr.js";
import { DEFAULT_API_BASE, readRelays } from "../lib/constants.js";
import type { DnsRecord } from "../lib/types.js";

function extractNpubFromDomain(domain: string): string | null {
  const parts = domain.split(".");
  if (parts.length >= 3 && parts[0].startsWith("npub1")) {
    return parts[0];
  }
  return null;
}

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

function pubkeyNpubToHex(npub: string): string | null {
  try {
    const decoded = nip19Decode(npub);
    if (decoded.type !== "npub") return null;
    return decoded.data;
  } catch {
    return null;
  }
}

async function resolveViaDns(
  domain: string,
  recordType: string,
): Promise<void> {
  console.log(";; DNS ANSWER:");
  try {
    const type = recordType.toUpperCase();
    if (type === "A" || type === "ANY") {
      const addrs = await dns.resolve4(domain).catch(() => []);
      for (const a of addrs) console.log(`  ${domain}.\t\tA\t${a}`);
    }
    if (type === "AAAA" || type === "ANY") {
      const addrs = await dns.resolve6(domain).catch(() => []);
      for (const aaaa of addrs) console.log(`  ${domain}.\t\tAAAA\t${aaaa}`);
    }
    if (type === "CNAME" || type === "ANY") {
      const cnames = await dns.resolveCname(domain).catch(() => []);
      for (const c of cnames) console.log(`  ${domain}.\t\tCNAME\t${c}`);
    }
    if (type === "TXT" || type === "ANY") {
      const txts = await dns.resolveTxt(domain).catch(() => []);
      for (const t of txts) console.log(`  ${domain}.\t\tTXT\t"${t.join("")}"`);
    }
    if (type === "MX" || type === "ANY") {
      const mxs = await dns.resolveMx(domain).catch(() => []);
      for (const mx of mxs) console.log(`  ${domain}.\t\tMX\t${mx.priority} ${mx.exchange}`);
    }
  } catch (e) {
    console.error(`  (dns query failed: ${e instanceof Error ? e.message : String(e)})`);
  }
}

async function resolveViaNostr(
  relays: string[],
  npubStr: string,
): Promise<void> {
  console.log(";; NOSTR EVENTS (kind 11111):");
  const hex = pubkeyNpubToHex(npubStr);
  if (!hex) {
    console.error("  (invalid npub)");
    return;
  }

  try {
    const events = await fetchEvents(relays, hex);
    if (events.length === 0) {
      console.log(`  (no events found for ${npubStr})`);
      return;
    }
    for (const ev of events) {
      const records = extractRecords(ev.tags);
      for (const r of records) {
        const name = r.name || "@";
        console.log(`  ${name}\t${r.type}\t${r.ttl}\t${r.rdata}`);
      }
    }
  } catch (e) {
    console.error(`  (nostr query failed: ${e instanceof Error ? e.message : String(e)})`);
  }
}

interface ApiRecord {
  name: string;
  fqdn: string;
  type: string;
  ttl: number;
  rdata: string;
}

async function resolveViaApi(
  apiBase: string,
  domain: string,
  npubStr: string | null,
): Promise<void> {
  console.log(`;; API RECORDS (${apiBase}):`);
  try {
    let url: string;
    if (npubStr) {
      url = `${apiBase}/api/records/by-npub/${npubStr}`;
    } else {
      url = `${apiBase}/api/records?domain=${encodeURIComponent(domain)}`;
    }
    const res = await fetch(url, { signal: AbortSignal.timeout(10_000) });
    if (!res.ok) {
      console.error(`  (api query failed: HTTP ${res.status})`);
      console.error("  ;; falling back to DNS...");
      await resolveViaDns(domain, "ANY");
      return;
    }
    const data = (await res.json()) as { records?: ApiRecord[]; count?: number };
    const records = data.records ?? [];
    if (records.length === 0) {
      console.log(`  (no records found for ${domain})`);
      return;
    }
    for (const r of records) {
      const name = r.name || "@";
      console.log(`  ${name}\t${r.type}\t${r.ttl}\t${r.rdata}\t${r.fqdn}`);
    }
    console.log(`  ;; ${data.count ?? records.length} record(s)`);
  } catch (e) {
    console.error(`  (api query failed: ${e instanceof Error ? e.message : String(e)})`);
    console.error("  ;; falling back to DNS...");
    await resolveViaDns(domain, "ANY");
  }
}

export const resolveCommand = new Command("resolve")
  .description("Resolve a DNS name")
  .argument("<domain>", "Domain name to resolve")
  .option("-t, --type <type>", "Record type", "ANY")
  .option("--dns", "Query DNS only")
  .option("--nostr", "Query Nostr relays for raw events")
  .option("--api-base <url>", "Bot API base URL", DEFAULT_API_BASE)
  .action(async (domain: string, opts, cmd: Command) => {
    const o = cmd.optsWithGlobals();
    const relays = readRelays(o.relay);
    const recordType = (opts.type as string).toUpperCase();
    const npubStr = extractNpubFromDomain(domain);

    if (opts.dns as boolean) {
      await resolveViaDns(domain, recordType);
      return;
    }

    if (opts.nostr as boolean) {
      if (!npubStr) {
        console.error("Cannot extract npub from domain. Use <npub>.nodns.shop format.");
        return;
      }
      await resolveViaNostr(relays, npubStr);
      console.log();
      return;
    }

    await resolveViaApi(opts.apiBase as string, domain, npubStr);
  });
