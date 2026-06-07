"use client";

import { useState, useCallback } from "react";
import { queryDoh } from "@/lib/dns";
import { DNS_TYPES, DNS_STATUS_CODES } from "@/lib/constants";

export function TryIt() {
  const [dohFqdn, setDohFqdn] = useState(
    "npub190queyng2pmx0jfw5rkx4fjjl3u0zxz6nlyaja53p2n0ydupr6jsdnqt8q.nodns.shop",
  );
  const [dohType, setDohType] = useState("A");
  const [dohResults, setDohResults] = useState<string>("");
  const [dohLoading, setDohLoading] = useState(false);

  const handleLookup = useCallback(async () => {
    if (!dohFqdn.trim()) {
      setDohResults("Enter a domain name.");
      return;
    }
    setDohLoading(true);
    try {
      const data = await queryDoh(dohFqdn, dohType);
      if (data.Answer && data.Answer.length > 0) {
        const rows = data.Answer.map(
          (a) =>
            `${a.name}  ${DNS_TYPES[a.type] ?? a.type}  ${a.TTL}s  ${a.data}`,
        ).join("\n");
        setDohResults(rows);
      } else if (data.Status === 0) {
        setDohResults("Query succeeded but no records found.");
      } else {
        setDohResults(
          `DNS error: ${DNS_STATUS_CODES[data.Status] ?? "Status " + data.Status}`,
        );
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : "Unknown error";
      setDohResults(`Query failed: ${msg}`);
    }
    setDohLoading(false);
  }, [dohFqdn, dohType]);

  return (
    <section id="try" className="px-6 py-16">
      <div className="mx-auto max-w-[960px]">
        <h2 className="mb-6 text-[1.75rem] font-bold tracking-tight">
          Try It Now
        </h2>
        <p className="mb-4 text-[#bbb]">
          Test resolution against the nodns.shop nameserver:
        </p>
        <pre className="mb-4 overflow-x-auto rounded-lg border border-[#222] bg-[#141414] p-4 text-[0.85rem] leading-relaxed">
          <code>{`# Query an A record (resolves to 185.18.221.10)
dig npub190queyng2pmx0jfw5rkx4fjjl3u0zxz6nlyaja53p2n0ydupr6jsdnqt8q.nodns.shop A

# Query a TXT record (resolves to "cool stuff!")
dig npub190queyng2pmx0jfw5rkx4fjjl3u0zxz6nlyaja53p2n0ydupr6jsdnqt8q.nodns.shop TXT

# Query a subdomain with CNAME
dig www.npub1hw6amg8p24ne08c9gdq8hhpqx0t0pwanpae9z25crn7m9uy7yarse465gr.nodns.shop

# Full trace
dig +trace npub190queyng2pmx0jfw5rkx4fjjl3u0zxz6nlyaja53p2n0ydupr6jsdnqt8q.nodns.shop A`}</code>
        </pre>

        {/* In-Browser DoH Lookup */}
        <div className="mt-6 border-t border-[#222] pt-6">
          <h3 className="mb-3 text-lg font-semibold">
            In-Browser DNS Lookup
          </h3>
          <p className="mb-4 text-sm text-[#666]">
            Resolve DNS records directly from your browser via Cloudflare
            DNS-over-HTTPS:
          </p>
          <div className="mb-4 flex gap-2">
            <input
              type="text"
              value={dohFqdn}
              onChange={(e) => setDohFqdn(e.target.value)}
              placeholder="Enter FQDN"
              className="flex-1 rounded-lg border border-[#222] bg-[#0a0a0a] px-3 py-2.5 text-sm text-[#e0e0e0] outline-none focus:border-[#ff6b35]"
            />
            <select
              value={dohType}
              onChange={(e) => setDohType(e.target.value)}
              className="w-auto rounded-lg border border-[#222] bg-[#0a0a0a] px-3 py-2.5 text-sm text-[#e0e0e0] outline-none"
            >
              <option value="A">A</option>
              <option value="TXT">TXT</option>
              <option value="CNAME">CNAME</option>
              <option value="AAAA">AAAA</option>
              <option value="MX">MX</option>
            </select>
            <button
              onClick={handleLookup}
              disabled={dohLoading}
              className="rounded-lg bg-[#ff6b35] px-3 py-2 text-sm font-semibold text-white hover:opacity-90 disabled:opacity-50"
            >
              Lookup
            </button>
          </div>
          {dohResults && (
            <pre className="overflow-x-auto rounded-lg border border-[#222] bg-[#141414] p-4 text-xs leading-relaxed">
              <code>{dohResults}</code>
            </pre>
          )}
        </div>
      </div>
    </section>
  );
}
