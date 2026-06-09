"use client";

import { Suspense, useCallback, useEffect, useMemo, useState } from "react";
import Link from "next/link";
import { SiteHeader } from "@/components/site-header";
import { SiteFooter } from "@/components/site-footer";
import { ErrorBoundary } from "@/components/error-boundary";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { useIdentity } from "@/contexts/IdentityContext";
import { useWallet } from "@/contexts/WalletContext";
import { DEFAULT_ZONE } from "@/lib/constants";
import { fetchZonePricing } from "@/lib/api";
import { hexPk } from "@/lib/identity";
import { subscribeToDnsEvents } from "@/lib/nostr";
import type { NostrEvent, ZonePricing } from "@/lib/types";
import {
  PlusIcon,
  GlobeIcon,
  ArrowRightIcon,
  RefreshCwIcon,
  LayersIcon,
} from "lucide-react";

interface DomainInfo {
  name: string;
  fqdn: string;
  recordCount: number;
  lastSeen: number;
}

type Status = "loading" | "ready" | "error";

function DashboardContent() {
  const { npub, initialized, nsec } = useIdentity();
  const { balance, status: walletStatus } = useWallet();
  const [domains, setDomains] = useState<DomainInfo[]>([]);
  const [pageStatus, setPageStatus] = useState<Status>("loading");
  const [errorMsg, setErrorMsg] = useState("");
  const [pricing, setPricing] = useState<ZonePricing | null>(null);

  const loadRecords = useCallback(async () => {
    if (!initialized || !npub) return;

    setPageStatus("loading");
    setErrorMsg("");

    try {
      const pk = hexPk(npub);

      const resp = await fetch(
        `/api/records?pubkey=${encodeURIComponent(pk)}`
      );

      if (!resp.ok) {
        throw new Error(`HTTP ${resp.status}`);
      }

      const data = await resp.json();
      const records: Array<{
        name: string;
        fqdn: string;
        type: string;
        created_at: number;
      }> = data.records ?? [];

      // Group by fqdn to get domain list
      const domainMap = new Map<string, DomainInfo>();
      for (const rec of records) {
        const fqdn = rec.fqdn || `${rec.name}.${DEFAULT_ZONE}`;
        const existing = domainMap.get(fqdn);
        if (existing) {
          existing.recordCount += 1;
          existing.lastSeen = Math.max(existing.lastSeen, rec.created_at);
        } else {
          const label = fqdn.replace(`.${DEFAULT_ZONE}`, "");
          domainMap.set(fqdn, {
            name: label,
            fqdn,
            recordCount: 1,
            lastSeen: rec.created_at,
          });
        }
      }

      setDomains(
        Array.from(domainMap.values()).sort(
          (a, b) => b.lastSeen - a.lastSeen
        )
      );
    } catch (err) {
      setErrorMsg(
        err instanceof Error ? err.message : "Failed to load domains"
      );
    } finally {
      setPageStatus("ready");
    }
  }, [initialized, npub]);

  // Fetch pricing
  useEffect(() => {
    fetchZonePricing(DEFAULT_ZONE)
      .then(setPricing)
      .catch(() => {
        // Pricing fetch is non-fatal
      });
  }, []);

  // Load on init
  useEffect(() => {
    loadRecords();
  }, [loadRecords]);

  // Live subscription for updates
  useEffect(() => {
    if (!initialized) return;

    const unsub = subscribeToDnsEvents((event: NostrEvent) => {
      // Re-fetch on new events
      loadRecords();
    });

    return unsub;
  }, [initialized, loadRecords]);

  // Derived expiry date (approximate: 1 year from last seen)
  const getExpiryDate = (lastSeen: number): string => {
    const expiry = new Date((lastSeen + 365 * 86400) * 1000);
    return expiry.toLocaleDateString("en-US", {
      year: "numeric",
      month: "short",
    });
  };

  const getStatusBadge = (lastSeen: number) => {
    const now = Math.floor(Date.now() / 1000);
    const age = now - lastSeen;
    const oneYear = 365 * 86400;

    if (age > oneYear) {
      return (
        <Badge className="border border-red-800 bg-red-950/60 text-red-400">
          Expired
        </Badge>
      );
    }
    if (age > oneYear * 0.9) {
      return (
        <Badge className="border border-yellow-800 bg-yellow-950/60 text-yellow-400">
          Grace
        </Badge>
      );
    }
    return (
      <Badge className="border border-emerald-800 bg-emerald-950/60 text-emerald-400">
        Active
      </Badge>
    );
  };

  return (
    <div className="mx-auto max-w-[960px] py-8 md:py-12">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4 mb-8">
        <div>
          <h1 className="text-2xl font-bold mb-1">My Domains</h1>
          <p className="text-sm text-muted-foreground">
            Manage your registered subdomains
            {pricing && (
              <span className="ml-2 text-xs">
                (Create: {pricing.create_price} sats)
              </span>
            )}
          </p>
        </div>
        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={loadRecords}
            disabled={pageStatus === "loading"}
          >
            <RefreshCwIcon
              className={`size-3.5 ${pageStatus === "loading" ? "animate-spin" : ""}`}
            />
            Refresh
          </Button>
          <Link href="/">
            <Button size="sm">
              <PlusIcon className="size-3.5" />
              Register
            </Button>
          </Link>
        </div>
      </div>

      {/* Wallet status bar */}
      <div className="flex items-center gap-4 mb-6 px-4 py-3 rounded-lg bg-card ring-1 ring-foreground/10">
        <div className="flex items-center gap-2">
          <span className="text-xs text-muted-foreground">Wallet</span>
          <span
            className={`text-xs font-mono ${walletStatus === "ready" ? "text-emerald-400" : walletStatus === "error" ? "text-red-400" : "text-yellow-400"}`}
          >
            {balance} sats
          </span>
        </div>
        <div className="h-3 w-px bg-border" />
        <div className="flex items-center gap-2">
          <span className="text-xs text-muted-foreground">Identity</span>
          <span className="text-xs font-mono text-foreground">
            {npub ? `${npub.slice(0, 12)}...` : "—"}
          </span>
        </div>
      </div>

      {/* Error state */}
      {errorMsg && (
        <div className="rounded-lg border border-red-800 bg-red-950/40 px-4 py-3 text-sm text-red-400 mb-6">
          {errorMsg}
          <button
            onClick={loadRecords}
            className="ml-2 underline hover:no-underline"
          >
            Retry
          </button>
        </div>
      )}

      {/* Loading skeleton */}
      {pageStatus === "loading" && domains.length === 0 && (
        <div className="rounded-xl bg-card ring-1 ring-foreground/10 overflow-hidden">
          <div className="hidden md:grid grid-cols-[1fr_100px_90px_110px_100px] gap-4 px-5 py-3 border-b border-border text-xs text-muted-foreground font-medium">
            <span>Domain</span>
            <span>Status</span>
            <span>Records</span>
            <span>Expires</span>
            <span className="text-right">Actions</span>
          </div>
          {[1, 2, 3].map((i) => (
            <div
              key={i}
              className="grid grid-cols-1 md:grid-cols-[1fr_100px_90px_110px_100px] gap-2 md:gap-4 px-5 py-4 border-b border-border last:border-b-0"
            >
              <div className="h-4 w-48 bg-muted rounded animate-pulse" />
              <div className="h-4 w-16 bg-muted rounded animate-pulse" />
              <div className="h-4 w-8 bg-muted rounded animate-pulse" />
              <div className="h-4 w-20 bg-muted rounded animate-pulse" />
              <div className="h-4 w-16 bg-muted rounded animate-pulse" />
            </div>
          ))}
        </div>
      )}

      {/* Domain table */}
      {pageStatus !== "loading" && domains.length > 0 && (
        <div className="rounded-xl bg-card ring-1 ring-foreground/10 overflow-hidden">
          {/* Desktop header */}
          <div className="hidden md:grid grid-cols-[1fr_100px_90px_110px_100px] gap-4 px-5 py-3 border-b border-border text-xs text-muted-foreground font-medium uppercase tracking-wider">
            <span>Domain</span>
            <span>Status</span>
            <span>Records</span>
            <span>Expires</span>
            <span className="text-right">Actions</span>
          </div>

          {domains.map((domain) => (
            <div
              key={domain.fqdn}
              className="grid grid-cols-1 md:grid-cols-[1fr_100px_90px_110px_100px] gap-2 md:gap-4 px-5 py-4 border-b border-border last:border-b-0 hover:bg-muted/30 transition-colors"
            >
              {/* Domain name */}
              <div className="flex items-center gap-2">
                <GlobeIcon className="size-4 text-muted-foreground shrink-0" />
                <Link
                  href={`/domain?name=${encodeURIComponent(domain.name)}`}
                  className="font-mono text-sm text-foreground hover:text-primary transition-colors truncate"
                >
                  {domain.fqdn}
                </Link>
              </div>

              {/* Status */}
              <div className="flex items-center">
                {getStatusBadge(domain.lastSeen)}
              </div>

              {/* Record count */}
              <div className="flex items-center gap-1.5 text-sm text-muted-foreground">
                <LayersIcon className="size-3.5" />
                {domain.recordCount}
              </div>

              {/* Expires */}
              <div className="text-sm text-muted-foreground">
                {getExpiryDate(domain.lastSeen)}
              </div>

              {/* Actions */}
              <div className="flex items-center justify-end">
                <Link href={`/domain?name=${encodeURIComponent(domain.name)}`}>
                  <Button variant="outline" size="sm">
                    Manage
                    <ArrowRightIcon className="size-3.5" />
                  </Button>
                </Link>
              </div>
            </div>
          ))}
        </div>
      )}

      {/* Empty state */}
      {pageStatus === "ready" && domains.length === 0 && !errorMsg && (
        <div className="rounded-xl border border-dashed border-border bg-card p-12 md:p-16 text-center">
          <div className="text-5xl mb-5 opacity-20">
            <GlobeIcon className="size-12 mx-auto text-muted-foreground" />
          </div>
          <h2 className="text-lg font-semibold mb-2">No domains yet</h2>
          <p className="text-sm text-muted-foreground mb-6 max-w-[360px] mx-auto">
            Register your first subdomain on {DEFAULT_ZONE} to start managing
            DNS records via Nostr events.
          </p>
          <Link href="/">
            <Button>
              <PlusIcon className="size-4" />
              Register Your First Domain
            </Button>
          </Link>
        </div>
      )}
    </div>
  );
}

export default function DashboardPage() {
  return (
    <ErrorBoundary>
      <SiteHeader />
      <main className="px-6 pb-16">
        <Suspense
          fallback={
            <div className="mx-auto max-w-[960px] py-20 text-center text-muted-foreground animate-pulse">
              Loading dashboard...
            </div>
          }
        >
          <DashboardContent />
        </Suspense>
      </main>
      <SiteFooter />
    </ErrorBoundary>
  );
}
