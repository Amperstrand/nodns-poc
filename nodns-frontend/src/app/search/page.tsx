"use client";

import { Suspense, useCallback, useEffect, useMemo, useState } from "react";
import { useSearchParams } from "next/navigation";
import Link from "next/link";
import { SiteHeader } from "@/components/site-header";
import { SiteFooter } from "@/components/site-footer";
import { ErrorBoundary } from "@/components/error-boundary";
import { DEFAULT_ZONE } from "@/lib/constants";
import { getPriceForName, sanitizeName, toFqdn } from "@/lib/pricing";
import {
  fetchTripartiteRecords,
  fetchPricing,
  compareTripartite,
  type TripartiteRecords,
} from "@/lib/sources";
import type { ZonePricing } from "@/lib/types";

function statusDot(status: string) {
  if (status === "ok") return "🟢";
  if (status === "error") return "🔴";
  if (status === "loading") return "🟡";
  return "⚫";
}

function SearchContent() {
  const searchParams = useSearchParams();
  const q = searchParams.get("q") || "";

  const [available, setAvailable] = useState<boolean | null>(null);
  const [loading, setLoading] = useState(false);
  const [pricing, setPricing] = useState<ZonePricing | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [tripartite, setTripartite] = useState<TripartiteRecords | null>(null);

  const name = useMemo(() => sanitizeName(q), [q]);

  useEffect(() => {
    if (!name) return;

    let cancelled = false;

    async function check() {
      setLoading(true);
      setError(null);
      try {
        const fqdn = toFqdn(name);

        const results = await fetchTripartiteRecords({ domain: fqdn });
        if (!cancelled) {
          setTripartite(results);
          const anyRecords =
            results.api.records.length > 0 ||
            results.nostr.records.length > 0 ||
            results.dns.records.length > 0;
          setAvailable(!anyRecords);
        }

        try {
          const p = await fetchPricing();
          if (!cancelled) setPricing(p);
        } catch {
          // Pricing fetch failure is non-critical
        }
      } catch (err) {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : "Failed to check availability");
          setAvailable(null);
        }
      } finally {
        if (!cancelled) setLoading(false);
      }
    }

    requestAnimationFrame(() => check());
    return () => { cancelled = true; };
  }, [name]);

  const price = name ? getPriceForName(name) : 0;

  const suggestions = useCallback(() => {
    if (available !== false || !name) return [];
    const prefixes = ["my", "the", "go", "hi"];
    return prefixes.map((p) => `${p}${name}`).slice(0, 3);
  }, [available, name]);

  const comparison = tripartite ? compareTripartite(tripartite) : null;

  if (!q) {
    return (
      <div className="mx-auto max-w-[640px] py-20 text-center">
        <h1 className="text-2xl font-bold mb-3">Search for a domain</h1>
        <p className="text-muted-foreground">
          Use the search bar above or{" "}
          <Link href="/" className="text-primary hover:underline">
            go home
          </Link>{" "}
          to find your .nodns.shop domain.
        </p>
      </div>
    );
  }

  return (
    <div className="mx-auto max-w-[640px] py-12">
      {/* Domain result */}
      <div className="rounded-xl border border-border bg-card p-6 mb-6">
        <div className="flex items-center justify-between mb-4">
          <h1 className="text-xl font-bold font-mono">
            {name}.<span className="text-primary">{DEFAULT_ZONE}</span>
          </h1>
          {loading && (
            <span className="text-sm text-muted-foreground animate-pulse">Checking...</span>
          )}
        </div>

        {error && (
          <div className="rounded-lg border border-red-800 bg-red-950/40 px-4 py-3 text-sm text-red-400 mb-4">
            {error}
          </div>
        )}

        {available === true && (
          <>
            <div className="flex items-center gap-2 mb-4">
              <span className="inline-block h-2.5 w-2.5 rounded-full bg-emerald-400" />
              <span className="text-emerald-400 font-semibold text-lg">Available!</span>
            </div>
            <div className="flex items-baseline gap-2 mb-5">
              <span className="text-3xl font-bold text-foreground">{price}</span>
              <span className="text-sm text-muted-foreground">sats/year</span>
            </div>
            {pricing && (
              <p className="text-xs text-muted-foreground mb-5">
                Base create price: {pricing.create_price} sats · Update: {pricing.update_price} sats ·
                Mint: {new URL(pricing.mint_url).hostname}
              </p>
            )}
            <Link
              href={`/register?name=${encodeURIComponent(name)}`}
              className="inline-flex items-center justify-center rounded-lg bg-primary px-8 py-3 text-sm font-semibold text-primary-foreground hover:bg-primary/80 transition-colors"
            >
              Register this domain →
            </Link>
          </>
        )}

        {available === false && (
          <>
            <div className="flex items-center gap-2 mb-4">
              <span className="inline-block h-2.5 w-2.5 rounded-full bg-red-400" />
              <span className="text-red-400 font-semibold text-lg">Already registered</span>
            </div>
            <p className="text-sm text-muted-foreground mb-5">
              This domain is taken. Try a different name or check out these alternatives:
            </p>
            {suggestions().length > 0 && (
              <div className="flex flex-wrap gap-2 mb-5">
                {suggestions().map((s) => (
                  <Link
                    key={s}
                    href={`/search?q=${encodeURIComponent(s)}`}
                    className="rounded-lg border border-border bg-background px-3 py-1.5 text-sm font-mono text-foreground hover:border-primary/50 hover:text-primary transition-colors"
                  >
                    {s}.{DEFAULT_ZONE}
                  </Link>
                ))}
              </div>
            )}
            <Link
              href="/"
              className="text-sm text-primary hover:underline"
            >
              ← Search again
            </Link>
          </>
        )}
      </div>

      {/* Source status */}
      {tripartite && (
        <div className="rounded-xl border border-border bg-card p-4 mb-6">
          <div className="flex items-center gap-4 text-sm">
            <span className="text-xs text-muted-foreground uppercase tracking-wider font-medium">Sources</span>
            <span className="flex items-center gap-1.5">
              {tripartite.api.icon} {statusDot(tripartite.api.status)} <span className="text-xs">{comparison?.apiCount ?? 0} records</span>
            </span>
            <span className="text-border">|</span>
            <span className="flex items-center gap-1.5">
              {tripartite.nostr.icon} {statusDot(tripartite.nostr.status)} <span className="text-xs">{comparison?.nostrCount ?? 0} records</span>
            </span>
            <span className="text-border">|</span>
            <span className="flex items-center gap-1.5">
              {tripartite.dns.icon} {statusDot(tripartite.dns.status)} <span className="text-xs">{comparison?.dnsCount ?? 0} records</span>
            </span>
            {comparison && (
              <span className="ml-auto text-xs">
                {comparison.match ? (
                  <span className="text-emerald-400">✓ Sources agree</span>
                ) : (
                  <span className="text-yellow-400">⚠ Sources differ</span>
                )}
              </span>
            )}
          </div>
        </div>
      )}

      {/* Pricing tiers */}
      <div className="rounded-xl border border-border bg-card p-6">
        <h2 className="text-sm font-semibold mb-4 text-muted-foreground uppercase tracking-wider">
          Pricing tiers
        </h2>
        <div className="grid grid-cols-3 gap-4">
          <div className={`rounded-lg border p-3 text-center ${name.length <= 3 ? "border-primary bg-primary/10" : "border-border"}`}>
            <div className="text-xs text-muted-foreground mb-1">1-3 chars</div>
            <div className="text-xl font-bold">200</div>
            <div className="text-xs text-muted-foreground">sats</div>
          </div>
          <div className={`rounded-lg border p-3 text-center ${name.length >= 4 && name.length <= 6 ? "border-primary bg-primary/10" : "border-border"}`}>
            <div className="text-xs text-muted-foreground mb-1">4-6 chars</div>
            <div className="text-xl font-bold">20</div>
            <div className="text-xs text-muted-foreground">sats</div>
          </div>
          <div className={`rounded-lg border p-3 text-center ${name.length >= 7 ? "border-primary bg-primary/10" : "border-border"}`}>
            <div className="text-xs text-muted-foreground mb-1">7+ chars</div>
            <div className="text-xl font-bold">4</div>
            <div className="text-xs text-muted-foreground">sats</div>
          </div>
        </div>
      </div>
    </div>
  );
}

export default function SearchPage() {
  return (
    <ErrorBoundary>
      <SiteHeader />
      <main className="px-6 pb-16">
        <Suspense
          fallback={
            <div className="mx-auto max-w-[640px] py-20 text-center text-muted-foreground animate-pulse">
              Loading search results...
            </div>
          }
        >
          <SearchContent />
        </Suspense>
      </main>
      <SiteFooter />
    </ErrorBoundary>
  );
}
