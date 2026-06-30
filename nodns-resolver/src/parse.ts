import { npubEncode } from 'nostr-tools/nip19';
import type { NostrEvent, NostrDnsRecord, RecordInfo, SpecVersion, ValidityInfo } from './types.js';
import { DEFAULT_ZONE, VALID_RECORD_TYPES } from './types.js';

function safeNpubEncode(pubkey: string): string {
  try {
    return npubEncode(pubkey);
  } catch {
    return pubkey.slice(0, 16);
  }
}

export function isNpubDerivedName(name: string): boolean {
  return name === '' || name === '@';
}

export function computeFqdn(name: string, pubkey: string, zone: string = DEFAULT_ZONE): string {
  if (isNpubDerivedName(name)) {
    return `${safeNpubEncode(pubkey)}.${zone}`;
  }
  return `${name}.${zone}`;
}

function parseTtl(tag: string[]): number {
  if (tag.length > 10) {
    const parsed = parseInt(tag[10], 10);
    if (!Number.isNaN(parsed) && parsed > 0) return parsed;
  }
  if (tag.length > 4) {
    for (let i = tag.length - 1; i >= 4; i--) {
      const parsed = parseInt(tag[i], 10);
      if (!Number.isNaN(parsed) && parsed > 0) return parsed;
    }
  }
  return 3600;
}

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

    const fqdn = computeFqdn(name, event.pubkey, zone);

    records.push({
      type,
      name: name || '@',
      value,
      ttl: Number.isNaN(ttl) ? 3600 : ttl,
      fqdn,
      pubkey: event.pubkey,
      eventId: event.id,
      created_at: event.created_at,
    });
  }

  return records;
}

export function parseRecords(
  event: NostrEvent,
  zone: string = DEFAULT_ZONE,
): RecordInfo[] {
  const records: RecordInfo[] = [];

  for (const tag of event.tags) {
    if (tag[0] !== 'record') continue;
    if (tag.length < 4) continue;

    const type = (tag[1] ?? '').toUpperCase();
    const name = tag[2] ?? '';
    const rdata = tag[3] ?? '';
    const ttl = parseTtl(tag);
    const isNpubDerived = isNpubDerivedName(name);
    const fqdn = computeFqdn(name, event.pubkey, zone);

    records.push({ type, name, ttl, rdata, fqdn, isNpubDerived });
  }

  return records;
}

export function parseRecord(
  event: NostrEvent,
  zone: string = DEFAULT_ZONE,
): RecordInfo | null {
  const records = parseRecords(event, zone);
  return records.length > 0 ? records[0] : null;
}

function detectSpecVersion(tags: string[][]): SpecVersion {
  const hasAlt = tags.some((t) => t[0] === 'alt');
  const cashuTag = tags.find((t) => t[0] === 'cashu');
  const hasP2PK = cashuTag?.[1]?.includes('P2PK') ?? false;
  if (hasP2PK) return 'v2';
  if (hasAlt) return 'v1.1';
  return 'v1';
}

export function checkValidity(event: NostrEvent): ValidityInfo {
  const tags = event.tags;
  const specVersion = detectSpecVersion(tags);

  const recordTags = tags.filter((t) => t[0] === 'record');

  if (recordTags.length === 0) {
    return { valid: false, reason: 'no record tags', specVersion };
  }

  for (const tag of recordTags) {
    if (tag.length < 4) {
      return { valid: false, reason: `malformed (${tag.length} fields)`, specVersion };
    }
    const type = (tag[1] ?? '').toUpperCase();
    if (!VALID_RECORD_TYPES.includes(type)) {
      return { valid: false, reason: `unknown type: ${type}`, specVersion };
    }
  }

  return { valid: true, specVersion };
}

export function deduplicateRecords(records: NostrDnsRecord[]): NostrDnsRecord[] {
  const seen = new Map<string, NostrDnsRecord>();
  for (const r of records) {
    const key = `${r.fqdn}:${r.type}:${r.name}:${r.value}`;
    const existing = seen.get(key);
    if (!existing || r.created_at > existing.created_at) {
      seen.set(key, r);
    }
  }
  return Array.from(seen.values()).sort((a, b) => b.created_at - a.created_at);
}
