import type { ZoneRecord } from "@/lib/zone-file";

const DOH_ENDPOINT = "https://dns.google/resolve";
const DOH_TIMEOUT_MS = 10_000;
const DNS_QUERY_TYPES = ["A", "AAAA", "TXT", "CNAME", "MX"] as const;
type DnsQueryType = (typeof DNS_QUERY_TYPES)[number];

interface DohAnswer {
  name: string;
  type: number;
  TTL: number;
  data: string;
}

interface DohResponse {
  Status: number;
  Answer?: DohAnswer[];
}

const TYPE_NUMBER_TO_NAME: Record<number, DnsQueryType> = {
  1: "A",
  5: "CNAME",
  16: "TXT",
  28: "AAAA",
  15: "MX",
};

function stripTxtQuotes(data: string): string {
  return data.replace(/^"/, "").replace(/"$/, "").replace(/"\s*"/g, "");
}

export async function queryDnsRecords(
  name: string,
  type: "A" | "AAAA" | "TXT" | "CNAME" | "MX",
): Promise<string[]> {
  const url = `${DOH_ENDPOINT}?name=${encodeURIComponent(name)}&type=${type}`;
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), DOH_TIMEOUT_MS);
  try {
    const res = await fetch(url, {
      headers: { accept: "application/dns-json" },
      signal: controller.signal,
    });
    if (!res.ok) return [];
    const data: unknown = await res.json();
    if (typeof data !== "object" || data === null || !("Answer" in data)) return [];
    const response = data as DohResponse;
    if (!response.Answer) return [];
    return response.Answer.filter(
      (a) => TYPE_NUMBER_TO_NAME[a.type] === type,
    ).map((a) => (type === "TXT" ? stripTxtQuotes(a.data) : a.data));
  } catch {
    return [];
  } finally {
    clearTimeout(timeout);
  }
}

export async function queryZoneRecords(fqdn: string): Promise<ZoneRecord[]> {
  const results = await Promise.all(
    DNS_QUERY_TYPES.map(async (type) => {
      const rdatas = await queryDnsRecords(fqdn, type);
      return rdatas.map<ZoneRecord>((rdata) => ({
        name: fqdn,
        type,
        ttl: 3600,
        rdata,
        npub: "",
        event_id: "",
        created_at: 0,
      }));
    }),
  );
  return results.flat();
}
