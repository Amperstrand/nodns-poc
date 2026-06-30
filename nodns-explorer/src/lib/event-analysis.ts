import {
  parseRecords as sdkParseRecords,
  parseRecord as sdkParseRecord,
  checkValidity as sdkCheckValidity,
  isNpubDerivedName,
  computeFqdn,
} from "@nodns/resolver";
import type { NostrEvent } from "nostr-tools/pure";
import { npubEncode } from "nostr-tools/nip19";
import type { ExplorerEvent } from "@/lib/types";

type AnyEvent = ExplorerEvent | NostrEvent;

export { isNpubDerivedName, computeFqdn };
export type { RecordInfo, SpecVersion, ValidityInfo } from "@nodns/resolver";

export type PaymentStatus = "paid" | "free" | "unpaid";

export interface PaymentInfo {
  status: PaymentStatus;
  paid: boolean;
  amount: number;
  mint: string;
  isTestnut: boolean;
}

function safeNpubEncode(pubkey: string): string {
  try {
    return npubEncode(pubkey);
  } catch {
    return pubkey.slice(0, 16);
  }
}

export function parseRecords(event: AnyEvent) {
  return sdkParseRecords(event as unknown as NostrEvent);
}

export function parseRecord(event: AnyEvent) {
  return sdkParseRecord(event as unknown as NostrEvent);
}

export function checkValidity(event: AnyEvent) {
  return sdkCheckValidity(event as unknown as NostrEvent);
}

export function parsePayment(event: AnyEvent): PaymentInfo {
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

export function parseZoneEvent(event: AnyEvent): ZoneEventInfo | null {
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
