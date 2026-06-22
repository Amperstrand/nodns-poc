"use client";

import { useState, useEffect, useCallback, useMemo } from "react";
import Link from "next/link";
import { useIdentity } from "@/contexts/IdentityContext";
import { useWallet } from "@/contexts/WalletContext";
import { fetchRecords } from "@/lib/api";
import { subscribeToRecords } from "@/lib/nostr";
import { DEFAULT_ZONE } from "@/lib/constants";
import type { DnsRecord, DomainInfo } from "@/lib/types";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  CardDescription,
} from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";

function timeAgo(ts: number): string {
  if (!ts) return "never";
  const seconds = Math.floor(Date.now() / 1000 - ts);
  if (seconds < 0) return "just now";
  if (seconds < 60) return `${seconds}s ago`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `${days}d ago`;
  const months = Math.floor(days / 30);
  return `${months}mo ago`;
}

function truncateNpub(npub: string | null): string {
  if (!npub) return "npub";
  if (npub.length <= 20) return npub;
  return `${npub.slice(0, 12)}...${npub.slice(-6)}`;
}

function groupRecordsByDomain(
  records: DnsRecord[],
  npub: string | null,
): DomainInfo[] {
  const groups = new Map<string, DnsRecord[]>();

  for (const record of records) {
    if (record.deleted) continue;
    const name = record.name || "";
    const zone = record.zone || DEFAULT_ZONE;
    const key = `${name}|${zone}`;
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key)!.push(record);
  }

  const domains: DomainInfo[] = [];

  for (const [key, recs] of groups) {
    const [name, zone] = key.split("|");
    const label = name || truncateNpub(npub);
    const fqdn = `${label}.${zone}`;
    const lastSeen = recs.reduce(
      (max, r) => Math.max(max, r.created_at || 0),
      0,
    );

    domains.push({
      fqdn,
      name,
      zone,
      recordCount: recs.length,
      records: recs,
      lastSeen,
      status: "active",
    });
  }

  return domains.sort((a, b) => b.lastSeen - a.lastSeen);
}

function StatusBadge({ status }: { status: DomainInfo["status"] }) {
  if (status === "active") {
    return (
      <Badge className="border-transparent bg-emerald-500/15 text-emerald-400">
        Active
      </Badge>
    );
  }
  if (status === "grace") {
    return (
      <Badge className="border-transparent bg-amber-500/15 text-amber-400">
        Grace
      </Badge>
    );
  }
  return (
    <Badge className="border-transparent bg-destructive/15 text-destructive">
      Expired
    </Badge>
  );
}

