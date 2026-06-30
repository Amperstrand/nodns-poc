import { queryDnsRecords } from "@nodns/resolver";
import type { ZoneRecord } from "@/lib/zone-file";

export { queryDnsRecords };

const DNS_QUERY_TYPES = ["A", "AAAA", "TXT", "CNAME", "MX"] as const;

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
