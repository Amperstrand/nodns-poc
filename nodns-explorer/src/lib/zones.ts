import { SimplePool } from "nostr-tools/pool";
import type { NostrEvent } from "nostr-tools/pure";
import { RELAYS, ZONE_HANDLER_KIND } from "./constants";
import type { ZoneStatus } from "./types";

const DOH_ENDPOINT = "https://dns.google/resolve";
const QUERY_MAX_WAIT = 6000;

export function parseZoneTxt(txt: string): Record<string, string> {
  const result: Record<string, string> = {};
  for (const part of txt.split(";")) {
    const eq = part.indexOf("=");
    if (eq === -1) continue;
    const key = part.slice(0, eq).trim();
    const val = part.slice(eq + 1).trim();
    if (key) result[key] = val;
  }
  return result;
}

interface DohAnswer {
  name: string;
  type: number;
  TTL: number;
  data: string;
}

interface DohResponse {
  Status: number;
  Answer?: DohAnswer[];
}

function isDohResponse(data: unknown): data is DohResponse {
  return typeof data === "object" && data !== null && "Status" in data;
}

export async function fetchDnsTxt(zone: string): Promise<string | null> {
  const name = `_nodns.${zone}`;
  const url = `${DOH_ENDPOINT}?name=${encodeURIComponent(name)}&type=TXT`;
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 10_000);
  try {
    const res = await fetch(url, {
      headers: { accept: "application/dns-json" },
      signal: controller.signal,
    });
    if (!res.ok) return null;
    const data: unknown = await res.json();
    if (!isDohResponse(data) || !data.Answer) return null;
    for (const answer of data.Answer) {
      if (answer.type === 16 && typeof answer.data === "string") {
        return answer.data
          .replace(/^"/, "")
          .replace(/"$/, "")
          .replace(/"\s*"/g, "");
      }
    }
    return null;
  } catch {
    return null;
  } finally {
    clearTimeout(timeout);
  }
}

function parsePricingTag(
  tags: string[][],
): { create: number; update: number; delete: number } | undefined {
  const tag = tags.find((t) => t[0] === "pricing");
  if (!tag) return undefined;
  let create = 0;
  let update = 0;
  let del = 0;
  let found = false;
  for (const entry of tag.slice(1)) {
    const eq = entry.indexOf("=");
    if (eq === -1) continue;
    const key = entry.slice(0, eq).trim();
    const val = parseInt(entry.slice(eq + 1).trim(), 10);
    if (Number.isNaN(val)) continue;
    found = true;
    if (key === "create") create = val;
    else if (key === "update") update = val;
    else if (key === "delete") del = val;
  }
  return found ? { create, update, delete: del } : undefined;
}

function parseStatusTag(
  tags: string[][],
): { status: ZoneStatus["status"]; reason?: string } {
  const tag = tags.find((t) => t[0] === "status" && t[1]);
  if (!tag || !tag[1]) return { status: "unknown" };
  const value = tag[1].toLowerCase();
  if (value === "testing" || value === "preview" || value === "production") {
    return { status: value, reason: tag[2] };
  }
  return { status: "unknown" };
}

function eventToPartialZone(event: NostrEvent): ZoneStatus | null {
  const tags = event.tags;
  const zoneTag = tags.find((t) => t[0] === "zone" && t[1]);
  if (!zoneTag) return null;
  const zone = zoneTag[1].toLowerCase();
  const pricing = parsePricingTag(tags);
  const { status, reason } = parseStatusTag(tags);
  const testnet = tags.some((t) => t[0] === "testnet");
  const mintTag = tags.find((t) => t[0] === "mint" && t[1]);
  const webTag = tags.find((t) => t[0] === "web" && t[1]);
  return {
    zone,
    pubkey: event.pubkey,
    status,
    testnet,
    statusReason: reason,
    pricing,
    mint: mintTag?.[1],
    web: webTag?.[1],
    verified: false,
  };
}

async function queryRelays(): Promise<NostrEvent[]> {
  const pool = new SimplePool();
  try {
    const tagged = await pool.querySync(
      RELAYS,
      { kinds: [ZONE_HANDLER_KIND], "#k": ["11111"] },
      { maxWait: QUERY_MAX_WAIT },
    );
    if (tagged.length > 0) return tagged;
    return await pool.querySync(
      RELAYS,
      { kinds: [ZONE_HANDLER_KIND] },
      { maxWait: QUERY_MAX_WAIT },
    );
  } finally {
    pool.close(RELAYS);
  }
}

export async function discoverZones(): Promise<ZoneStatus[]> {
  let events: NostrEvent[] = [];
  try {
    events = await queryRelays();
  } catch {
    return [];
  }

  const byZone = new Map<string, { event: NostrEvent; ts: number }>();
  for (const event of events) {
    const kTags = event.tags
      .filter((t) => t[0] === "k")
      .map((t) => t[1]);
    if (kTags.length > 0 && !kTags.includes("11111")) continue;
    const zoneTag = event.tags.find((t) => t[0] === "zone" && t[1]);
    if (!zoneTag) continue;
    const zone = zoneTag[1].toLowerCase();
    const existing = byZone.get(zone);
    if (!existing || event.created_at > existing.ts) {
      byZone.set(zone, { event, ts: event.created_at });
    }
  }

  const zones: ZoneStatus[] = [];
  for (const { event } of byZone.values()) {
    const partial = eventToPartialZone(event);
    if (partial) zones.push(partial);
  }

  await Promise.allSettled(
    zones.map(async (zone) => {
      const txt = await fetchDnsTxt(zone.zone);
      if (!txt) {
        zone.verificationError = "No _nodns TXT record found";
        return;
      }
      const parsed = parseZoneTxt(txt);
      const txtNpub = parsed["npub"];
      if (!txtNpub) {
        zone.verificationError = "TXT record missing npub field";
        return;
      }
      if (txtNpub.toLowerCase() !== zone.pubkey.toLowerCase()) {
        zone.verificationError = "TXT npub does not match event signer";
        return;
      }
      if (parsed["testnet"] === "1") {
        zone.testnet = true;
      }
      zone.verified = true;
    }),
  );

  zones.sort((a, b) => a.zone.localeCompare(b.zone));
  return zones;
}
