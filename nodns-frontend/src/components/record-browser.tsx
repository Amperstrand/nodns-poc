"use client";

import { useState, useEffect, useCallback, useRef } from "react";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { RELAYS, DNS_TYPES, DNS_STATUS_CODES } from "@/lib/constants";
import { queryDoh } from "@/lib/dns";
import type { DnsRecord, ApiRecordsResponse, EventWithRelay, NostrEvent } from "@/lib/types";
import { subscribeToDnsEvents } from "@/lib/nostr";

const API_BASE = process.env.NEXT_PUBLIC_API_BASE ?? "";

export function RecordBrowser() {
  const [activeTab, setActiveTab] = useState("api");
  const [records, setRecords] = useState<DnsRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [stats, setStats] = useState({ total: 0, domains: 0, types: 0 });
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set());
  const [nostrEvents, setNostrEvents] = useState<EventWithRelay[]>([]);
  const [expandedEvents, setExpandedEvents] = useState<Set<string>>(new Set());
  const fetchRecordsRef = useRef<(() => void) | null>(null);

  
  const [dohFqdn, setDohFqdn] = useState(
    "npub190queyng2pmx0jfw5rkx4fjjl3u0zxz6nlyaja53p2n0ydupr6jsdnqt8q.nodns.shop",
  );
  const [dohType, setDohType] = useState("A");
  const [dohResults, setDohResults] = useState<string>("");
  const [dohLoading, setDohLoading] = useState(false);

  const fetchRecords = useCallback(async () => {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), 30_000);
    try {
      const resp = await fetch(`${API_BASE}/api/records`, { signal: controller.signal });
      if (!resp.ok) {
        setRecords([]);
        return;
      }
      const data: ApiRecordsResponse = await resp.json();
      const recs = data.records || [];
      setRecords(recs);

      const npubs = [...new Set(recs.map((r) => r.npub))];
      const types = [...new Set(recs.map((r) => r.type))];
      setStats({
        total: recs.length,
        domains: npubs.length,
        types: types.length,
      });

      setExpandedGroups((prev) => {
        const next = new Set(prev);
        for (const npub of npubs) {
          if (!next.has(npub)) next.add(npub);
        }
        return next;
      });
    } catch {
      setRecords([]);
    } finally {
      setLoading(false);
      clearTimeout(timer);
    }
  }, []);

  useEffect(() => {
    fetchRecordsRef.current = fetchRecords;
  }, [fetchRecords]);

  useEffect(() => {
    fetchRecordsRef.current?.();
    const interval = setInterval(() => fetchRecordsRef.current?.(), 30000);
    return () => clearInterval(interval);
  }, []);


  useEffect(() => {
    const cleanup = subscribeToDnsEvents((event: NostrEvent, relay: string) => {
      setNostrEvents((prev) => {
        if (prev.some((e) => e.event.id === event.id)) return prev;
        const updated = [{ event, relay }, ...prev].slice(0, 50);
        return updated;
      });
    });
    return cleanup;
  }, []);

  const toggleGroup = (npub: string) => {
    setExpandedGroups((prev) => {
      const next = new Set(prev);
      if (next.has(npub)) next.delete(npub);
      else next.add(npub);
      return next;
    });
  };

  const handleDnsQuery = useCallback(async () => {
    if (!dohFqdn.trim()) {
      setDohResults("Enter a domain name.");
      return;
    }
    setDohLoading(true);
    try {
      const data = await queryDoh(dohFqdn, dohType);
      if (data.Answer && data.Answer.length > 0) {
        setDohResults(
          data.Answer.map(
            (a) =>
              `${a.name} ${DNS_TYPES[a.type] ?? a.type} ${a.TTL}s ${a.data}`,
          ).join("\n"),
        );
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


  const grouped: Record<string, DnsRecord[]> = {};
  for (const r of records) {
    if (!grouped[r.npub]) grouped[r.npub] = [];
    grouped[r.npub].push(r);
  }

  const trustBadge =
    activeTab === "api"
      ? { icon: "🗄️", text: "Source: nodns-bot backend database — trusted operator", authority: false }
      : activeTab === "dns"
        ? { icon: "🌐", text: "Source: DNS resolver — derived from authoritative nameserver", authority: false }
        : { icon: "🔐", text: "Source: Signed Nostr event — cryptographic authority", authority: true };

  return (
    <section id="records" className="px-6 py-16">
      <div className="mx-auto max-w-[960px]">
        <h2 className="mb-2 text-[1.75rem] font-bold tracking-tight">
          DNS Record Browser{" "}
          <span className="relative ml-2.5 inline-block rounded-full bg-[rgba(46,204,113,0.15)] px-2.5 py-0.5 text-[0.65rem] font-bold uppercase tracking-wider text-[#2ecc71]">
            <span className="mr-1.5 inline-block h-1.5 w-1.5 rounded-full bg-[#2ecc71] align-middle animate-live-pulse" />
            Live via Nostr
          </span>
        </h2>
        <p className="mb-4 text-sm text-[#666]">
          The same records, verified from three independent sources. The signed
          Nostr event is the authority.
        </p>

        <Tabs value={activeTab} onValueChange={setActiveTab}>
          <TabsList className="mb-0 h-auto gap-0 rounded-none border-b-2 border-[#222] bg-transparent p-0">
            <TabsTrigger
              value="api"
              className="rounded-none border-b-2 border-transparent bg-transparent px-5 py-2.5 text-sm font-semibold text-[#666] data-[state=active]:border-[#ff6b35] data-[state=active]:text-[#ff6b35] data-[state=active]:shadow-none"
            >
              API Database
            </TabsTrigger>
            <TabsTrigger
              value="dns"
              className="rounded-none border-b-2 border-transparent bg-transparent px-5 py-2.5 text-sm font-semibold text-[#666] data-[state=active]:border-[#ff6b35] data-[state=active]:text-[#ff6b35] data-[state=active]:shadow-none"
            >
              DNS Resolver
            </TabsTrigger>
            <TabsTrigger
              value="nostr"
              className="rounded-none border-b-2 border-transparent bg-transparent px-5 py-2.5 text-sm font-semibold text-[#666] data-[state=active]:border-[#ff6b35] data-[state=active]:text-[#ff6b35] data-[state=active]:shadow-none"
            >
              Nostr Events
            </TabsTrigger>
          </TabsList>

          {/* Trust badge */}
          <div
            className={`flex items-center gap-2 rounded-lg border px-3.5 py-2 text-sm ${trustBadge.authority ? "border-[#2ecc71] bg-[rgba(46,204,113,0.05)]" : "border-[#222] bg-[#141414]"}`}
          >
            <span>{trustBadge.icon}</span> {trustBadge.text}
          </div>

          {/* API Database Tab */}
          <TabsContent value="api" className="pt-4">
            {/* Relay status */}
            <div className="flex flex-wrap gap-3 border-b border-[#222] pb-2.5 mb-3">
              {RELAYS.map((relay) => (
                <div key={relay} className="flex items-center gap-1.5 font-mono text-[0.7rem] text-[#666]">
                  <span className="h-[5px] w-[5px] rounded-full bg-[#2ecc71]" />
                  {relay}
                </div>
              ))}
            </div>

            {/* Stats */}
            <div className="mb-4 flex flex-wrap gap-4">
              {[
                { value: stats.total, label: "Total Records" },
                { value: stats.domains, label: "Domains" },
                { value: stats.types, label: "Record Types" },
              ].map((s) => (
                <div key={s.label} className="min-w-[120px] flex-1 rounded-lg border border-[#222] bg-[#141414] px-4 py-3">
                  <div data-testid={s.label === "Total Records" ? "stat-total" : undefined} className="text-2xl font-bold text-[#ff6b35]">
                    {loading ? "—" : s.value}
                  </div>
                  <div className="text-[0.75rem] uppercase tracking-wider text-[#666]">
                    {s.label}
                  </div>
                </div>
              ))}
            </div>

            {/* Records grouped by npub */}
            {loading ? (
              <div className="py-8 text-center text-sm text-[#666]">
                Loading records from Nostr events via nodns.shop...
              </div>
            ) : records.length === 0 ? (
              <div className="py-8 text-center text-sm text-[#666]">
                No records found.
              </div>
            ) : (
              Object.entries(grouped).map(([npub, recs]) => (
                <div key={npub} className="mb-1">
                  <div
                    data-testid="npub-group-header"
                    onClick={() => toggleGroup(npub)}
                    className="flex cursor-pointer items-center justify-between rounded-lg border border-[#222] bg-[#141414] px-4 py-3 font-mono text-sm select-none hover:bg-[#1a1a1a]"
                  >
                    <span>
                      <span className="text-[#ff6b35]">{npub.slice(0, 20)}...</span>
                      .nodns.shop{" "}
                      <span className="text-[#666]">({recs.length} records)</span>
                    </span>
                    <span
                      className={`text-[#666] transition-transform ${expandedGroups.has(npub) ? "rotate-90" : ""}`}
                    >
                      ▶
                    </span>
                  </div>
                  {expandedGroups.has(npub) && (
                    <div data-testid="npub-group-records" className="overflow-x-auto">
                      <table className="w-full text-sm">
                        <thead>
                          <tr>
                            <th className="px-3 py-2.5 text-left text-xs font-semibold uppercase tracking-wider text-[#666]">
                              FQDN
                            </th>
                            <th className="px-3 py-2.5 text-left text-xs font-semibold uppercase tracking-wider text-[#666]">
                              Type
                            </th>
                            <th className="px-3 py-2.5 text-left text-xs font-semibold uppercase tracking-wider text-[#666]">
                              Value
                            </th>
                            <th className="px-3 py-2.5 text-left text-xs font-semibold uppercase tracking-wider text-[#666]">
                              TTL
                            </th>
                          </tr>
                        </thead>
                        <tbody>
                          {recs.map((r) => (
                            <tr key={r.fqdn + r.type + r.rdata} className="border-b border-[#222]">
                              <td className="px-3 py-2.5 font-mono text-xs">
                                {r.fqdn}
                              </td>
                              <td className="px-3 py-2.5">
                                <span className="rounded bg-[rgba(255,107,53,0.15)] px-1.5 py-0.5 text-[0.7rem] font-semibold text-[#ff6b35]">
                                  {r.type}
                                </span>
                              </td>
                              <td className="px-3 py-2.5 font-mono text-xs">
                                {r.rdata}
                              </td>
                              <td className="px-3 py-2.5">{r.ttl}</td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </div>
                  )}
                </div>
              ))
            )}

            <div className="mt-4 text-center">
              <button
                onClick={fetchRecords}
                className="rounded-lg bg-[#222] px-3 py-1.5 text-xs font-semibold text-[#e0e0e0] hover:bg-[#333]"
              >
                Refresh
              </button>
              <span className="ml-2 text-[0.75rem] text-[#666]">
                Records sourced from Nostr events via relay subscription &middot;
                Auto-refreshes every 30s
              </span>
            </div>
          </TabsContent>

          {/* DNS Resolver Tab */}
          <TabsContent value="dns" className="pt-4">
            <div className="mb-4 flex gap-2">
              <input
                type="text"
                value={dohFqdn}
                onChange={(e) => setDohFqdn(e.target.value)}
                placeholder="Enter FQDN to query"
                className="flex-1 rounded-lg border border-[#222] bg-[#0a0a0a] px-3 py-2.5 text-sm text-[#e0e0e0] outline-none focus:border-[#ff6b35]"
              />
              <button
                onClick={handleDnsQuery}
                disabled={dohLoading}
                className="rounded-lg bg-[#ff6b35] px-4 py-2.5 text-sm font-semibold text-white hover:opacity-90 disabled:opacity-50"
              >
                Query DNS
              </button>
            </div>
            <div className="mb-4 flex flex-wrap gap-1.5">
              {["A", "TXT", "CNAME", "AAAA", "MX"].map((t) => (
                <button
                  key={t}
                  onClick={() => {
                    setDohType(t);
                  }}
                  className={`rounded border px-2.5 py-1 text-xs font-medium transition-colors ${
                    dohType === t
                      ? "border-[#ff6b35] text-[#ff6b35]"
                      : "border-[#222] text-[#666] hover:border-[#ff6b35] hover:text-[#e0e0e0]"
                  } bg-[#141414]`}
                >
                  {t}
                </button>
              ))}
            </div>
            <div className="mt-4">
              {dohLoading ? (
                <div className="py-8 text-center text-sm text-[#666]">
                  Querying DNS via Cloudflare DoH...
                </div>
              ) : dohResults ? (
                <pre className="overflow-x-auto rounded-lg border border-[#222] bg-[#141414] p-4 text-xs leading-relaxed">
                  <code>{dohResults}</code>
                </pre>
              ) : (
                <div className="py-8 text-center text-sm text-[#666]">
                  Enter a domain and click Query DNS to resolve via Cloudflare
                  DoH.
                </div>
              )}
            </div>
          </TabsContent>

          {/* Nostr Events Tab */}
          <TabsContent value="nostr" className="pt-4">
            {nostrEvents.length === 0 ? (
              <div className="py-8 text-center text-sm text-[#666]">
                Waiting for events from relays...
              </div>
            ) : (
              nostrEvents.map(({ event: ev, relay }) => {
                const recordTags = ev.tags.filter((t) => t[0] === "record");
                const tagSummary =
                  recordTags.map((t) => t[1]).join(", ") || "no records";
                const shortPk = ev.pubkey.slice(0, 12) + "...";
                const time = new Date(ev.created_at * 1000).toLocaleString();

                return (
                  <div
                    key={ev.id}
                    className="mb-2 rounded-lg border border-[#222] bg-[#141414] px-4 py-3"
                  >
                    <div
                      onClick={() =>
                        setExpandedEvents((prev) => {
                          const next = new Set(prev);
                          if (next.has(ev.id)) next.delete(ev.id);
                          else next.add(ev.id);
                          return next;
                        })
                      }
                      className="flex cursor-pointer items-center justify-between select-none hover:opacity-80"
                    >
                      <div>
                        <strong className="font-mono text-sm">{shortPk}</strong>
                        <span className="ml-2 text-xs text-[#666]">{time}</span>
                        <span className="ml-2 text-sm">{tagSummary}</span>
                      </div>
                      <div className="flex items-center">
                        <span className="rounded bg-[rgba(255,107,53,0.15)] px-1.5 py-0.5 font-mono text-[0.7rem] text-[#ff6b35]">
                          {relay.replace("wss://", "")}
                        </span>
                        <span className="ml-2 text-[#666]">▼</span>
                      </div>
                    </div>
                    {expandedEvents.has(ev.id) && (
                      <div className="mt-3 border-t border-[#222] pt-3">
                        <pre className="overflow-x-auto rounded-lg border border-[#222] bg-[#0a0a0a] p-3 text-xs leading-relaxed">
                          <code>{JSON.stringify(ev, null, 2)}</code>
                        </pre>
                      </div>
                    )}
                  </div>
                );
              })
            )}
          </TabsContent>
        </Tabs>
      </div>
    </section>
  );
}
