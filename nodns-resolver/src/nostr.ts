import { SimplePool } from 'nostr-tools/pool';
import { decode, npubEncode } from 'nostr-tools/nip19';
import type { NostrEvent, NostrDnsRecord } from './types.js';
import { RECORD_KIND } from './types.js';
import { parseRecordsFromEvent, deduplicateRecords } from './parse.js';

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
