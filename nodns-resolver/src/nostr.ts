import { SimplePool } from 'nostr-tools/pool';
import { decode, npubEncode, nsecEncode } from 'nostr-tools/nip19';
import { generateSecretKey, getPublicKey } from 'nostr-tools/pure';
import type { NostrEvent, NostrDnsRecord } from './types.js';
import { RECORD_KIND } from './types.js';
import { parseRecordsFromEvent, deduplicateRecords } from './parse.js';

export interface Keypair {
  secretKey: Uint8Array;
  pubkey: string;
  nsec: string;
  npub: string;
}

export function generateKeypair(): Keypair {
  const sk = generateSecretKey();
  const pk = getPublicKey(sk);
  return {
    secretKey: sk,
    pubkey: pk,
    nsec: nsecEncode(sk),
    npub: npubEncode(pk),
  };
}

export function decodeNsec(nsec: string): Keypair {
  const decoded = decode(nsec);
  if (decoded.type !== 'nsec') throw new Error('Not a valid nsec');
  const sk = decoded.data as Uint8Array;
  const pk = getPublicKey(sk);
  return {
    secretKey: sk,
    pubkey: pk,
    nsec,
    npub: npubEncode(pk),
  };
}

export function decodeSec(sec: string): Keypair {
  if (sec.startsWith('nsec1')) {
    return decodeNsec(sec);
  }
  const hexToBytes = (hex: string): Uint8Array => {
    const bytes = new Uint8Array(hex.length / 2);
    for (let i = 0; i < bytes.length; i++) {
      bytes[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
    }
    return bytes;
  };
  const sk = hexToBytes(sec);
  const pk = getPublicKey(sk);
  return {
    secretKey: sk,
    pubkey: pk,
    nsec: nsecEncode(sk),
    npub: npubEncode(pk),
  };
}

export function buildRecordTag(
  recordType: string,
  name: string,
  rdata: string,
  ttl: number = 3600,
): string[] {
  return ['record', recordType.toUpperCase(), name, String(ttl), rdata];
}

export function buildDeleteTag(recordType: string, name: string): string[] {
  return ['record', recordType.toUpperCase(), name, '3600', ''];
}

export function buildCashuTag(
  token: string,
  mintUrl: string,
  amount: number,
): string[] {
  return ['cashu', token, mintUrl, String(amount)];
}

const DEFAULT_QUERY_LIMIT = 100;
const QUERY_MAX_WAIT = 10_000;

function normalizePubkey(pubkey: string): string {
  if (pubkey.startsWith('npub1')) {
    try {
      const decoded = decode(pubkey);
      if (decoded.type === 'npub') {
        return decoded.data as string;
      }
    } catch {
      // fall through
    }
  }
  return pubkey;
}

export async function queryRecordsByPubkey(
  pubkeyHex: string,
  zone: string,
  relays: string[],
  opts?: { limit?: number },
): Promise<NostrDnsRecord[]> {
  const limit = opts?.limit ?? DEFAULT_QUERY_LIMIT;
  const pool = new SimplePool();

  try {
    const events = await pool.querySync(
      relays,
      { kinds: [RECORD_KIND], authors: [pubkeyHex], limit },
      { maxWait: QUERY_MAX_WAIT },
    );

    const allRecords: NostrDnsRecord[] = [];
    for (const ev of events) {
      allRecords.push(...parseRecordsFromEvent(ev as NostrEvent, zone));
    }

    return deduplicateRecords(allRecords);
  } catch {
    return [];
  } finally {
    pool.close(relays);
  }
}

export async function queryRecordsByDomain(
  fqdn: string,
  zone: string,
  relays: string[],
  opts?: { limit?: number },
): Promise<NostrDnsRecord[]> {
  const limit = opts?.limit ?? 500;
  const pool = new SimplePool();

  try {
    const events = await pool.querySync(
      relays,
      { kinds: [RECORD_KIND], limit },
      { maxWait: QUERY_MAX_WAIT },
    );

    const allRecords: NostrDnsRecord[] = [];
    for (const ev of events) {
      const records = parseRecordsFromEvent(ev as NostrEvent, zone);
      for (const r of records) {
        if (r.fqdn === fqdn) {
          allRecords.push(r);
        }
      }
    }

    return deduplicateRecords(allRecords);
  } catch {
    return [];
  } finally {
    pool.close(relays);
  }
}

export async function queryAllRecentRecords(
  zone: string,
  relays: string[],
  opts?: { limit?: number },
): Promise<NostrDnsRecord[]> {
  const limit = opts?.limit ?? 200;
  const pool = new SimplePool();

  try {
    const events = await pool.querySync(
      relays,
      { kinds: [RECORD_KIND], limit },
      { maxWait: QUERY_MAX_WAIT },
    );

    const allRecords: NostrDnsRecord[] = [];
    for (const ev of events) {
      allRecords.push(...parseRecordsFromEvent(ev as NostrEvent, zone));
    }

    return allRecords.sort((a, b) => b.created_at - a.created_at);
  } catch {
    return [];
  } finally {
    pool.close(relays);
  }
}

export async function fetchEvents(
  relays: string[],
  pubkey: string,
  limit: number = 100,
): Promise<NostrEvent[]> {
  const hexPubkey = normalizePubkey(pubkey);
  const pool = new SimplePool();

  try {
    const events = await pool.querySync(
      relays,
      { kinds: [RECORD_KIND], authors: [hexPubkey], limit },
      { maxWait: QUERY_MAX_WAIT },
    );
    return events.sort((a, b) => b.created_at - a.created_at) as NostrEvent[];
  } finally {
    pool.close(relays);
  }
}

export function pubkeyToNpub(pubkey: string): string {
  try {
    return npubEncode(pubkey);
  } catch {
    return pubkey;
  }
}
