"use client";

import { useState, useEffect, useCallback, useRef } from "react";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import { RELAYS, DNS_TYPES, DNS_STATUS_CODES, DEFAULT_ZONE } from "@/lib/constants";
import { queryDoh } from "@/lib/dns";
import { queryAllRecentRecords, subscribeToDnsEvents } from "@/lib/nostr";
import type {
  ApiRecordsResponse,
  EventWithRelay,
  NostrEvent,
} from "@/lib/types";
import {
  RefreshCwIcon,
  DatabaseIcon,
  GlobeIcon,
  ShieldIcon,
  LayersIcon,
  ChevronRightIcon,
  ChevronDownIcon,
  SearchIcon,
  RadioIcon,
} from "lucide-react";

const API_BASE = process.env.NEXT_PUBLIC_API_BASE ?? "";

interface MergedRecord {
  id: string;
  type: string;
  name: string;
  value: string;
  ttl: number;
  fqdn: string;
  npub: string;
  created_at: number;
  sources: string[];
}

function makeRecordId(r: {
  type: string;
  name: string;
  value: string;
}): string {
  return `${r.type}:${r.name}:${r.value}`;
}

export function RecordBrowser() {
  const [activeTab, setActiveTab] = useState("api");
  const [mergedRecords, setMergedRecords] = useState<MergedRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [stats, setStats] = useState({ total: 0, domains: 0, types: 0 });
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set());
  const [nostrEvents, setNostrEvents] = useState<EventWithRelay[]>([]);
  const [expandedEvents, setExpandedEvents] = useState<Set<string>>(new Set());
  const [recordFilter, setRecordFilter] = useState("");
  const fetchRef = useRef<(() => void) | null>(null);

  const [dohFqdn, setDohFqdn] = useState(
    "npub190queyng2pmx0jfw5rkx4fjjl3u0zxz6nlyaja53p2n0ydupr6jsdnqt8q.nodns.shop"
  );
  const [dohType, setDohType] = useState("A");
  const [dohResults, setDohResults] = useState<string>("");
  const [dohLoading, setDohLoading] = useState(false);

  const fetchRecords = useCallback(async () => {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), 30_000);
    try {
      const [apiResp, nostrRecs] = await Promise.all([
        fetch(`${API_BASE}/api/records`, { signal: controller.signal }).then(
          async (resp) => {
            if (!resp.ok) return [];
            const data: ApiRecordsResponse = await resp.json();
            return data.records || [];
          }
        ),
        queryAllRecentRecords(DEFAULT_ZONE, { limit: 200 }),
      ]);

      const recordMap = new Map<string, MergedRecord>();

      for (const r of apiResp) {
        const name = r.name || "@";
        const id = makeRecordId({ type: r.type, name, value: r.rdata });
        const existing = recordMap.get(id);
        if (existing) {
          if (!existing.sources.includes("api")) existing.sources.push("api");
          existing.created_at = Math.max(existing.created_at, r.created_at);
        } else {
          recordMap.set(id, {
            id,
            type: r.type,
            name,
            value: r.rdata,
            ttl: r.ttl,
            fqdn: r.fqdn,
            npub: r.npub,
            created_at: r.created_at,
            sources: ["api"],
          });
        }
      }

      for (const r of nostrRecs) {
        const name = r.name || "@";
        const id = makeRecordId({ type: r.type, name, value: r.value });
        const existing = recordMap.get(id);
        if (existing) {
          if (!existing.sources.includes("nostr"))
            existing.sources.push("nostr");
          existing.created_at = Math.max(existing.created_at, r.created_at);
        } else {
          recordMap.set(id, {
            id,
            type: r.type,
            name,
            value: r.value,
            ttl: r.ttl,
            fqdn: r.fqdn,
            npub: r.pubkey,
            created_at: r.created_at,
            sources: ["nostr"],
          });
        }
      }

      const merged = Array.from(recordMap.values()).sort(
        (a, b) => b.created_at - a.created_at
      );
      setMergedRecords(merged);

      const npubs = [...new Set(merged.map((r) => r.npub))];
      const types = [...new Set(merged.map((r) => r.type))];
      setStats({
        total: merged.length,
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
      setMergedRecords([]);
    } finally {
      setLoading(false);
      clearTimeout(timer);
    }
  }, []);

  useEffect(() => {
    fetchRef.current = fetchRecords;
  }, [fetchRecords]);

  useEffect(() => {
    fetchRef.current?.();
    const interval = setInterval(() => fetchRef.current?.(), 30000);
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
            (a) => `${a.name} ${DNS_TYPES[a.type] ?? a.type} ${a.TTL}s ${a.data}`
          ).join("\n")
        );
      } else if (data.Status === 0) {
        setDohResults("Query succeeded but no records found.");
      } else {
        setDohResults(
          `DNS error: ${DNS_STATUS_CODES[data.Status] ?? "Status " + data.Status}`
        );
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : "Unknown error";
      setDohResults(`Query failed: ${msg}`);
    }
    setDohLoading(false);
  }, [dohFqdn, dohType]);

  const filteredRecords = recordFilter.trim()
    ? mergedRecords.filter(
        (r) =>
          r.fqdn.toLowerCase().includes(recordFilter.toLowerCase()) ||
          r.type.toLowerCase().includes(recordFilter.toLowerCase()) ||
          r.value.toLowerCase().includes(recordFilter.toLowerCase()) ||
          r.npub.toLowerCase().includes(recordFilter.toLowerCase()),
      )
    : mergedRecords;

  const grouped: Record<string, MergedRecord[]> = {};
  for (const r of filteredRecords) {
    const key = r.npub || r.fqdn;
    if (!grouped[key]) grouped[key] = [];
    grouped[key].push(r);
  }

  const trustBadge =
    activeTab === "api"
      ? { icon: "🗄️", text: "Source: API database + Nostr relay merge", authority: false }
      : activeTab === "dns"
        ? { icon: "🌐", text: "Source: DNS resolver via Cloudflare DoH", authority: false }
        : { icon: "🔐", text: "Source: Signed Nostr events — cryptographic authority", authority: true };

  const sourceBadge = (source: string) => {
    if (source === "api")
      return (
        <span className="text-xs px-1.5 py-0.5 rounded bg-secondary" title="API confirmed">
          🗄
        </span>
      );
    if (source === "nostr")
      return (
        <span className="text-xs px-1.5 py-0.5 rounded bg-secondary" title="Nostr confirmed">
          🔐
        </span>
      );
    if (source === "dns")
      return (
        <span className="text-xs px-1.5 py-0.5 rounded bg-secondary" title="DNS confirmed">
          🌐
        </span>
      );
    return null;
  };

  return (
    <div className="mx-auto max-w-[960px] py-8 md:py-12">
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4 mb-6">
        <div>
          <h1 className="text-2xl font-bold mb-1">DNS Record Browser</h1>
          <p className="text-sm text-muted-foreground">
            Browse all DNS records verified from three independent sources
          </p>
        </div>
        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={fetchRecords}
            disabled={loading}
          >
            <RefreshCwIcon
              className={`size-3.5 ${loading ? "animate-spin" : ""}`}
            />
            Refresh
          </Button>
        </div>
      </div>

      <Tabs value={activeTab} onValueChange={setActiveTab}>
        <TabsList className="mb-4 h-auto gap-0 rounded-none border-b border-border bg-transparent p-0">
          <TabsTrigger
            value="api"
            className="rounded-none border-b-2 border-transparent bg-transparent px-5 py-2.5 text-sm font-semibold text-muted-foreground data-[state=active]:border-primary data-[state=active]:text-primary data-[state=active]:shadow-none data-[state=active]:bg-transparent"
          >
            <DatabaseIcon className="size-3.5 mr-1.5" />
            API + Nostr
            {stats.total > 0 && (
              <span className="ml-1.5 rounded-full bg-secondary px-1.5 py-0.5 text-xs font-mono text-muted-foreground">
                {stats.total}
              </span>
            )}
          </TabsTrigger>
          <TabsTrigger
            value="dns"
            className="rounded-none border-b-2 border-transparent bg-transparent px-5 py-2.5 text-sm font-semibold text-muted-foreground data-[state=active]:border-primary data-[state=active]:text-primary data-[state=active]:shadow-none data-[state=active]:bg-transparent"
          >
            <GlobeIcon className="size-3.5 mr-1.5" />
            DNS Resolver
          </TabsTrigger>
          <TabsTrigger
            value="nostr"
            className="rounded-none border-b-2 border-transparent bg-transparent px-5 py-2.5 text-sm font-semibold text-muted-foreground data-[state=active]:border-primary data-[state=active]:text-primary data-[state=active]:shadow-none data-[state=active]:bg-transparent"
          >
            <RadioIcon className="size-3.5 mr-1.5" />
            Nostr Events
            {nostrEvents.length > 0 && (
              <span className="ml-1.5 rounded-full bg-secondary px-1.5 py-0.5 text-xs font-mono text-muted-foreground">
                {nostrEvents.length}
              </span>
            )}
          </TabsTrigger>
        </TabsList>

        {/* Trust badge */}
        <div
          className={`flex items-center gap-2 rounded-lg border px-3.5 py-2 text-sm mb-4 ${
            trustBadge.authority
              ? "border-emerald-800 bg-emerald-950/30 text-emerald-400"
              : "border-border bg-card text-muted-foreground"
          }`}
        >
          <span>{trustBadge.icon}</span> {trustBadge.text}
        </div>

        {/* API + Nostr merged tab */}
        <TabsContent value="api" className="pt-0">
          {/* Relay status */}
          <div className="flex flex-wrap gap-3 border-b border-border pb-2.5 mb-4">
            {RELAYS.map((relay) => (
              <div
                key={relay}
                className="flex items-center gap-1.5 font-mono text-xs text-muted-foreground"
              >
                <span className="h-1.5 w-1.5 rounded-full bg-emerald-400" />
                {relay}
              </div>
            ))}
          </div>

          {/* Stats */}
          <div className="mb-4 flex flex-wrap gap-4">
            {[
              {
                value: stats.total,
                label: "Total Records",
                icon: DatabaseIcon,
                testId: "stat-total",
              },
              { value: stats.domains, label: "Domains", icon: GlobeIcon },
              { value: stats.types, label: "Record Types", icon: LayersIcon },
            ].map((s) => (
              <div
                key={s.label}
                data-testid={s.testId}
                className="min-w-[120px] flex-1 rounded-lg bg-card ring-1 ring-foreground/10 px-4 py-3"
              >
                <div className="flex items-center gap-2">
                  <s.icon className="size-4 text-primary" />
                  <span className="text-2xl font-bold text-primary">
                    {loading ? "—" : s.value}
                  </span>
                </div>
                <div className="text-xs uppercase tracking-wider text-muted-foreground mt-0.5">
                  {s.label}
                </div>
              </div>
            ))}
          </div>

          {/* Search filter */}
          {!loading && mergedRecords.length > 0 && (
            <div className="relative mb-4">
              <SearchIcon className="absolute left-3 top-1/2 -translate-y-1/2 size-4 text-muted-foreground pointer-events-none" />
              <input
                type="text"
                value={recordFilter}
                onChange={(e) => setRecordFilter(e.target.value)}
                placeholder="Filter by domain, type, value, or npub..."
                className="w-full rounded-lg border border-input bg-transparent pl-10 pr-4 py-2.5 text-sm text-foreground placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 outline-none"
              />
              {recordFilter && (
                <button
                  onClick={() => setRecordFilter("")}
                  className="absolute right-3 top-1/2 -translate-y-1/2 text-xs text-muted-foreground hover:text-foreground"
                >
                  Clear
                </button>
              )}
            </div>
          )}

          {/* Records grouped by npub */}
          {loading ? (
            <div className="rounded-xl bg-card ring-1 ring-foreground/10 overflow-hidden">
              {[1, 2, 3].map((i) => (
                <div
                  key={i}
                  className="px-5 py-4 border-b border-border last:border-b-0"
                >
                  <div className="h-4 w-48 bg-muted rounded animate-pulse mb-2" />
                  <div className="h-3 w-32 bg-muted rounded animate-pulse" />
                </div>
              ))}
            </div>
          ) : mergedRecords.length === 0 ? (
            <div className="rounded-xl border border-dashed border-border bg-card p-12 text-center">
              <DatabaseIcon className="size-12 mx-auto text-muted-foreground opacity-20 mb-4" />
              <p className="text-muted-foreground text-sm">No records found.</p>
            </div>
          ) : (
            Object.entries(grouped).map(([npub, recs]) => (
              <div key={npub} className="mb-2">
                <div
                  data-testid="npub-group-header"
                  onClick={() => toggleGroup(npub)}
                  className="flex cursor-pointer items-center justify-between rounded-lg bg-card ring-1 ring-foreground/10 border-l-2 border-primary px-4 py-3.5 select-none hover:bg-muted/30 transition-colors"
                >
                  <div className="flex items-center gap-2 min-w-0">
                    <GlobeIcon className="size-4 text-muted-foreground shrink-0" />
                    <span className="font-mono text-sm truncate">
                      <span className="text-primary">
                        {npub.slice(0, 20)}...
                      </span>
                      .{DEFAULT_ZONE}
                    </span>
                    <Badge variant="secondary" className="text-xs shrink-0">
                      {recs.length} records
                    </Badge>
                  </div>
                  {expandedGroups.has(npub) ? (
                    <ChevronDownIcon className="size-4 text-muted-foreground shrink-0" />
                  ) : (
                    <ChevronRightIcon className="size-4 text-muted-foreground shrink-0" />
                  )}
                </div>
                {expandedGroups.has(npub) && (
                  <div
                    data-testid="npub-group-records"
                    className="rounded-b-lg bg-card ring-1 ring-foreground/10 ring-t-0 overflow-x-auto"
                  >
                    {/* Table header */}
                    <div className="hidden md:grid grid-cols-[1fr_80px_1fr_60px_70px] gap-3 px-5 py-3 border-b border-border bg-muted/30 text-xs text-foreground/70 font-semibold uppercase tracking-wider">
                      <span>FQDN</span>
                      <span>Type</span>
                      <span>Value</span>
                      <span className="text-center">TTL</span>
                      <span className="text-center">Src</span>
                    </div>
                    {recs.map((r, idx) => (
                      <div key={r.id} className="border-b border-border last:border-b-0">
                        {/* Mobile card layout */}
                        <div className="md:hidden px-4 py-3.5">
                          <div className="flex items-center justify-between mb-2">
                            <span className="inline-flex items-center rounded-md bg-primary/10 ring-1 ring-primary/20 px-2 py-1 text-xs font-mono font-medium text-primary">
                              {r.type}
                            </span>
                            <div className="flex items-center gap-1">
                              {r.sources.map((s) => (
                                <span key={s}>{sourceBadge(s)}</span>
                              ))}
                            </div>
                          </div>
                          <div className="font-mono text-xs font-medium text-foreground mb-1 break-all leading-relaxed">
                            {r.fqdn}
                          </div>
                          <div className="font-mono text-xs text-muted-foreground break-all leading-relaxed">
                            {r.value}
                          </div>
                          <div className="text-xs text-muted-foreground/80 mt-1.5 font-mono">
                            TTL {r.ttl}s
                          </div>
                        </div>
                        {/* Desktop grid layout */}
                        <div className={`hidden md:grid grid-cols-[1fr_80px_1fr_60px_70px] gap-3 px-5 py-4 hover:bg-muted/30 transition-colors ${idx % 2 === 1 ? 'bg-muted/10' : ''}`}>
                          <div className="font-mono text-sm font-medium truncate">
                            {r.fqdn}
                          </div>
                          <div>
                            <span className="inline-flex items-center rounded-md bg-primary/10 ring-1 ring-primary/20 px-2 py-0.5 text-xs font-mono font-medium text-primary">
                              {r.type}
                            </span>
                          </div>
                          <div className="font-mono text-xs truncate">
                            {r.value}
                          </div>
                          <div className="text-sm text-muted-foreground text-center">
                            {r.ttl}
                          </div>
                          <div className="flex items-center justify-center gap-0.5">
                            {r.sources.map((s) => (
                              <span key={s}>{sourceBadge(s)}</span>
                            ))}
                          </div>
                        </div>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            ))
          )}

          <div className="mt-4 flex items-center justify-center gap-3 text-xs text-muted-foreground">
            <span>
              Records sourced from API + Nostr relay query · Auto-refreshes
              every 30s
            </span>
          </div>
        </TabsContent>

        {/* DNS Resolver Tab */}
        <TabsContent value="dns" className="pt-0">
          <div className="mb-4 flex gap-2">
            <Input
              value={dohFqdn}
              onChange={(e) => setDohFqdn(e.target.value)}
              placeholder="Enter FQDN to query"
              className="flex-1 font-mono text-sm"
            />
            <Button onClick={handleDnsQuery} disabled={dohLoading}>
              <SearchIcon className="size-3.5" />
              Query DNS
            </Button>
          </div>
          <div className="mb-4 flex flex-wrap gap-1.5">
            {["A", "TXT", "CNAME", "AAAA", "MX"].map((t) => (
              <Button
                key={t}
                variant={dohType === t ? "default" : "outline"}
                size="sm"
                onClick={() => setDohType(t)}
                className="text-xs"
              >
                {t}
              </Button>
            ))}
          </div>
          <div className="mt-4">
            {dohLoading ? (
              <div className="py-8 text-center text-sm text-muted-foreground">
                <RefreshCwIcon className="size-4 mx-auto mb-2 animate-spin" />
                Querying DNS via Cloudflare DoH...
              </div>
            ) : dohResults ? (
              <pre className="overflow-x-auto rounded-lg border border-border bg-card p-4 text-xs leading-relaxed font-mono">
                <code>{dohResults}</code>
              </pre>
            ) : (
              <div className="rounded-xl border border-dashed border-border bg-card p-12 text-center">
                <GlobeIcon className="size-12 mx-auto text-muted-foreground opacity-20 mb-4" />
                <p className="text-muted-foreground text-sm">
                  Enter a domain and click Query DNS to resolve via Cloudflare
                  DoH.
                </p>
              </div>
            )}
          </div>
        </TabsContent>

        {/* Nostr Events Tab */}
        <TabsContent value="nostr" className="pt-0">
          {nostrEvents.length === 0 ? (
            <div className="rounded-xl border border-dashed border-border bg-card p-12 text-center">
              <RadioIcon className="size-12 mx-auto text-muted-foreground opacity-20 mb-4" />
              <p className="text-muted-foreground text-sm">
                Waiting for live events from relays...
              </p>
              <div className="flex flex-wrap justify-center gap-2 mt-4">
                {RELAYS.map((relay) => (
                  <span
                    key={relay}
                    className="text-xs font-mono text-muted-foreground flex items-center gap-1"
                  >
                    <span className="h-1.5 w-1.5 rounded-full bg-emerald-400" />
                    {relay}
                  </span>
                ))}
              </div>
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
                  className="mb-2 rounded-lg bg-card ring-1 ring-foreground/10 px-4 py-3"
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
                    <div className="flex items-center gap-2 min-w-0">
                      <ShieldIcon className="size-3.5 text-primary shrink-0" />
                      <span className="font-mono text-sm font-medium">
                        {shortPk}
                      </span>
                      <span className="text-xs text-muted-foreground">
                        {time}
                      </span>
                      <span className="text-sm">{tagSummary}</span>
                    </div>
                    <div className="flex items-center gap-2 shrink-0">
                      <span className="rounded-md bg-secondary px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground">
                        {relay.replace("wss://", "")}
                      </span>
                      {expandedEvents.has(ev.id) ? (
                        <ChevronDownIcon className="size-3 text-muted-foreground" />
                      ) : (
                        <ChevronRightIcon className="size-3 text-muted-foreground" />
                      )}
                    </div>
                  </div>
                  {expandedEvents.has(ev.id) && (
                    <div className="mt-3 border-t border-border pt-3">
                      <pre className="overflow-x-auto rounded-lg border border-border bg-background p-3 text-xs leading-relaxed font-mono">
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
  );
}
