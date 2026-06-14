"use client";

import { Suspense, useEffect, useMemo, useState } from "react";
import { useSearchParams } from "next/navigation";
import Link from "next/link";
import { SiteHeader } from "@/components/site-header";
import { SiteFooter } from "@/components/site-footer";
import { ErrorBoundary } from "@/components/error-boundary";
import { DEFAULT_ZONE } from "@/lib/constants";
import { sanitizeName, toFqdn } from "@/lib/pricing";
import {
  fetchTripartiteRecords,
  compareTripartite,
  type TripartiteRecords,
} from "@/lib/sources";
import { GlobeIcon, RefreshCwIcon } from "lucide-react";

interface ProfileRecord {
  id: string;
  type: string;
  name: string;
  value: string;
  ttl: number;
  created_at: number;
  sources: string[];
}

function makeRecordId(r: { type: string; name: string; value: string }): string {
  return `${r.type}:${r.name}:${r.value}`;
}

function ProfileContent() {
  const searchParams = useSearchParams();
  const rawDomain = searchParams.get("domain") || "";

  const name = useMemo(() => sanitizeName(rawDomain), [rawDomain]);
  const fqdn = useMemo(() => toFqdn(name), [name]);

  const [tripartite, setTripartite] = useState<TripartiteRecords | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!name) return;

    let cancelled = false;

    async function load() {
      setLoading(true);
      setError(null);
      try {
        const results = await fetchTripartiteRecords({ domain: fqdn });
        if (!cancelled) setTripartite(results);
      } catch (err) {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : "Failed to load domain");
        }
      } finally {
        if (!cancelled) setLoading(false);
      }
    }

    requestAnimationFrame(() => load());
    return () => {
      cancelled = true;
    };
  }, [name, fqdn]);

  const records = useMemo<ProfileRecord[]>(() => {
    if (!tripartite) return [];
    const map = new Map<string, ProfileRecord>();

    for (const r of tripartite.api.records) {
      const id = makeRecordId({ type: r.type, name: r.name || "@", value: r.rdata });
      const existing = map.get(id);
      if (existing) {
        if (!existing.sources.includes("api")) existing.sources.push("api");
        existing.created_at = Math.max(existing.created_at, r.created_at);
      } else {
        map.set(id, {
          id,
          type: r.type,
          name: r.name || "@",
          value: r.rdata,
          ttl: r.ttl,
          created_at: r.created_at,
          sources: ["api"],
        });
      }
    }

    for (const r of tripartite.nostr.records) {
      const id = makeRecordId({ type: r.type, name: r.name, value: r.value });
      const existing = map.get(id);
      if (existing) {
        if (!existing.sources.includes("nostr")) existing.sources.push("nostr");
        existing.created_at = Math.max(existing.created_at, r.created_at);
      } else {
        map.set(id, {
          id,
          type: r.type,
          name: r.name,
          value: r.value,
          ttl: r.ttl,
          created_at: r.created_at,
          sources: ["nostr"],
        });
      }
    }

    for (const r of tripartite.dns.records) {
      const label = r.name.split(".")[0] || "@";
      const id = makeRecordId({ type: r.type, name: label, value: r.data });
      const existing = map.get(id);
      if (existing) {
        if (!existing.sources.includes("dns")) existing.sources.push("dns");
      } else {
        map.set(id, {
          id,
          type: r.type,
          name: label,
          value: r.data,
          ttl: r.ttl,
          created_at: 0,
          sources: ["dns"],
        });
      }
    }

    return Array.from(map.values()).sort((a, b) => b.created_at - a.created_at);
  }, [tripartite]);

  const comparison = tripartite ? compareTripartite(tripartite) : null;

  if (!rawDomain) {
    return (
      <div className="mx-auto max-w-[560px] py-20 text-center">
        <div className="mb-5 opacity-20">
          <GlobeIcon className="size-12 mx-auto text-muted-foreground" />
        </div>
        <h1 className="text-lg font-semibold mb-2">No domain specified</h1>
        <p className="text-sm text-foreground/60 mb-8 max-w-[400px] mx-auto">
          Search for a domain to view its tripartite verification, DNS records, and ownership details.
        </p>
        <Link
          href="/"
          className="inline-flex items-center justify-center rounded-lg bg-primary px-6 py-2.5 text-sm font-semibold text-primary-foreground hover:bg-primary/80 transition-colors"
        >
          Search a domain
        </Link>
      </div>
    );
  }

  return (
    <div className="mx-auto max-w-[800px] py-8 md:py-12">
      {/* Domain header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4 mb-8">
        <div className="flex items-center gap-3">
          <div className="flex items-center justify-center size-10 rounded-lg bg-primary/10 shrink-0">
            <GlobeIcon className="size-5 text-primary" />
          </div>
          <div>
            <h1 className="text-xl md:text-2xl font-bold font-mono break-all">
              {name}<span className="text-primary">.{DEFAULT_ZONE}</span>
            </h1>
            <div className="flex items-center gap-3 mt-1">
              {records.length > 0 ? (
                <span className="inline-flex items-center gap-1.5 text-sm text-emerald-400">
                  <span className="inline-block h-2 w-2 rounded-full bg-emerald-400" />
                  {records.length} record{records.length !== 1 ? "s" : ""}
                </span>
              ) : loading ? (
                <span className="text-sm text-muted-foreground animate-pulse">Loading...</span>
              ) : (
                <span className="inline-flex items-center gap-1.5 text-sm text-yellow-400">
                  <span className="inline-block h-2 w-2 rounded-full bg-yellow-400" />
                  No records found
                </span>
              )}
            </div>
          </div>
        </div>
        <button
          onClick={() => {
            if (!loading) {
              setLoading(true);
              fetchTripartiteRecords({ domain: fqdn }).then(setTripartite).finally(() => setLoading(false));
            }
          }}
          disabled={loading}
          className="inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground transition-colors"
        >
          <RefreshCwIcon className={`size-3.5 ${loading ? "animate-spin" : ""}`} />
          Refresh
        </button>
      </div>

      {/* Error */}
      {error && (
        <div className="rounded-lg border border-red-800 bg-red-950/40 px-4 py-3 text-sm text-red-400 mb-6">
          {error}
        </div>
      )}

      {/* Source verification bar */}
      {comparison && (
        <div className="flex items-center gap-4 mb-6 px-4 py-3 rounded-lg bg-card ring-1 ring-foreground/10">
          <span className="text-xs text-muted-foreground uppercase tracking-wider font-medium">Sources</span>
          <span className="flex items-center gap-1.5 text-sm">
            <span>🗄️</span>
            <span className="text-xs text-muted-foreground">{comparison.apiCount}</span>
          </span>
          <span className="text-border">|</span>
          <span className="flex items-center gap-1.5 text-sm">
            <span>🔐</span>
            <span className="text-xs text-muted-foreground">{comparison.nostrCount}</span>
          </span>
          <span className="text-border">|</span>
          <span className="flex items-center gap-1.5 text-sm">
            <span>🌐</span>
            <span className="text-xs text-muted-foreground">{comparison.dnsCount}</span>
          </span>
          <span className="ml-auto text-xs">
            {comparison.match ? (
              <span className="text-emerald-400">✓ Sources agree</span>
            ) : (
              <span className="text-yellow-400">⚠ Sources differ</span>
            )}
          </span>
        </div>
      )}

      {/* Records table */}
      <div className="rounded-xl bg-card ring-1 ring-foreground/10 overflow-hidden">
        {/* Table header (desktop) */}
        <div className="hidden md:grid grid-cols-[80px_100px_1fr_80px_70px] gap-3 px-5 py-2.5 border-b border-border text-xs text-muted-foreground font-medium uppercase tracking-wider">
          <span>Type</span>
          <span>Name</span>
          <span>Value</span>
          <span className="text-center">TTL</span>
          <span className="text-center">Src</span>
        </div>

        {/* Loading skeleton */}
        {loading && records.length === 0 && (
          <div>
            {[1, 2, 3].map((i) => (
              <div
                key={i}
                className="grid grid-cols-1 md:grid-cols-[80px_100px_1fr_80px_70px] gap-2 md:gap-3 px-5 py-3.5 border-b border-border last:border-b-0"
              >
                <div className="h-4 w-12 bg-muted rounded animate-pulse" />
                <div className="h-4 w-10 bg-muted rounded animate-pulse" />
                <div className="h-4 w-40 bg-muted rounded animate-pulse" />
                <div className="h-4 w-10 bg-muted rounded animate-pulse" />
                <div className="h-4 w-12 bg-muted rounded animate-pulse" />
              </div>
            ))}
          </div>
        )}

        {/* Empty state */}
        {!loading && records.length === 0 && (
          <div className="px-5 py-12 text-center">
            <p className="text-muted-foreground text-sm">
              No DNS records found for this domain.
            </p>
          </div>
        )}

        {/* Records */}
        {records.map((record) => (
          <div
            key={record.id}
            className="grid grid-cols-1 md:grid-cols-[80px_100px_1fr_80px_70px] gap-2 md:gap-3 px-5 py-3.5 border-b border-border last:border-b-0 hover:bg-muted/30 transition-colors"
          >
            <div>
              <span className="md:hidden text-xs text-muted-foreground mr-1">Type:</span>
              <span className="inline-flex items-center rounded-md bg-secondary px-2 py-0.5 text-xs font-mono font-medium">
                {record.type}
              </span>
            </div>
            <div className="font-mono text-sm truncate">
              <span className="md:hidden text-xs text-muted-foreground mr-1">Name:</span>
              {record.name}
            </div>
            <div className="font-mono text-sm truncate">
              <span className="md:hidden text-xs text-muted-foreground mr-1">Value:</span>
              {record.value}
            </div>
            <div className="text-sm text-muted-foreground text-center">
              <span className="md:hidden text-xs mr-1">TTL:</span>
              {record.ttl}
            </div>
            <div className="flex items-center justify-center gap-0.5">
              <span className="md:hidden text-xs text-muted-foreground mr-1">Sources:</span>
              {record.sources.includes("api") && <span title="API" className="text-xs">🗄️</span>}
              {record.sources.includes("nostr") && <span title="Nostr" className="text-xs">🔐</span>}
              {record.sources.includes("dns") && <span title="DNS" className="text-xs">🌐</span>}
            </div>
          </div>
        ))}
      </div>

      {/* Back link */}
      <div className="mt-6">
        <Link
          href={`/search?q=${encodeURIComponent(rawDomain)}`}
          className="text-sm text-muted-foreground hover:text-foreground transition-colors"
        >
          ← Back to search
        </Link>
      </div>
    </div>
  );
}

export default function ProfilePage() {
  return (
    <ErrorBoundary>
      <SiteHeader />
      <main id="main-content" className="px-6 pb-16">
        <Suspense
          fallback={
            <div className="mx-auto max-w-[800px] py-20 text-center text-muted-foreground animate-pulse">
              Loading domain profile...
            </div>
          }
        >
          <ProfileContent />
        </Suspense>
      </main>
      <SiteFooter />
    </ErrorBoundary>
  );
}
