import type { NostrEvent } from "nostr-tools/pure";
import { npubEncode } from "nostr-tools/nip19";
import { SimplePool } from "nostr-tools/pool";
import { RECORD_KIND, QUERY_MAX_WAIT } from "./constants.js";

export interface ZoneRecord {
  name: string;
  type: string;
  ttl: number;
  rdata: string;
  npub: string;
  event_id: string;
  created_at: number;
}

const TYPE_ORDER: Record<string, number> = {
  A: 0,
  AAAA: 1,
  CNAME: 2,
  MX: 3,
  TXT: 4,
};

const DEFAULT_TTL = 3600;
const SOA_REFRESH = 3600;
const SOA_RETRY = 600;
const SOA_EXPIRE = 86400;
const SOA_MINIMUM = 3600;

function safeNpubEncode(pubkey: string): string {
  try {
    return npubEncode(pubkey);
  } catch {
    return pubkey.slice(0, 16);
  }
}

function isNpubDerivedName(name: string): boolean {
  return name === "" || name === "@";
}

export function compareRecords(a: ZoneRecord, b: ZoneRecord): number {
  const nameCmp = a.name.toLowerCase().localeCompare(b.name.toLowerCase());
  if (nameCmp !== 0) return nameCmp;
  const orderA = TYPE_ORDER[a.type.toUpperCase()] ?? 99;
  const orderB = TYPE_ORDER[b.type.toUpperCase()] ?? 99;
  if (orderA !== orderB) return orderA - orderB;
  return a.rdata.localeCompare(b.rdata);
}

function parseRecordTag(
  tag: string[],
  pubkey: string,
  eventId: string,
  createdAt: number,
): ZoneRecord | null {
  if (tag.length === 0 || tag[0] !== "record") return null;

  const npub = safeNpubEncode(pubkey);

  if (tag.length === 5) {
    const type = (tag[1] ?? "").toUpperCase();
    const rawName = tag[2] ?? "";
    const ttlStr = tag[3] ?? "";
    const rdata = tag[4] ?? "";

    let ttl = DEFAULT_TTL;
    const parsed = parseInt(ttlStr, 10);
    if (!Number.isNaN(parsed) && parsed > 0) ttl = parsed;

    const name = isNpubDerivedName(rawName) ? npub : rawName;
    return { name, type, ttl, rdata, npub, event_id: eventId, created_at: createdAt };
  }

  if (tag.length === 11) {
    const type = (tag[1] ?? "").toUpperCase();
    const rawName = tag[2] ?? "";

    const rdataParts: string[] = [];
    for (let i = 3; i <= 9; i++) {
      const part = tag[i] ?? "";
      if (part !== "") rdataParts.push(part);
    }
    const rdata = rdataParts.join(" ");

    let ttl = DEFAULT_TTL;
    const parsed = parseInt(tag[10] ?? "", 10);
    if (!Number.isNaN(parsed) && parsed > 0) ttl = parsed;

    const name = isNpubDerivedName(rawName) ? npub : rawName;
    return { name, type, ttl, rdata, npub, event_id: eventId, created_at: createdAt };
  }

  return null;
}

export function parseEventsToRecords(
  events: NostrEvent[],
  zone: string,
): ZoneRecord[] {
  const zoneSuffix = `.${zone}`;
  const sorted = [...events].sort((a, b) => a.created_at - b.created_at);
  const latest = new Map<string, ZoneRecord>();

  for (const ev of sorted) {
    for (const tag of ev.tags) {
      const record = parseRecordTag(tag, ev.pubkey, ev.id, ev.created_at);
      if (!record) continue;
      const fqdn = `${record.name}${zoneSuffix}`;
      if (!fqdn.endsWith(zoneSuffix)) continue;
      const key = `${record.name.toLowerCase()}|${record.type.toUpperCase()}`;
      latest.set(key, record);
    }
  }

  const records: ZoneRecord[] = [];
  for (const record of latest.values()) {
    if (record.rdata === "") continue;
    records.push(record);
  }

  return records.sort(compareRecords);
}

function escapeTxt(value: string): string {
  return value.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
}

export function formatRdata(type: string, rdata: string): string {
  if (type.toUpperCase() === "TXT") {
    return `"${escapeTxt(rdata)}"`;
  }
  return rdata;
}

function computeSerial(records: ZoneRecord[]): number {
  if (records.length === 0) return 1;
  return Math.max(...records.map((r) => r.created_at));
}

