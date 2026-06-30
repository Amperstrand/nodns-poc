import type { DnsRecordType, DohAnswer, DohResponse } from './types.js';
import { DEFAULT_DOH_ENDPOINT, DNS_TYPE_MAP } from './types.js';

const DOH_TIMEOUT_MS = 10_000;

export async function queryDoh(
  fqdn: string,
  type: string,
  dohEndpoint: string = DEFAULT_DOH_ENDPOINT,
): Promise<DohResponse> {
  const url = `${dohEndpoint}?name=${encodeURIComponent(fqdn)}&type=${type}`;

  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), DOH_TIMEOUT_MS);

  let response: Response;
  try {
    response = await fetch(url, {
      headers: { Accept: 'application/dns-json' },
      signal: controller.signal,
    });
  } catch (err: unknown) {
    if (err instanceof DOMException && err.name === 'AbortError') {
      throw new Error('DNS query timed out. Please try again.');
    }
    throw new Error('DNS query failed. Please check your network.');
  } finally {
    clearTimeout(timer);
  }

  if (!response.ok) {
    throw new Error(`DNS query failed (HTTP ${response.status}).`);
  }

  return (await response.json()) as DohResponse;
}

function stripTxtQuotes(data: string): string {
  return data.replace(/^"/, '').replace(/"$/, '').replace(/"\s*"/g, '');
}

function dnsTypeNumberToString(num: number): string {
  return DNS_TYPE_MAP[num] ?? String(num);
}

export async function queryDnsRecords(
  name: string,
  type: DnsRecordType,
  dohEndpoint: string = DEFAULT_DOH_ENDPOINT,
): Promise<string[]> {
  const resp = await queryDoh(name, type, dohEndpoint);
  if (!resp.Answer) return [];

  return resp.Answer.filter(
    (a: DohAnswer) => dnsTypeNumberToString(a.type) === type,
  ).map((a: DohAnswer) => (type === 'TXT' ? stripTxtQuotes(a.data) : a.data));
}

export async function queryAllDnsRecordTypes(
  fqdn: string,
  types: readonly DnsRecordType[] = ['A', 'AAAA', 'TXT', 'CNAME', 'MX'],
  dohEndpoint: string = DEFAULT_DOH_ENDPOINT,
): Promise<{ name: string; type: string; ttl: number; data: string }[]> {
  const results: { name: string; type: string; ttl: number; data: string }[] = [];

  for (const type of types) {
    try {
      const resp = await queryDoh(fqdn, type, dohEndpoint);
      if (!resp.Answer) continue;
      for (const a of resp.Answer) {
        const typeStr = dnsTypeNumberToString(a.type);
        if (typeStr !== type) continue;
        results.push({
          name: a.name,
          type: typeStr,
          ttl: a.TTL,
          data: type === 'TXT' ? stripTxtQuotes(a.data) : a.data,
        });
      }
    } catch {
      // individual type failures are non-fatal
    }
  }

  return results;
}
