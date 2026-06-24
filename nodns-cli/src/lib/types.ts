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

export interface ZonePricing {
  createPrice: number;
  updatePrice: number;
  deletePrice: number;
}

export type ZoneCheckOutcome =
  | { result: "verified"; info: ZoneInfo }
  | { result: "testnet"; info: ZoneInfo }
  | { result: "unverified"; info: ZoneInfo }
  | { result: "not-opted-in" };

export interface DnsRecord {
  type: string;
  name: string;
  ttl: number;
  rdata: string;
}

export interface GlobalOpts {
  relay?: string;
  zone?: string;
  sec?: string;
  skipZoneCheck?: boolean;
}

export interface Keypair {
  secretKey: Uint8Array;
  pubkey: string;
  nsec: string;
  npub: string;
}
