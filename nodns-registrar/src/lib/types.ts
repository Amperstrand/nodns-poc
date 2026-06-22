export interface DnsRecord {
  id?: string;
  record_type: string;
  name: string;
  rdata: string;
  ttl: number;
  zone?: string;
  npub?: string;
  event_id?: string;
  created_at?: number;
  deleted?: boolean;
}

export interface DomainInfo {
  fqdn: string;
  name: string;
  zone: string;
  recordCount: number;
  records: DnsRecord[];
  lastSeen: number;
  status: "active" | "expired" | "grace";
}

export interface PricingInfo {
  create_price: number;
  update_price: number;
  delete_price: number;
  npub_names_free: boolean;
  mint_url: string;
}

export interface AvailabilityResult {
  name: string;
  zone: string;
  fqdn: string;
  available: boolean;
  price: number;
  reason?: string;
}

export interface SavedAccount {
  pubkey: string;
  nsec: string;
  npub: string;
  addedAt: number;
}

export interface Session {
  pubkey: string;
  secretKeyHex: string | null;
  authMethod: "nsec" | "extension" | "ephemeral";
}

export interface WalletState {
  balance: number;
  mintUrl: string;
  ready: boolean;
}
