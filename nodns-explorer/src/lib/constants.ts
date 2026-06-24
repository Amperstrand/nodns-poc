export const RELAYS = [
  "wss://relay.cashu.email",
];

export const RECORD_KIND = 11111;
export const ZONE_HANDLER_KIND = 31990;

export const DNS_TYPES = ["A", "AAAA", "CNAME", "TXT", "MX"] as const;
export type DnsType = (typeof DNS_TYPES)[number];

export const RELAY_LOOKUP_URL = "https://relay.cashu.email";

export const DEFAULT_ZONE = "nodns.shop";

export const BOT_API_BASE = "https://nodns.shop";

export const VALID_RECORD_TYPES = ["A", "AAAA", "CNAME", "TXT", "MX"] as const;
