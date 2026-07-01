import type { NostrEvent } from 'nostr-tools/pure';

export type DnsRecordType = 'A' | 'AAAA' | 'CNAME' | 'TXT' | 'MX';

export type ResolutionMode = 'dns' | 'nostr' | 'tripartite';

export const RECORD_KIND = 11111 as const;

export const ZONE_HANDLER_KIND = 31990 as const;

export const DEFAULT_ZONE = 'nodns.shop';

export const DEFAULT_API_BASE = 'https://nodns.shop';

export const DEFAULT_DOH_ENDPOINT = 'https://cloudflare-dns.com/dns-query';

export const DEFAULT_READ_RELAYS: string[] = [
  'wss://relay.cashu.email',
  'wss://relay.damus.io',
  'wss://nos.lol',
  'wss://relay.nostr.band',
  'wss://relay.tollgate.me',
];

export const VALID_RECORD_TYPES: readonly string[] = ['A', 'AAAA', 'CNAME', 'TXT', 'MX'];

export const DNS_TYPE_MAP: Record<number, string> = {
  1: 'A',
  28: 'AAAA',
  5: 'CNAME',
  16: 'TXT',
  15: 'MX',
  2: 'NS',
  6: 'SOA',
};

export interface DohAnswer {
  name: string;
  type: number;
  TTL: number;
  data: string;
}

export interface DohResponse {
  Status: number;
  Answer?: DohAnswer[];
}

export interface DnsAnswer {
  name: string;
  type: string;
  ttl: number;
  data: string;
}

export interface DnsRecord {
  npub: string;
  name: string;
  fqdn: string;
  type: string;
  ttl: number;
  rdata: string;
  created_at: number;
}

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

export interface RecordInfo {
  type: string;
  name: string;
  ttl: number;
  rdata: string;
  fqdn: string;
  isNpubDerived: boolean;
}

export interface ResolvedRecord {
  type: string;
  name: string;
  ttl: number;
  data: string;
  source?: 'dns' | 'nostr';
  pubkey?: string;
  eventId?: string;
}

export type SourceStatus = 'loading' | 'ok' | 'error' | 'unavailable';

export interface SourceResult<T> {
  source: string;
  status: SourceStatus;
  records: T[];
  error?: string;
}

export interface TripartiteRecords {
  api: SourceResult<DnsRecord>;
  nostr: SourceResult<NostrDnsRecord>;
  dns: SourceResult<DnsAnswer>;
}

export interface TripartiteComparison {
  match: boolean;
  apiCount: number;
  nostrCount: number;
  dnsCount: number;
  onlyInApi: string[];
  onlyInNostr: string[];
  onlyInDns: string[];
}

export interface ResolveVerifiedResult {
  records: ResolvedRecord[];
  verified: boolean;
  sources: TripartiteRecords;
  comparison: TripartiteComparison;
}

export interface ReverseResult {
  names: string[];
  records: NostrDnsRecord[];
}

export type SpecVersion = 'v1' | 'v1.1' | 'v2';

export interface ValidityInfo {
  valid: boolean;
  reason?: string;
  specVersion: SpecVersion;
}

export type ZoneStatusLevel = 'testing' | 'preview' | 'production' | 'unknown';

export interface ZonePricing {
  create: number;
  update: number;
  delete: number;
}

export interface DiscoveredZone {
  zone: string;
  pubkey: string;
  status: ZoneStatusLevel;
  testnet: boolean;
  statusReason?: string;
  dnskeyHash?: string;
  dnskeyAlg?: string;
  pricing?: ZonePricing;
  mint?: string;
  web?: string;
  verified: boolean;
  verificationError?: string;
}

export type ZoneCheckOutcome =
  | { result: 'verified'; info: ZoneInfo }
  | { result: 'testnet'; info: ZoneInfo }
  | { result: 'unverified'; info: ZoneInfo }
  | { result: 'not-opted-in' };

export interface ZoneInfo {
  zone: string;
  npub: string;
  testnet: boolean;
  optedIn: boolean;
  pricing?: ZonePricing;
  mintUrl?: string;
  npubNamesFree: boolean;
  handlerEventFound: boolean;
  verified: boolean;
}

export interface ResolverOptions {
  mode?: ResolutionMode;
  relays?: string[];
  apiBase?: string;
  zone?: string;
  dohEndpoint?: string;
}

export type { NostrEvent };
