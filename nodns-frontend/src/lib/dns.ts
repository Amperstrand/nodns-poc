import type { DohResponse } from "./types";
import { DNS_TYPES } from "./constants";

const DOH_TIMEOUT_MS = 30_000;

export async function queryDoh(fqdn: string, type: string): Promise<DohResponse> {
  const url = `https://cloudflare-dns.com/dns-query?name=${encodeURIComponent(fqdn)}&type=${type}`;

  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), DOH_TIMEOUT_MS);

  let response: Response;
  try {
    response = await fetch(url, {
      headers: { Accept: "application/dns-json" },
      signal: controller.signal,
    });
  } catch (err: unknown) {
    if (err instanceof DOMException && err.name === "AbortError") {
      throw new Error("DNS query timed out. Please try again.");
    }
    throw new Error("DNS query failed. Please check your network.");
  } finally {
    clearTimeout(timer);
  }

  if (!response.ok) {
    throw new Error(`DNS query failed (HTTP ${response.status}). Please try again.`);
  }

  const data: DohResponse = await response.json();
  return data;
}

export function dnsTypeNumberToString(num: number): string {
  return DNS_TYPES[num] ?? String(num);
}
