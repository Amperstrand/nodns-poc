import { API_BASE, DEFAULT_ZONE, RELAYS } from "./constants";
import { MINT_URL } from "./wallet";
import type { NostrDnsRecord } from "./nostr";
import type { DnsRecord, ZonePricing } from "./types";
import {
  fetchApiRecords as sdkFetchApiRecords,
  fetchNostrRecords as sdkFetchNostrRecords,
  fetchDnsRecords as sdkFetchDnsRecords,
  fetchTripartiteRecords as sdkFetchTripartiteRecords,
} from "@nodns/resolver";

export { compareTripartite } from "@nodns/resolver";

export type SourceStatus = "loading" | "ok" | "error" | "unavailable";

export interface SourceResult<T> {
  source: string;
  status: SourceStatus;
  records: T[];
  error?: string;
  label: string;
  icon: string;
  authority: boolean;
}

export interface TripartiteRecords {
  api: SourceResult<DnsRecord>;
  nostr: SourceResult<NostrDnsRecord>;
  dns: SourceResult<{ name: string; type: string; ttl: number; data: string }>;
}

const UI_META = {
  api: { label: "Operator API", icon: "🗄️", authority: false },
  nostr: { label: "Nostr Relays", icon: "🔐", authority: true },
  dns: { label: "DNS Resolver", icon: "🌐", authority: false },
} as const;

export async function fetchApiRecords(
  params: { pubkey?: string; domain?: string },
): Promise<SourceResult<DnsRecord>> {
  const result = await sdkFetchApiRecords(params, API_BASE);
  return { ...result, ...UI_META.api };
}

export async function fetchNostrRecords(
  params: { pubkey?: string; domain?: string },
  zone: string = DEFAULT_ZONE,
): Promise<SourceResult<NostrDnsRecord>> {
  const result = await sdkFetchNostrRecords(params, zone, RELAYS);
  return { ...result, ...UI_META.nostr };
}

export async function fetchDnsRecords(
  fqdn: string,
  types: string[] = ["A", "AAAA", "TXT", "CNAME", "MX"],
): Promise<SourceResult<{ name: string; type: string; ttl: number; data: string }>> {
  const result = await sdkFetchDnsRecords(fqdn, types);
  return { ...result, ...UI_META.dns };
}

export async function fetchTripartiteRecords(
  params: { pubkey?: string; domain?: string },
  zone: string = DEFAULT_ZONE,
): Promise<TripartiteRecords> {
  const result = await sdkFetchTripartiteRecords(params, zone, RELAYS, API_BASE);
  return {
    api: { ...result.api, ...UI_META.api },
    nostr: { ...result.nostr, ...UI_META.nostr },
    dns: { ...result.dns, ...UI_META.dns },
  };
}

const DEFAULT_PRICING: ZonePricing = {
  zone: DEFAULT_ZONE,
  enabled: true,
  create_price: 2,
  update_price: 0,
  delete_price: 0,
  npub_names_free: true,
  mint_url: MINT_URL,
  mint_filter: "testnut",
};

export async function fetchPricing(
  zone: string = DEFAULT_ZONE,
): Promise<ZonePricing> {
  try {
    const resp = await fetch(`${API_BASE}/api/zones/${encodeURIComponent(zone)}/pricing`, {
      signal: AbortSignal.timeout(5_000),
    });
    if (resp.ok) return await resp.json();
  } catch {
    // API unavailable — use defaults
  }
  return DEFAULT_PRICING;
}
