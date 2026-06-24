import type { NostrEvent } from "nostr-tools/pure";

export interface ExplorerEvent {
  id: string;
  kind: number;
  pubkey: string;
  created_at: number;
  content: string;
  tags: string[][];
  raw: NostrEvent;
}

export interface ZoneStatus {
  zone: string;
  pubkey: string;
  status: "testing" | "preview" | "production" | "unknown";
  testnet: boolean;
  statusReason?: string;
  pricing?: { create: number; update: number; delete: number };
  mint?: string;
  web?: string;
  verified: boolean;
  verificationError?: string;
}

export interface FilterState {
  npub: string;
  recordType: string;
  kindFilter: "all" | "records" | "zones";
  paymentFilter: "all" | "paid" | "free" | "unpaid";
  validityFilter: "all" | "valid" | "invalid";
}

export interface ParsedRecord {
  type: string;
  name: string;
  data: string;
  ttl?: string;
}

export interface ParsedZoneEvent {
  zone: string;
  status: string;
}
