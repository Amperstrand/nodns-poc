import { decode } from 'nostr-tools/nip19';
import type {
  DiscoveredZone,
  NostrDnsRecord,
  ResolutionMode,
  ResolvedRecord,
  ResolveVerifiedResult,
  ResolverOptions,
  ReverseResult,
} from './types.js';
import { DEFAULT_API_BASE, DEFAULT_DOH_ENDPOINT, DEFAULT_READ_RELAYS, DEFAULT_ZONE } from './types.js';
import { queryAllDnsRecordTypes } from './dns.js';
import { queryRecordsByDomain, queryRecordsByPubkey } from './nostr.js';
import {
  compareTripartite,
  fetchTripartiteRecords,
  toResolvedRecords,
} from './verify.js';
import { discoverZones as discoverZonesImpl } from './zones.js';

export interface Resolver {
  readonly mode: ResolutionMode;
  readonly relays: string[];
  readonly apiBase: string;
  readonly zone: string;
  readonly dohEndpoint: string;

  resolve(name: string, type?: string): Promise<ResolvedRecord[]>;

  resolveVerified(name: string, type?: string): Promise<ResolveVerifiedResult>;

  reverse(npub: string): Promise<ReverseResult>;

  discoverZones(): Promise<DiscoveredZone[]>;

  getNostrRecords(
    params: { pubkey?: string; domain?: string },
  ): Promise<NostrDnsRecord[]>;
}

function normalizeNpubToHex(npub: string): string {
  if (npub.startsWith('npub1')) {
    try {
      const decoded = decode(npub);
      if (decoded.type === 'npub') {
        return decoded.data as string;
      }
    } catch {
      // fall through
    }
  }
  return npub;
}

function extractZoneFromFqdn(fqdn: string, knownZones: string[]): string {
  const lower = fqdn.toLowerCase();
  for (const z of knownZones) {
    if (lower.endsWith(`.${z}`)) return z;
  }
  return '';
}

export function createResolver(options?: ResolverOptions): Resolver {
  const mode: ResolutionMode = options?.mode ?? 'dns';
  const relays = options?.relays ?? DEFAULT_READ_RELAYS;
  const apiBase = options?.apiBase ?? DEFAULT_API_BASE;
  const zone = options?.zone ?? DEFAULT_ZONE;
  const dohEndpoint = options?.dohEndpoint ?? DEFAULT_DOH_ENDPOINT;

  async function resolve(name: string, type?: string): Promise<ResolvedRecord[]> {
    if (mode === 'dns') {
      const types = type ? [type] : undefined;
      const results = await queryAllDnsRecordTypes(
        name,
        types as undefined | Parameters<typeof queryAllDnsRecordTypes>[1],
        dohEndpoint,
      );
      return results.map((r) => ({
        type: r.type,
        name: r.name,
        ttl: r.ttl,
        data: r.data,
        source: 'dns' as const,
      }));
    }

    if (mode === 'nostr') {
      const records = await queryRecordsByDomain(name, zone, relays);
      return records
        .filter((r) => !type || r.type === type)
        .map((r) => ({
          type: r.type,
          name: r.fqdn,
          ttl: r.ttl,
          data: r.value,
          source: 'nostr' as const,
          pubkey: r.pubkey,
          eventId: r.eventId,
        }));
    }

    const result = await resolveVerifiedInternal(name, type);
    return result.records;
  }

  async function resolveVerified(
    name: string,
    type?: string,
  ): Promise<ResolveVerifiedResult> {
    return resolveVerifiedInternal(name, type);
  }

  async function resolveVerifiedInternal(
    name: string,
    type?: string,
  ): Promise<ResolveVerifiedResult> {
    const detectedZone = extractZoneFromFqdn(name, [zone]) || zone;
    const pubkey = name.startsWith('npub1') ? normalizeNpubToHex(name) : undefined;
    const domain = !pubkey ? name : undefined;

    const sources = await fetchTripartiteRecords(
      { pubkey, domain },
      detectedZone,
      relays,
      apiBase,
      dohEndpoint,
    );

    const comparison = compareTripartite(sources);
    let records = toResolvedRecords(sources, comparison);

    if (type) {
      records = records.filter((r) => r.type === type);
    }

    return {
      records,
      verified: comparison.match,
      sources,
      comparison,
    };
  }

  async function reverse(npub: string): Promise<ReverseResult> {
    const hexPubkey = normalizeNpubToHex(npub);
    const records = await queryRecordsByPubkey(hexPubkey, zone, relays);
    const names = [...new Set(records.map((r) => r.fqdn))].sort();
    return { names, records };
  }

  async function discoverZones(): Promise<DiscoveredZone[]> {
    return discoverZonesImpl(relays, dohEndpoint);
  }

  async function getNostrRecords(
    params: { pubkey?: string; domain?: string },
  ): Promise<NostrDnsRecord[]> {
    if (params.pubkey) {
      const hex = params.pubkey.startsWith('npub1')
        ? normalizeNpubToHex(params.pubkey)
        : params.pubkey;
      return queryRecordsByPubkey(hex, zone, relays);
    }
    if (params.domain) {
      return queryRecordsByDomain(params.domain, zone, relays);
    }
    return [];
  }

  return {
    mode,
    relays,
    apiBase,
    zone,
    dohEndpoint,
    resolve,
    resolveVerified,
    reverse,
    discoverZones,
    getNostrRecords,
  };
}
