export interface DnsRecord {
  npub: string;
  name: string;
  fqdn: string;
  type: string;
  ttl: number;
  rdata: string;
  created_at: number;
}

export interface ApiRecordsResponse {
  records: DnsRecord[];
  count: number;
}

export interface PendingRecord {
  type: string;
  name: string;
  value: string;
  ttl: number;
  displayName: string;
}

export interface NostrEvent {
  id: string;
  pubkey: string;
  created_at: number;
  kind: number;
  tags: string[][];
  content: string;
  sig: string;
}

export interface EventWithRelay {
  event: NostrEvent;
  relay: string;
}

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

export type FeedbackType = 'success' | 'error' | null;

export interface KeyPair {
  secretKey: Uint8Array;
  publicKey: string;
  npub: string;
  nsec: string;
}

export interface ZonePricing {
  zone: string;
  enabled: boolean;
  create_price: number;
  update_price: number;
  delete_price: number;
  npub_names_free: boolean;
  mint_url: string;
  mint_filter: string;
}
