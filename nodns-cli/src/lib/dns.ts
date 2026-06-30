import * as dns from "node:dns/promises";
import { queryAllDnsRecordTypes } from "@nodns/resolver";
import type { ZoneRecord } from "./zone-file.js";
import { compareRecords } from "./zone-file.js";

export interface DnsQueryResult {
  name: string;
  type: string;
  ttl: number;
  rdata: string;
}

async function safeResolve<T>(
  fn: () => Promise<T>,
  fallback: T,
): Promise<T> {
  try {
    return await fn();
  } catch {
    return fallback;
  }
}

export async function queryAllRecords(fqdn: string): Promise<DnsQueryResult[]> {
  const records = await queryAllDnsRecordTypes(fqdn);
  return records.map((r) => ({
    name: r.name,
    type: r.type,
    ttl: r.ttl,
    rdata: r.data,
  }));
}

export interface ZoneApexResult {
  soa: {
    nsname: string;
    hostmaster: string;
    serial: number;
    refresh: number;
    retry: number;
    expire: number;
    minttl: number;
  } | null;
  ns: string[];
}

export async function queryZoneApex(zone: string): Promise<ZoneApexResult> {
  const [soa, ns] = await Promise.all([
    safeResolve(() => dns.resolveSoa(zone), null),
    safeResolve(() => dns.resolveNs(zone), [] as string[]),
  ]);
  return { soa, ns };
}

export function dnsResultsToZoneRecords(
  results: DnsQueryResult[],
  zone: string,
): ZoneRecord[] {
  const zoneSuffix = `.${zone}`;
  const records: ZoneRecord[] = [];

  for (const r of results) {
    let name = r.name;
    if (name.endsWith(".")) name = name.slice(0, -1);
    if (name.endsWith(zoneSuffix)) {
      name = name.slice(0, -zoneSuffix.length);
    }
    records.push({
      name,
      type: r.type.toUpperCase(),
      ttl: r.ttl || 3600,
      rdata: r.rdata,
      npub: "dns",
      event_id: "dns",
      created_at: 0,
    });
  }

  return records.sort(compareRecords);
}

export async function queryZoneRecords(
  zone: string,
  knownFqdns: string[],
): Promise<ZoneRecord[]> {
  const uniqueFqdns = [...new Set(knownFqdns)];
  const allResults: DnsQueryResult[] = [];

  const results = await Promise.allSettled(
    uniqueFqdns.map((fqdn) => queryAllRecords(fqdn)),
  );

  for (const result of results) {
    if (result.status === "fulfilled") {
      allResults.push(...result.value);
    }
  }

  return dnsResultsToZoneRecords(allResults, zone);
}
