import { PUBLISH_RELAYS, READ_RELAYS } from "../../../shared/relays";
import { DEFAULT_POW_DIFFICULTY as _POW } from "../../../shared/pow";

export const DEFAULT_RELAYS = PUBLISH_RELAYS;
export const ALL_RELAYS = READ_RELAYS;
export const DEFAULT_ZONE = "nodns.shop";
export const DEFAULT_MINT_URL = "https://testnut.cashu.space";
export const RECORD_KIND = 11111;
export const ZONE_HANDLER_KIND = 31990;
export const DOH_ENDPOINT = "https://dns.google/resolve";
export const DEFAULT_API_BASE = "https://nodns.shop";
export const DEFAULT_POW_DIFFICULTY = _POW;

export const DNS_TYPES = ["A", "AAAA", "CNAME", "TXT", "MX"] as const;
export type DnsType = (typeof DNS_TYPES)[number];

export const QUERY_MAX_WAIT = 6000;
