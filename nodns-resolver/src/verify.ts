import type {
  DnsAnswer,
  DnsRecord,
  NostrDnsRecord,
  ResolvedRecord,
  SourceResult,
  TripartiteComparison,
  TripartiteRecords,
} from './types.js';
import { DEFAULT_API_BASE } from './types.js';
import { queryDoh } from './dns.js';
import { queryRecordsByPubkey, queryRecordsByDomain } from './nostr.js';

const DEFAULT_DNS_TYPES = ['A', 'AAAA', 'TXT', 'CNAME', 'MX'] as const;
const WILDCARD_REDIRECT_IPS = new Set(['46.224.104.12']);

export async function fetchApiRecords(
  params: { pubkey?: string; domain?: string },
  apiBase: string = DEFAULT_API_BASE,
): Promise<SourceResult<DnsRecord>> {
  const base: SourceResult<DnsRecord> = {
    source: 'api',
    status: 'loading',
    records: [],
  };

  try {
    const qs = params.pubkey
      ? `pubkey=${encodeURIComponent(params.pubkey)}`
      : params.domain
        ? `domain=${encodeURIComponent(params.domain)}`
        : '';
    if (!qs) return { ...base, status: 'unavailable' };

    const resp = await fetch(`${apiBase}/api/records?${qs}`, {
      signal: AbortSignal.timeout(10_000),
    });
    if (!resp.ok) {
      return { ...base, status: 'error', error: `HTTP ${resp.status}` };
    }
    const data = await resp.json();
    return {
      ...base,
      status: 'ok',
      records: (data.records ?? []) as DnsRecord[],
    };
  } catch {
    return { ...base, status: 'unavailable' };
  }
}

export async function fetchNostrRecords(
  params: { pubkey?: string; domain?: string },
  zone: string,
  relays: string[],
): Promise<SourceResult<NostrDnsRecord>> {
  const base: SourceResult<NostrDnsRecord> = {
    source: 'nostr',
    status: 'loading',
    records: [],
  };

  try {
    const records = params.pubkey
      ? await queryRecordsByPubkey(params.pubkey, zone, relays)
      : params.domain
        ? await queryRecordsByDomain(params.domain, zone, relays)
        : [];
    return { ...base, status: 'ok', records };
  } catch {
    return { ...base, status: 'error', error: 'Relay query failed' };
  }
}

export async function fetchDnsRecords(
  fqdn: string,
  types: readonly string[] = DEFAULT_DNS_TYPES,
  dohEndpoint?: string,
): Promise<SourceResult<DnsAnswer>> {
  const base: SourceResult<DnsAnswer> = {
    source: 'dns',
    status: 'loading',
    records: [],
  };

  try {
    const allAnswers: DnsAnswer[] = [];

    for (const type of types) {
      try {
        const resp = await queryDoh(fqdn, type, dohEndpoint);
        if (resp.Answer) {
          for (const a of resp.Answer) {
            const typeStr = String(a.type);
            const isWildcardA =
              (typeStr === '1' || typeStr === 'A') &&
              WILDCARD_REDIRECT_IPS.has(a.data.replace(/"/g, '').trim());
            if (isWildcardA) continue;
            allAnswers.push({
              name: a.name,
              type: typeStr,
              ttl: a.TTL,
              data: a.data,
            });
          }
        }
      } catch {
        // individual type failures are non-fatal
      }
    }

    return {
      ...base,
      status: allAnswers.length > 0 ? 'ok' : 'unavailable',
      records: allAnswers,
    };
  } catch {
    return { ...base, status: 'error', error: 'DNS query failed' };
  }
}

export async function fetchTripartiteRecords(
  params: { pubkey?: string; domain?: string },
  zone: string,
  relays: string[],
  apiBase: string,
  dohEndpoint?: string,
): Promise<TripartiteRecords> {
  const fqdn = params.domain ?? '';

  const [apiResult, nostrResult, dnsResult] = await Promise.all([
    fetchApiRecords(params, apiBase),
    fetchNostrRecords(params, zone, relays),
    fqdn
      ? fetchDnsRecords(fqdn, DEFAULT_DNS_TYPES, dohEndpoint)
      : Promise.resolve({
          source: 'dns',
          status: 'unavailable' as const,
          records: [],
        }),
  ]);

  return { api: apiResult, nostr: nostrResult, dns: dnsResult };
}

export function compareTripartite(results: TripartiteRecords): TripartiteComparison {
  const toKey = (type: string, data: string) => `${type}:${data}`;

  const apiKeys = new Set(
    results.api.records.map((r) => toKey(r.type, r.rdata)),
  );
  const nostrKeys = new Set(
    results.nostr.records.map((r) => toKey(r.type, r.value)),
  );
  const dnsKeys = new Set(
    results.dns.records.map((r) => toKey(r.type, r.data)),
  );

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
    match:
      onlyInApi.length === 0 &&
      onlyInNostr.length === 0 &&
      onlyInDns.length === 0,
    apiCount: results.api.records.length,
    nostrCount: results.nostr.records.length,
    dnsCount: results.dns.records.length,
    onlyInApi,
    onlyInNostr,
    onlyInDns,
  };
}

export function toResolvedRecords(
  sources: TripartiteRecords,
  comparison: TripartiteComparison,
): ResolvedRecord[] {
  if (comparison.match) {
    const fromDns = sources.dns.records.map((r) => ({
      type: r.type,
      name: r.name,
      ttl: r.ttl,
      data: r.data,
      source: 'dns' as const,
    }));
    if (fromDns.length > 0) return fromDns;

    const fromApi = sources.api.records.map((r) => ({
      type: r.type,
      name: r.fqdn,
      ttl: r.ttl,
      data: r.rdata,
      source: 'dns' as const,
    }));
    return fromApi;
  }

  const agreedKeys = new Set(
    [...sources.api.records.map((r) => `${r.type}:${r.rdata}`)].filter((k) => {
      const inNostr = sources.nostr.records.some(
        (r) => `${r.type}:${r.value}` === k,
      );
      const inDns = sources.dns.records.some(
        (r) => `${r.type}:${r.data}` === k,
      );
      return inNostr || inDns;
    }),
  );

  return sources.api.records
    .filter((r) => agreedKeys.has(`${r.type}:${r.rdata}`))
    .map((r) => ({
      type: r.type,
      name: r.fqdn,
      ttl: r.ttl,
      data: r.rdata,
      source: 'dns' as const,
    }));
}
