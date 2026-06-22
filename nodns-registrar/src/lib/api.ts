import { API_BASE, DEFAULT_ZONE } from "./constants";
import type { AvailabilityResult, DnsRecord, PricingInfo } from "./types";

async function safeFetch(url: string, init?: RequestInit): Promise<Response> {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 30_000);
  try {
    const res = await fetch(url, {
      ...init,
      signal: controller.signal,
    });
    return res;
  } catch (e) {
    if (e instanceof DOMException && e.name === "AbortError") {
      throw new Error("Request timed out. Please try again.");
    }
    throw new Error("Unable to connect. Please check your network.");
  } finally {
    clearTimeout(timeout);
  }
}

function sanitizeError(e: unknown): string {
  const raw = e instanceof Error ? e.message : String(e);
  if (raw.includes("<") || raw.includes("stack")) {
    return "Server error. Please try again.";
  }
  return raw.substring(0, 200);
}

export async function checkAvailability(
  name: string,
  zone: string,
): Promise<AvailabilityResult> {
  const res = await safeFetch(
    `${API_BASE}/api/check?name=${encodeURIComponent(name)}&zone=${encodeURIComponent(zone)}`,
  );
  if (!res.ok) {
    const body = await res.json().catch(() => ({}));
    throw new Error(body.error || `HTTP ${res.status}`);
  }

  const data = await res.json();

  const apiRegistered = data?.api?.registered ?? false;
  const dnsRegistered = data?.dns?.registered ?? false;
  const available = !apiRegistered && !dnsRegistered;

  let price = 0;
  try {
    const pricing = await fetchPricing(zone);
    price = pricing.create_price;
  } catch {
  }

  return {
    name,
    zone,
    fqdn: `${name}.${zone}`,
    available,
    price,
    reason: available ? undefined : "Name already registered",
  };
}

export async function fetchRecords(npub: string): Promise<DnsRecord[]> {
  const res = await safeFetch(
    `${API_BASE}/api/records?pubkey=${encodeURIComponent(npub)}`,
  );
  if (!res.ok) throw new Error("Failed to fetch records");
  const data = await res.json();
  const records = Array.isArray(data) ? data : (data?.records ?? []);
  return records.map((r: Record<string, unknown>) => {
    const fqdn = ((r.fqdn as string) || "").replace(/\.$/, "");
    const parts = fqdn.split(".");
    const derivedZone = parts.length > 1 ? parts.slice(1).join(".") : DEFAULT_ZONE;
    return {
      record_type: (r.type ?? r.record_type ?? "") as string,
      name: ((r.name as string) || "").replace(/^@$/, ""),
      rdata: (r.rdata ?? "") as string,
      ttl: (r.ttl ?? 3600) as number,
      zone: (r.zone as string) || derivedZone,
      npub: r.npub as string | undefined,
      event_id: r.event_id as string | undefined,
      created_at: r.created_at as number | undefined,
      deleted: false,
    };
  });
}

export async function fetchPricing(zone: string): Promise<PricingInfo> {
  const res = await safeFetch(
    `${API_BASE}/api/zones/${encodeURIComponent(zone)}/pricing`,
  );
  if (!res.ok) throw new Error("Failed to fetch pricing");
  return res.json();
}

export async function healthCheck(): Promise<boolean> {
  try {
    const res = await safeFetch(`${API_BASE}/health`);
    return res.ok;
  } catch {
    return false;
  }
}

export { safeFetch, sanitizeError };
