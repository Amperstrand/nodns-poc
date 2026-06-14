import { API_BASE, DEFAULT_ZONE } from "./constants";
import { MINT_URL } from "./wallet";
import { queryDoh } from "./dns";
import {
  queryRecordsByPubkey,
  queryRecordsByDomain,
  type NostrDnsRecord,
} from "./nostr";
import type { DnsRecord, ZonePricing } from "./types";

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

const WILDCARD_REDIRECT_IPS = new Set(["46.224.104.12"]);

export async function fetchApiRecords(
  params: { pubkey?: string; domain?: string },
): Promise<SourceResult<DnsRecord>> {
  const base: SourceResult<DnsRecord> = {
    source: "api",
    status: "loading",
    records: [],
    label: "Operator API",
    icon: "🗄️",
    authority: false,
  };

  try {
    const qs = params.pubkey
      ? `pubkey=${encodeURIComponent(params.pubkey)}`
      : params.domain
        ? `domain=${encodeURIComponent(params.domain)}`
        : "";
    if (!qs) return { ...base, status: "unavailable" };

    const resp = await fetch(`${API_BASE}/api/records?${qs}`, {
      signal: AbortSignal.timeout(10_000),
    });
    if (!resp.ok) {
      return { ...base, status: "error", error: `HTTP ${resp.status}` };
    }
    const data = await resp.json();
    return {
      ...base,
      status: "ok",
      records: data.records ?? [],
    };
  } catch {
    return { ...base, status: "unavailable" };
  }
}

export async function fetchNostrRecords(
  params: { pubkey?: string; domain?: string },
  zone: string = DEFAULT_ZONE,
): Promise<SourceResult<NostrDnsRecord>> {
  const base: SourceResult<NostrDnsRecord> = {
    source: "nostr",
    status: "loading",
    records: [],
    label: "Nostr Relays",
    icon: "🔐",
    authority: true,
  };

  try {
    const records = params.pubkey
      ? await queryRecordsByPubkey(params.pubkey, zone)
      : params.domain
        ? await queryRecordsByDomain(params.domain, zone)
        : [];
    return { ...base, status: "ok", records };
  } catch {
    return { ...base, status: "error", error: "Relay query failed" };
  }
}

export async function fetchDnsRecords(
  fqdn: string,
  types: string[] = ["A", "AAAA", "TXT", "CNAME", "MX"],
): Promise<SourceResult<{ name: string; type: string; ttl: number; data: string }>> {
  const base: SourceResult<{ name: string; type: string; ttl: number; data: string }> = {
    source: "dns",
    status: "loading",
    records: [],
    label: "DNS Resolver",
    icon: "🌐",
    authority: false,
  };

  try {
    const allAnswers: { name: string; type: string; ttl: number; data: string }[] = [];

    for (const type of types) {
      try {
        const resp = await queryDoh(fqdn, type);
        if (resp.Answer) {
          for (const a of resp.Answer) {
            const typeNum = String(a.type);
            const isWildcardA =
              (typeNum === "1" || typeNum === "A") &&
              WILDCARD_REDIRECT_IPS.has(a.data.replace(/"/g, "").trim());
            if (isWildcardA) continue;
            allAnswers.push({
              name: a.name,
              type: typeNum,
              ttl: a.TTL,
              data: a.data,
            });
          }
        }
      } catch {
        // Individual type failures are non-fatal
      }
    }

    return {
      ...base,
      status: allAnswers.length > 0 ? "ok" : "unavailable",
      records: allAnswers,
    };
  } catch {
    return { ...base, status: "error", error: "DNS query failed" };
  }
}

export async function fetchTripartiteRecords(
  params: { pubkey?: string; domain?: string },
  zone: string = DEFAULT_ZONE,
): Promise<TripartiteRecords> {
  const fqdn = params.domain ?? "";

  const [apiResult, nostrResult, dnsResult] = await Promise.all([
    fetchApiRecords(params),
    fetchNostrRecords(params, zone),
    fqdn ? fetchDnsRecords(fqdn) : Promise.resolve({
      source: "dns",
      status: "unavailable" as SourceStatus,
      records: [],
      label: "DNS Resolver",
      icon: "🌐",
      authority: false,
    }),
  ]);

  return { api: apiResult, nostr: nostrResult, dns: dnsResult };
}

export function compareTripartite(results: TripartiteRecords): {
  match: boolean;
  apiCount: number;
  nostrCount: number;
  dnsCount: number;
  onlyInApi: string[];
  onlyInNostr: string[];
  onlyInDns: string[];
} {
  const toKey = (type: string, data: string) => `${type}:${data}`;

  const apiKeys = new Set(results.api.records.map((r) => toKey(r.type, r.rdata)));
  const nostrKeys = new Set(results.nostr.records.map((r) => toKey(r.type, r.value)));
  const dnsKeys = new Set(results.dns.records.map((r) => toKey(r.type, r.data)));

  const allKeys = new Set([...apiKeys, ...nostrKeys, ...dnsKeys]);

  const onlyInApi: string[] = [];
  const onlyInNostr: string[] = [];
  const onlyInDns: string[] = [];

  for (const key of allKeys) {
    const inApi = apiKeys.has(key);
    const inNostr = nostrKeys.has(key);
    const inDns = dnsKeys.has(key);
    if (inApi && !inNostr && !inDns) onlyInApi.push(key);
    if (inNostr && !inApi && !inDns) onlyInNostr.push(key);
    if (inDns && !inApi && !inNostr) onlyInDns.push(key);
  }

  return {
    match: onlyInApi.length === 0 && onlyInNostr.length === 0 && onlyInDns.length === 0,
    apiCount: results.api.records.length,
    nostrCount: results.nostr.records.length,
    dnsCount: results.dns.records.length,
    onlyInApi,
    onlyInNostr,
    onlyInDns,
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
