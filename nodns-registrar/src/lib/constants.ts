export const API_BASE = import.meta.env.VITE_API_BASE || "";

import { PUBLISH_RELAYS as _PUBLISH, READ_RELAYS as _READ } from "../../../shared/relays";
import { DEFAULT_POW_DIFFICULTY } from "../../../shared/pow";

export const RELAYS = _READ;
export const PUBLISH_RELAYS = _PUBLISH;
export { DEFAULT_POW_DIFFICULTY };

export const DEFAULT_ZONE = "nodns.shop";

export const DEFAULT_MINT_URL = "https://testnut.cashu.space";

export const DNS_TYPES = ["A", "AAAA", "CNAME", "TXT", "MX"] as const;
export type DnsType = (typeof DNS_TYPES)[number];

export const DNS_STATUS = {
  ACTIVE: "active",
  EXPIRED: "expired",
  GRACE: "grace",
} as const;
export type DnsStatus = (typeof DNS_STATUS)[keyof typeof DNS_STATUS];
