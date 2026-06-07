import type { DohResponse } from './types';
import { DNS_TYPES } from './constants';

export async function queryDoh(fqdn: string, type: string): Promise<DohResponse> {
  const url = `https://cloudflare-dns.com/dns-query?name=${encodeURIComponent(fqdn)}&type=${type}`;
  const resp = await fetch(url, {
    headers: { Accept: 'application/dns-json' },
  });
  const data: DohResponse = await resp.json();
  return data;
}

export function dnsTypeNumberToString(num: number): string {
  return DNS_TYPES[num] ?? String(num);
}