export function generateZoneFile(
  records: ZoneRecord[],
  zone: string,
  opts?: { includeSoa?: boolean },
): string {
  const includeSoa = opts?.includeSoa ?? true;
  const sorted = [...records].sort(compareRecords);
  const serial = computeSerial(sorted);
  const lines: string[] = [];

  lines.push(`$ORIGIN ${zone}.`);
  lines.push(`$TTL ${DEFAULT_TTL}`);
  lines.push("");

  if (includeSoa) {
    lines.push(`${"@".padEnd(20)} IN  SOA  ns1.${zone}. admin.${zone}. (`);
    lines.push(`                                ${serial} ; serial`);
    lines.push(`                                ${SOA_REFRESH}   ; refresh`);
    lines.push(`                                ${SOA_RETRY}    ; retry`);
    lines.push(`                                ${SOA_EXPIRE}  ; expire`);
    lines.push(`                                ${SOA_MINIMUM}   ; minimum`);
    lines.push(`                                )`);
    lines.push("");
  }

  for (const r of sorted) {
    const nameField = r.name.padEnd(20);
    const typeField = r.type.padEnd(5);
    const rdata = formatRdata(r.type, r.rdata);
    lines.push(`${nameField} IN  ${typeField} ${r.ttl}  ${rdata}`);
  }

  return lines.join("\n") + "\n";
}

export async function fetchZoneEvents(relays: string[]): Promise<NostrEvent[]> {
  const pool = new SimplePool();
  try {
    const events = await pool.querySync(
      relays,
      { kinds: [RECORD_KIND], limit: 1000 },
      { maxWait: QUERY_MAX_WAIT * 5 },
    );
    return events;
  } finally {
    pool.close(relays);
  }
}

export interface ApiRecordResponse {
  npub: string;
  name: string;
  fqdn: string;
  type: string;
  ttl: number;
  rdata: string;
  created_at: number;
}

export interface RecordsApiResponse {
  records: ApiRecordResponse[];
  count: number;
}

export function apiRecordsToZoneRecords(
  records: ApiRecordResponse[],
  zone: string,
): ZoneRecord[] {
  const zoneSuffix = `.${zone}`;
  const result: ZoneRecord[] = [];

  for (const r of records) {
    const fqdn = r.fqdn.replace(/\.$/, "");
    if (!fqdn.endsWith(zoneSuffix)) continue;
    const name = fqdn.slice(0, -zoneSuffix.length);
    if (!name) continue;
    result.push({
      name,
      type: r.type.toUpperCase(),
      ttl: r.ttl || DEFAULT_TTL,
      rdata: r.rdata,
      npub: r.npub,
      event_id: "api",
      created_at: r.created_at,
    });
  }

  return result.sort(compareRecords);
}

export async function fetchApiRecords(
  apiBase: string,
  zone: string,
): Promise<ZoneRecord[]> {
  const url = `${apiBase}/api/records`;
  const res = await fetch(url, { signal: AbortSignal.timeout(30_000) });
  if (!res.ok) {
    throw new Error(`API returned HTTP ${res.status}`);
  }
  const data = (await res.json()) as RecordsApiResponse;
  return apiRecordsToZoneRecords(data.records ?? [], zone);
}

export interface RecordDiff {
  onlyInA: ZoneRecord[];
  onlyInB: ZoneRecord[];
  ttlMismatch: { a: ZoneRecord; b: ZoneRecord }[];
  matching: number;
}

export function diffRecords(a: ZoneRecord[], b: ZoneRecord[]): RecordDiff {
  const mapA = new Map<string, ZoneRecord>();
  const mapB = new Map<string, ZoneRecord>();

  for (const r of a) {
    mapA.set(`${r.name.toLowerCase()}|${r.type.toUpperCase()}|${r.rdata}`, r);
  }
  for (const r of b) {
    mapB.set(`${r.name.toLowerCase()}|${r.type.toUpperCase()}|${r.rdata}`, r);
  }

  const onlyInA: ZoneRecord[] = [];
  const onlyInB: ZoneRecord[] = [];
  const ttlMismatch: { a: ZoneRecord; b: ZoneRecord }[] = [];
  let matching = 0;

  for (const [key, recA] of mapA) {
    if (mapB.has(key)) {
      matching++;
    } else {
      const partialKey = `${recA.name.toLowerCase()}|${recA.type.toUpperCase()}`;
      let foundTtlMismatch = false;
      for (const [bkey, recB] of mapB) {
        if (bkey.startsWith(`${partialKey}|`) && recB.rdata === recA.rdata && recB.ttl !== recA.ttl) {
          ttlMismatch.push({ a: recA, b: recB });
          foundTtlMismatch = true;
          break;
        }
      }
      if (!foundTtlMismatch) onlyInA.push(recA);
    }
  }

  for (const [key, recB] of mapB) {
    if (!mapA.has(key)) {
      const partialKey = `${recB.name.toLowerCase()}|${recB.type.toUpperCase()}`;
      let isTtlMismatch = false;
      for (const [akey] of mapA) {
        if (akey.startsWith(`${partialKey}|`) && akey.endsWith(`|${recB.rdata}`)) {
          isTtlMismatch = true;
          break;
        }
      }
      if (!isTtlMismatch) onlyInB.push(recB);
    }
  }

  return { onlyInA, onlyInB, ttlMismatch, matching };
}
