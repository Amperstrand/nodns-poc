import * as dns from "node:dns/promises";
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
  const results: DnsQueryResult[] = [];

  const [aAddrs, aaaaAddrs, cnames, txtRecords, mxRecords] = await Promise.all([
    safeResolve(() => dns.resolve4(fqdn), [] as string[]),
    safeResolve(() => dns.resolve6(fqdn), [] as string[]),
    safeResolve(() => dns.resolveCname(fqdn), [] as string[]),
    safeResolve(() => dns.resolveTxt(fqdn), [] as string[][]),
    safeResolve(
      () => dns.resolveMx(fqdn),
      [] as { priority: number; exchange: string }[],
    ),
  ]);

  for (const addr of aAddrs) {
    results.push({ name: fqdn, type: "A", ttl: 0, rdata: addr });
  }
  for (const addr of aaaaAddrs) {
    results.push({ name: fqdn, type: "AAAA", ttl: 0, rdata: addr });
  }
  for (const cname of cnames) {
    results.push({ name: fqdn, type: "CNAME", ttl: 0, rdata: cname });
  }
  for (const mx of mxRecords) {
    results.push({
      name: fqdn,
      type: "MX",
      ttl: 0,
      rdata: `${mx.priority} ${mx.exchange}`,
    });
  }
  for (const txtParts of txtRecords) {
    results.push({ name: fqdn, type: "TXT", ttl: 0, rdata: txtParts.join("") });
  }

  return results;
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