export default function DashboardPage() {
  const { session, npub, loading, loginWithEphemeral } = useIdentity();
  const { balance, ready } = useWallet();

  const [records, setRecords] = useState<DnsRecord[]>([]);
  const [fetching, setFetching] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [live, setLive] = useState(false);

  const loadRecords = useCallback(async (pubkey: string) => {
    setFetching(true);
    setError(null);
    try {
      const data = await fetchRecords(pubkey);
      setRecords(data);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to load records");
    } finally {
      setFetching(false);
    }
  }, []);

  useEffect(() => {
    if (!session?.pubkey) return;

    loadRecords(session.pubkey);

    let debounce: ReturnType<typeof setTimeout> | null = null;
    const unsub = subscribeToRecords(session.pubkey, () => {
      setLive(true);
      if (debounce) clearTimeout(debounce);
      debounce = setTimeout(() => {
        loadRecords(session.pubkey);
        setTimeout(() => setLive(false), 2000);
        debounce = null;
      }, 3000);
    });

    return () => {
      if (debounce) clearTimeout(debounce);
      unsub();
    };
  }, [session?.pubkey, loadRecords]);

  const domains = useMemo(
    () => groupRecordsByDomain(records, npub),
    [records, npub],
  );

  const totalRecords = useMemo(
    () => domains.reduce((sum, d) => sum + d.recordCount, 0),
    [domains],
  );

  if (loading) {
    return (
      <div className="flex min-h-[60vh] items-center justify-center">
        <div className="text-muted-foreground">Loading...</div>
      </div>
    );
  }

  if (!session) {
    return (
      <div className="flex min-h-[60vh] items-center justify-center">
        <Card className="max-w-md w-full">
          <CardContent className="pt-8 pb-8 text-center space-y-4">
            <h2 className="text-xl font-semibold tracking-tight">
              Sign in to view your dashboard
            </h2>
            <p className="text-sm text-muted-foreground">
              Create an ephemeral identity or connect your Nostr key to manage
              your DNS records.
            </p>
            <Button onClick={loginWithEphemeral} size="lg">
              Generate Identity
            </Button>
          </CardContent>
        </Card>
      </div>
    );
  }

  return (
    <div className="space-y-8">
      <div className="flex items-start justify-between gap-4 flex-wrap">
        <div className="space-y-1">
          <div className="flex items-center gap-3">
            <h1 className="text-2xl font-bold tracking-tight">Dashboard</h1>
            {live && (
              <span className="flex items-center gap-1.5 text-xs text-primary">
                <span className="h-1.5 w-1.5 rounded-full bg-primary animate-live-pulse" />
                Live
              </span>
            )}
          </div>
          <p className="text-sm text-muted-foreground">
            Manage your decentralized DNS records
          </p>
        </div>
        <Link href="/">
          <Button>Register New Domain</Button>
        </Link>
      </div>

      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
        <Card className="border-primary/20">
          <CardHeader className="pb-2">
            <CardDescription>Wallet Balance</CardDescription>
            <CardTitle className="text-2xl">
              <span className="text-primary">{ready ? balance : "..."}</span>
              <span className="text-sm font-normal text-muted-foreground ml-1.5">
                sats
              </span>
            </CardTitle>
          </CardHeader>
          <CardContent className="pt-0">
            <Link
              href="/wallet"
              className="text-xs text-primary hover:underline"
            >
              Manage wallet
            </Link>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="pb-2">
            <CardDescription>Domains</CardDescription>
            <CardTitle className="text-2xl">
              {fetching && domains.length === 0 ? "..." : domains.length}
            </CardTitle>
          </CardHeader>
          <CardContent className="pt-0">
            <span className="text-xs text-muted-foreground">
              {domains.length === 1 ? "domain registered" : "domains registered"}
            </span>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="pb-2">
            <CardDescription>Total Records</CardDescription>
            <CardTitle className="text-2xl">
              {fetching && records.length === 0 ? "..." : totalRecords}
            </CardTitle>
          </CardHeader>
          <CardContent className="pt-0">
            <span className="text-xs text-muted-foreground">
              across all domains
            </span>
          </CardContent>
        </Card>
      </div>

      <div className="space-y-3">
        <div className="flex items-center justify-between">
          <h2 className="text-lg font-semibold tracking-tight">Your Domains</h2>
          {fetching && domains.length > 0 && (
            <span className="text-xs text-muted-foreground">Updating...</span>
          )}
        </div>

        {error && (
          <Card className="border-destructive/30">
            <CardContent className="py-4 text-sm text-destructive">
              {error}
            </CardContent>
          </Card>
        )}

        {!error && !fetching && domains.length === 0 && (
          <Card className="border-dashed">
            <CardContent className="py-16 text-center space-y-3">
              <p className="text-muted-foreground">
                No domains yet. Register your first name.
              </p>
              <Link href="/">
                <Button variant="outline">Register a Domain</Button>
              </Link>
            </CardContent>
          </Card>
        )}

        <div className="grid gap-3 md:grid-cols-2">
          {domains.map((domain) => {
            const recordTypes = [
              ...new Set(domain.records.map((r) => r.record_type)),
            ].sort();

            return (
              <Card
                key={`${domain.name}|${domain.zone}`}
                className="group transition-colors hover:border-primary/40"
              >
                <CardContent className="pt-5 pb-5 space-y-3">
                  <div className="flex items-start justify-between gap-2">
                    <Link
                      href={{
                        pathname: "/domain",
                        query: { name: domain.name, zone: domain.zone },
                      }}
                      className="font-mono text-sm hover:text-primary transition-colors break-all"
                    >
                      {domain.fqdn}
                    </Link>
                    <StatusBadge status={domain.status} />
                  </div>

                  <div className="flex flex-wrap gap-1.5">
                    {recordTypes.map((type) => (
                      <Badge
                        key={type}
                        className="font-mono text-[10px] text-muted-foreground"
                      >
                        {type}
                      </Badge>
                    ))}
                  </div>

                  <div className="flex items-center justify-between pt-1">
                    <div className="flex items-center gap-3 text-xs text-muted-foreground">
                      <span>{domain.recordCount} records</span>
                      <span>{timeAgo(domain.lastSeen)}</span>
                    </div>
                    <Link
                      href={{
                        pathname: "/domain",
                        query: { name: domain.name, zone: domain.zone },
                      }}
                    >
                      <Button variant="ghost" size="sm" className="opacity-70 group-hover:opacity-100">
                        Manage
                      </Button>
                    </Link>
                  </div>
                </CardContent>
              </Card>
            );
          })}
        </div>
      </div>
    </div>
  );
}
