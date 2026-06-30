import { SimplePool } from 'nostr-tools/pool';
import type { NostrEvent } from 'nostr-tools/pure';
import type {
  DiscoveredZone,
  ZoneCheckOutcome,
  ZoneInfo,
  ZonePricing,
  ZoneStatusLevel,
} from './types.js';
import { DEFAULT_DOH_ENDPOINT, ZONE_HANDLER_KIND } from './types.js';

const QUERY_MAX_WAIT = 6_000;
const DOH_TIMEOUT_MS = 10_000;

export function parseZoneTxt(txt: string): Record<string, string> {
  const result: Record<string, string> = {};
  for (const part of txt.split(';')) {
    const eq = part.indexOf('=');
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
  return typeof data === 'object' && data !== null && 'Status' in data;
}

export async function fetchDnsTxt(
  zone: string,
  dohEndpoint: string = DEFAULT_DOH_ENDPOINT,
): Promise<string | null> {
  const name = `_nodns.${zone}`;
  const url = `${dohEndpoint}?name=${encodeURIComponent(name)}&type=TXT`;
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), DOH_TIMEOUT_MS);
  try {
    const res = await fetch(url, {
      headers: { accept: 'application/dns-json' },
      signal: controller.signal,
    });
    if (!res.ok) return null;
    const data: unknown = await res.json();
    if (!isDohResponse(data) || !data.Answer) return null;
    for (const answer of data.Answer) {
      if (answer.type === 16 && typeof answer.data === 'string') {
        return answer.data
          .replace(/^"/, '')
          .replace(/"$/, '')
          .replace(/"\s*"/g, '');
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
): ZonePricing | undefined {
  const tag = tags.find((t) => t[0] === 'pricing');
  if (!tag) return undefined;
  let create = 0;
  let update = 0;
  let del = 0;
  let found = false;
  for (const entry of tag.slice(1)) {
    const eq = entry.indexOf('=');
    if (eq === -1) continue;
    const key = entry.slice(0, eq).trim();
    const val = parseInt(entry.slice(eq + 1).trim(), 10);
    if (Number.isNaN(val)) continue;
    found = true;
    if (key === 'create') create = val;
    else if (key === 'update') update = val;
    else if (key === 'delete') del = val;
  }
  return found ? { create, update, delete: del } : undefined;
}

function parseStatusTag(
  tags: string[][],
): { status: ZoneStatusLevel; reason?: string } {
  const tag = tags.find((t) => t[0] === 'status' && t[1]);
  if (!tag || !tag[1]) return { status: 'unknown' };
  const value = tag[1].toLowerCase();
  if (value === 'testing' || value === 'preview' || value === 'production') {
    return { status: value, reason: tag[2] };
  }
  return { status: 'unknown' };
}

function eventToPartialZone(event: NostrEvent): DiscoveredZone | null {
  const tags = event.tags;
  const zoneTag = tags.find((t) => t[0] === 'zone' && t[1]);
  if (!zoneTag) return null;
  const zone = zoneTag[1].toLowerCase();
  const pricing = parsePricingTag(tags);
  const { status, reason } = parseStatusTag(tags);
  const testnet = tags.some((t) => t[0] === 'testnet');
  const dnskeyHashTag = tags.find((t) => t[0] === 'dnskey_hash' && t[1]);
  const dnskeyAlgTag = tags.find((t) => t[0] === 'dnskey_alg' && t[1]);
  const mintTag = tags.find((t) => t[0] === 'mint' && t[1]);
  const webTag = tags.find((t) => t[0] === 'web' && t[1]);
  return {
    zone,
    pubkey: event.pubkey,
    status,
    testnet,
    statusReason: reason,
    dnskeyHash: dnskeyHashTag?.[1],
    dnskeyAlg: dnskeyAlgTag?.[1],
    pricing,
    mint: mintTag?.[1],
    web: webTag?.[1],
    verified: false,
  };
}

async function queryZoneHandlerEvents(
  relays: string[],
): Promise<NostrEvent[]> {
  const pool = new SimplePool();
  try {
    const tagged = await pool.querySync(
      relays,
      { kinds: [ZONE_HANDLER_KIND], '#k': ['11111'] },
      { maxWait: QUERY_MAX_WAIT },
    );
    if (tagged.length > 0) return tagged as NostrEvent[];
    return await pool.querySync(
      relays,
      { kinds: [ZONE_HANDLER_KIND] },
      { maxWait: QUERY_MAX_WAIT },
    ) as NostrEvent[];
  } finally {
    pool.close(relays);
  }
}

export async function discoverZones(
  relays: string[],
  dohEndpoint: string = DEFAULT_DOH_ENDPOINT,
): Promise<DiscoveredZone[]> {
  let events: NostrEvent[];
  try {
    events = await queryZoneHandlerEvents(relays);
  } catch {
    return [];
  }

  const byZone = new Map<string, { event: NostrEvent; ts: number }>();
  for (const event of events) {
    const kTags = event.tags.filter((t) => t[0] === 'k').map((t) => t[1]);
    if (kTags.length > 0 && !kTags.includes('11111')) continue;
    const zoneTag = event.tags.find((t) => t[0] === 'zone' && t[1]);
    if (!zoneTag) continue;
    const zone = zoneTag[1].toLowerCase();
    const existing = byZone.get(zone);
    if (!existing || event.created_at > existing.ts) {
      byZone.set(zone, { event, ts: event.created_at });
    }
  }

  const zones: DiscoveredZone[] = [];
  for (const { event } of byZone.values()) {
    const partial = eventToPartialZone(event);
    if (partial) zones.push(partial);
  }

  await Promise.allSettled(
    zones.map(async (zone) => {
      const txt = await fetchDnsTxt(zone.zone, dohEndpoint);
      if (!txt) {
        zone.verificationError = 'No _nodns TXT record found';
        return;
      }
      const parsed = parseZoneTxt(txt);
      const txtNpub = parsed['npub'];
      if (!txtNpub) {
        zone.verificationError = 'TXT record missing npub field';
        return;
      }
      if (txtNpub.toLowerCase() !== zone.pubkey.toLowerCase()) {
        zone.verificationError = 'TXT npub does not match event signer';
        return;
      }
      if (parsed['testnet'] === '1') {
        zone.testnet = true;
      }
      zone.verified = true;
    }),
  );

  zones.sort((a, b) => a.zone.localeCompare(b.zone));
  return zones;
}

function buildZoneInfo(
  parsed: Record<string, string>,
  zone: string,
): ZoneInfo {
  const npub = parsed['npub'] ?? '';
  const testnet = parsed['testnet'] === '1' || parsed['testnet'] === 'true';
  const npubNamesFree =
    parsed['npub_free'] === undefined
      ? true
      : parsed['npub_free'] === 'true' || parsed['npub_free'] === '1';

  const createPrice = parseInt(parsed['create'] ?? '0', 10) || 0;
  const updatePrice = parseInt(parsed['update'] ?? '0', 10) || 0;
  const deletePrice = parseInt(parsed['delete'] ?? '0', 10) || 0;

  const pricing: ZonePricing | undefined =
    createPrice > 0 || updatePrice > 0 || deletePrice > 0
      ? { create: createPrice, update: updatePrice, delete: deletePrice }
      : undefined;

  let mintUrl = parsed['mint'];
  if (mintUrl && !mintUrl.startsWith('http')) {
    mintUrl = `https://${mintUrl}`;
  }

  return {
    zone,
    npub,
    testnet,
    optedIn: true,
    pricing,
    mintUrl,
    npubNamesFree,
    handlerEventFound: false,
    verified: false,
  };
}

async function fetchZoneHandler(
  relays: string[],
  pubkeyHex: string,
): Promise<NostrEvent | null> {
  const pool = new SimplePool();
  try {
    const events = await pool.querySync(
      relays,
      { kinds: [ZONE_HANDLER_KIND], authors: [pubkeyHex], limit: 1 },
      { maxWait: QUERY_MAX_WAIT },
    );
    return events.length > 0 ? (events[0] as NostrEvent) : null;
  } catch {
    return null;
  } finally {
    pool.close(relays);
  }
}

export async function checkZone(
  zone: string,
  relays: string[],
  opts?: { skipCheck?: boolean; dohEndpoint?: string },
): Promise<ZoneCheckOutcome> {
  const dohEndpoint = opts?.dohEndpoint ?? DEFAULT_DOH_ENDPOINT;

  if (opts?.skipCheck) {
    return {
      result: 'verified',
      info: {
        zone,
        npub: '',
        testnet: false,
        optedIn: true,
        npubNamesFree: true,
        handlerEventFound: true,
        verified: true,
      },
    };
  }

  const txt = await fetchDnsTxt(zone, dohEndpoint);
  if (!txt) {
    return { result: 'not-opted-in' };
  }

  const parsed = parseZoneTxt(txt);
  const info = buildZoneInfo(parsed, zone);

  if (!info.npub) {
    return { result: 'unverified', info };
  }

  const handler = await fetchZoneHandler(relays, info.npub);
  if (handler && handler.pubkey.toLowerCase() === info.npub.toLowerCase()) {
    info.handlerEventFound = true;
    info.verified = true;
    info.optedIn = true;
    if (info.testnet) {
      return { result: 'testnet', info };
    }
    return { result: 'verified', info };
  }

  info.handlerEventFound = handler !== null;
  info.verified = false;
  return { result: 'unverified', info };
}
