import type { ExplorerEvent } from "@/lib/types";
import type { NostrEvent } from "nostr-tools/pure";
import { npubEncode } from "nostr-tools/nip19";
import { DEFAULT_ZONE, VALID_RECORD_TYPES } from "@/lib/constants";

export interface RecordInfo {
  type: string;
  name: string;
  ttl: number;
  rdata: string;
  fqdn: string;
  isNpubDerived: boolean;
}

export type PaymentStatus = "paid" | "free" | "unpaid";

export interface PaymentInfo {
  status: PaymentStatus;
  paid: boolean;
  amount: number;
  mint: string;
  isTestnut: boolean;
}

export type SpecVersion = "v1" | "v1.1" | "v2";

export interface ValidityInfo {
  valid: boolean;
  reason?: string;
  specVersion: SpecVersion;
}

function safeNpubEncode(pubkey: string): string {
  try {
    return npubEncode(pubkey);
  } catch {
    return pubkey.slice(0, 16);
  }
}

export function isNpubDerivedName(name: string): boolean {
  return name === "" || name === "@";
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

export function parseRecords(event: ExplorerEvent | NostrEvent): RecordInfo[] {
  const tags = event.tags;
  const pubkey = event.pubkey;
  const records: RecordInfo[] = [];
  for (const tag of tags) {
    if (tag[0] !== "record") continue;
    if (tag.length < 4) continue;
    const type = (tag[1] ?? "").toUpperCase();
    const name = tag[2] ?? "";
    const rdata = tag[3] ?? "";
    const ttl = parseTtl(tag);
    const isNpubDerived = isNpubDerivedName(name);
    const fqdn = computeFqdn(name, pubkey);
    records.push({ type, name, ttl, rdata, fqdn, isNpubDerived });
  }
  return records;
}

export function parseRecord(event: ExplorerEvent | NostrEvent): RecordInfo | null {
  const records = parseRecords(event);
  return records.length > 0 ? records[0] : null;
}

export function parsePayment(event: ExplorerEvent | NostrEvent): PaymentInfo {
  const tag = event.tags.find((t) => t[0] === "cashu");
  if (!tag || tag.length < 4) {
    const firstRecord = parseRecord(event);
    const isNpub = firstRecord ? firstRecord.isNpubDerived : true;
    return {
      status: isNpub ? "free" : "unpaid",
      paid: false,
      amount: 0,
      mint: "",
      isTestnut: false,
    };
  }
  const mint = tag[2] ?? "";
  const amount = parseInt(tag[3] ?? "0", 10);
  return {
    status: "paid",
    paid: true,
    amount: Number.isNaN(amount) ? 0 : amount,
    mint,
    isTestnut: mint.includes("testnut"),
  };
}

function detectSpecVersion(tags: string[][]): SpecVersion {
  const hasAlt = tags.some((t) => t[0] === "alt");
  const cashuTag = tags.find((t) => t[0] === "cashu");
  const hasP2PK = cashuTag?.[1]?.includes("P2PK") ?? false;
  if (hasP2PK) return "v2";
  if (hasAlt) return "v1.1";
  return "v1";
}

export function checkValidity(event: ExplorerEvent | NostrEvent): ValidityInfo {
  const tags = event.tags;
  const specVersion = detectSpecVersion(tags);
  const validTypes = VALID_RECORD_TYPES as readonly string[];

  const recordTags = tags.filter((t) => t[0] === "record");

  if (recordTags.length === 0) {
    return { valid: false, reason: "no record tags", specVersion };
  }

  for (const tag of recordTags) {
    if (tag.length < 4) {
      return { valid: false, reason: `malformed (${tag.length} fields)`, specVersion };
    }
    const type = (tag[1] ?? "").toUpperCase();
    if (!validTypes.includes(type)) {
      return { valid: false, reason: `unknown type: ${type}`, specVersion };
    }
  }

  return { valid: true, specVersion };
}

export interface ZoneEventInfo {
  zone: string;
  status?: string;
  testnet: boolean;
  pricing?: { create: number; update: number; del: number };
  mint?: string;
  web?: string;
  operatorNpub: string;
  dnskeyHash?: string;
}

export function parseZoneEvent(event: ExplorerEvent | NostrEvent): ZoneEventInfo | null {
  const tags = event.tags;
  const zoneTag = tags.find((t) => t[0] === "zone" && t[1]);
  if (!zoneTag) return null;

  const statusTag = tags.find((t) => t[0] === "status" && t[1]);
  const testnet = tags.some((t) => t[0] === "testnet");
  const mintTag = tags.find((t) => t[0] === "mint" && t[1]);
  const webTag = tags.find((t) => t[0] === "web" && t[1]);
  const dnskeyTag = tags.find((t) => t[0] === "dnskey_hash" && t[1]);

  const pricingTag = tags.find((t) => t[0] === "pricing");
  let pricing: ZoneEventInfo["pricing"] | undefined;
  if (pricingTag) {
    let create = 0;
    let update = 0;
    let del = 0;
    let found = false;
    for (const entry of pricingTag.slice(1)) {
      const eq = entry.indexOf("=");
      if (eq === -1) continue;
      const key = entry.slice(0, eq).trim();
      const val = parseInt(entry.slice(eq + 1).trim(), 10);
      if (Number.isNaN(val)) continue;
      found = true;
      if (key === "create") create = val;
      else if (key === "update") update = val;
      else if (key === "delete") del = val;
    }
    if (found) pricing = { create, update, del };
  }

  return {
    zone: zoneTag[1],
    status: statusTag?.[1],
    testnet,
    pricing,
    mint: mintTag?.[1],
    web: webTag?.[1],
    operatorNpub: safeNpubEncode(event.pubkey),
    dnskeyHash: dnskeyTag?.[1],
  };
}
