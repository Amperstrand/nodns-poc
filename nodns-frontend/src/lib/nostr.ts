import { SimplePool } from 'nostr-tools/pool';
import { generateSecretKey, getPublicKey, finalizeEvent } from 'nostr-tools/pure';
import { npubEncode, nsecEncode, decode as nip19Decode } from 'nostr-tools/nip19';
import { bytesToHex } from 'nostr-tools/utils';
import { RELAYS } from './constants';
import type { NostrEvent, KeyPair } from './types';

const pool = new SimplePool();

export function generateEphemeralKeyPair(): KeyPair {
  const sk = generateSecretKey();
  const pk = getPublicKey(sk);
  const npub = npubEncode(pk);
  const nsec = nsecEncode(sk);
  return { secretKey: sk, publicKey: pk, npub, nsec };
}

export function keyPairFromNsec(nsec: string): KeyPair {
  const decoded = nip19Decode(nsec);
  if (decoded.type !== 'nsec') {
    throw new Error('Invalid nsec key');
  }
  const sk = decoded.data as Uint8Array;
  const pk = getPublicKey(sk);
  const npub = npubEncode(pk);
  return { secretKey: sk, publicKey: pk, npub, nsec };
}

export function secretKeyToHex(sk: Uint8Array): string {
  return bytesToHex(sk);
}

export function buildRecordTag(type: string, name: string, rdata: string, ttl: number): string[] {
  return ['record', type, name, rdata, '', '', '', '', '', '', String(ttl)];
}

export function buildCashuTag(token: string, mintUrl: string, amount: string): string[] {
  return ['cashu', token, mintUrl, amount];
}

export async function publishDnsEvent(
  records: { type: string; name: string; value: string; ttl: number }[],
  secretKey: Uint8Array,
  cashuToken?: string,
  mintUrl?: string,
  satAmount?: number,
): Promise<NostrEvent> {
  const tags = records.map((r) =>
    buildRecordTag(r.type, r.name || '@', r.value, r.ttl),
  );

  if (cashuToken && mintUrl) {
    tags.push(buildCashuTag(cashuToken, mintUrl, String(satAmount ?? 0)));
  }

  const template = {
    kind: 11111,
    created_at: Math.floor(Date.now() / 1000),
    tags,
    content: '',
  };

  const event = finalizeEvent(template, secretKey);

  const pubs = pool.publish(RELAYS, event);
  await Promise.any(pubs);

  return event as NostrEvent;
}

export async function publishDeleteEvent(
  deletes: { type: string; name: string }[],
  secretKey: Uint8Array,
): Promise<NostrEvent> {
  const tags = deletes.map((d) => ['delete', d.type, d.name || '@']);

  const template = {
    kind: 11111,
    created_at: Math.floor(Date.now() / 1000),
    tags,
    content: '',
  };

  const event = finalizeEvent(template, secretKey);

  const pubs = pool.publish(RELAYS, event);
  await Promise.any(pubs);

  return event as NostrEvent;
}

export function subscribeToDnsEvents(
  onEvent: (event: NostrEvent, relay: string) => void,
): () => void {
  const sub = pool.subscribeMany(
    RELAYS,
    { kinds: [11111], limit: 20 },
    {
      onevent(event) {
        onEvent(event as unknown as NostrEvent, '');
      },
    },
  );

  return () => {
    sub.close();
  };
}

// --- Relay-based record queries ---

export interface NostrDnsRecord {
  type: string;
  name: string;
  value: string;
  ttl: number;
  fqdn: string;
  pubkey: string;
  eventId: string;
  created_at: number;
}

// Record tag: ["record", TYPE, NAME, RDATA, "", "", "", "", "", "", TTL]
export function parseRecordsFromEvent(
  event: NostrEvent,
  zone: string,
): NostrDnsRecord[] {
  const records: NostrDnsRecord[] = [];

  for (const tag of event.tags) {
    if (tag[0] !== 'record') continue;
    if (tag.length < 4) continue;

    const type = tag[1];
    const name = tag[2] || '';
    const value = tag[3];
    const ttl = tag.length >= 11 ? parseInt(tag[10], 10) : 3600;

    if (!type || !value) continue;

    const subdomain = name ? `${name}.${event.pubkey}` : event.pubkey;
    const fqdn = `${subdomain}.${zone}`;

    records.push({
      type,
      name: name || '@',
      value,
      ttl: isNaN(ttl) ? 3600 : ttl,
      fqdn,
      pubkey: event.pubkey,
      eventId: event.id,
      created_at: event.created_at,
    });
  }

  return records;
}

export async function queryRecordsByPubkey(
  pubkeyHex: string,
  zone: string,
  opts?: { limit?: number },
): Promise<NostrDnsRecord[]> {
  const limit = opts?.limit ?? 100;

  try {
    const events = await pool.querySync(RELAYS, {
      kinds: [11111],
      authors: [pubkeyHex],
      limit,
    });

    const allRecords: NostrDnsRecord[] = [];
    for (const ev of events) {
      allRecords.push(...parseRecordsFromEvent(ev as unknown as NostrEvent, zone));
    }

    const seen = new Map<string, NostrDnsRecord>();
    for (const r of allRecords) {
      const key = `${r.fqdn}:${r.type}:${r.name}:${r.value}`;
      const existing = seen.get(key);
      if (!existing || r.created_at > existing.created_at) {
        seen.set(key, r);
      }
    }

    return Array.from(seen.values()).sort((a, b) => b.created_at - a.created_at);
  } catch {
    return [];
  }
}

export async function queryRecordsByDomain(
  fqdn: string,
  zone: string,
  opts?: { limit?: number },
): Promise<NostrDnsRecord[]> {
  const limit = opts?.limit ?? 500;

  try {
    const events = await pool.querySync(RELAYS, {
      kinds: [11111],
      limit,
    });

    const allRecords: NostrDnsRecord[] = [];
    for (const ev of events) {
      const records = parseRecordsFromEvent(ev as unknown as NostrEvent, zone);
      for (const r of records) {
        if (r.fqdn === fqdn) {
          allRecords.push(r);
        }
      }
    }

    const seen = new Map<string, NostrDnsRecord>();
    for (const r of allRecords) {
      const key = `${r.type}:${r.name}:${r.value}`;
      const existing = seen.get(key);
      if (!existing || r.created_at > existing.created_at) {
        seen.set(key, r);
      }
    }

    return Array.from(seen.values()).sort((a, b) => b.created_at - a.created_at);
  } catch {
    return [];
  }
}

export async function queryAllRecentRecords(
  zone: string,
  opts?: { limit?: number },
): Promise<NostrDnsRecord[]> {
  const limit = opts?.limit ?? 200;

  try {
    const events = await pool.querySync(RELAYS, {
      kinds: [11111],
      limit,
    });

    const allRecords: NostrDnsRecord[] = [];
    for (const ev of events) {
      allRecords.push(...parseRecordsFromEvent(ev as unknown as NostrEvent, zone));
    }

    return allRecords.sort((a, b) => b.created_at - a.created_at);
  } catch {
    return [];
  }
}
