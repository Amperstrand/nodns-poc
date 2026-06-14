"use client";

import { useState, useEffect } from "react";
import { SiteHeader } from "@/components/site-header";
import { SiteFooter } from "@/components/site-footer";
import { ErrorBoundary } from "@/components/error-boundary";
import { RELAYS, API_BASE } from "@/lib/constants";
import type { DnsRecord, ApiRecordsResponse } from "@/lib/types";
import { SimplePool } from "nostr-tools/pool";
import { decode as nip19Decode } from "nostr-tools/nip19";

const pool = new SimplePool();

interface NostrMetadata {
  name?: string;
  display_name?: string;
  about?: string;
  picture?: string;
  nip05?: string;
}

interface NostrNote {
  id: string;
  created_at: number;
  content: string;
}

function extractNpubFromHost(hostname: string): string | null {
  if (!hostname.endsWith(".nodns.shop")) return null;
  const sub = hostname.replace(".nodns.shop", "");
  if (sub.startsWith("npub1") && sub.length > 10) return sub;
  return null;
}

function npubToHex(npub: string): string | null {
  try {
    const decoded = nip19Decode(npub);
    if (decoded.type === "npub") return decoded.data as string;
    return null;
  } catch {
    return null;
  }
}

function formatTime(utcSeconds: number): string {
  const d = new Date(utcSeconds * 1000);
  const now = new Date();
  const diffMs = now.getTime() - d.getTime();
  const diffMin = Math.floor(diffMs / 60000);
  if (diffMin < 1) return "just now";
  if (diffMin < 60) return `${diffMin}m ago`;
  const diffHr = Math.floor(diffMin / 60);
  if (diffHr < 24) return `${diffHr}h ago`;
  const diffDay = Math.floor(diffHr / 24);
  if (diffDay < 30) return `${diffDay}d ago`;
  return d.toLocaleDateString("en-US", { month: "short", day: "numeric", year: "numeric" });
}

export function NpubProfile() {
  const [records, setRecords] = useState<DnsRecord[]>([]);
  const [npub, setNpub] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [metadata, setMetadata] = useState<NostrMetadata | null>(null);
  const [notes, setNotes] = useState<NostrNote[]>([]);
  const [nostrLoading, setNostrLoading] = useState(false);

  async function fetchNostrEvents(hex: string | null) {
    if (!hex) return;
    setNostrLoading(true);
    try {
      const [metaEvents, noteEvents] = await Promise.all([
        pool.querySync(RELAYS, { kinds: [0], authors: [hex], limit: 1 }),
        pool.querySync(RELAYS, { kinds: [1], authors: [hex], limit: 5 }),
      ]);

      const kind0 = metaEvents.find((e) => e.kind === 0);
      if (kind0) {
        try {
          setMetadata(JSON.parse(kind0.content));
        } catch {}
      }

      const kind1s = noteEvents
        .filter((e) => e.kind === 1)
        .sort((a, b) => b.created_at - a.created_at)
        .slice(0, 5)
        .map((e) => ({
          id: e.id,
          created_at: e.created_at,
          content: e.content,
        }));
      setNotes(kind1s);
    } catch {
    } finally {
      setNostrLoading(false);
    }
  }

  useEffect(() => {
    const hostname = window.location.hostname;
    const extracted = extractNpubFromHost(hostname);
    if (!extracted) {
      setLoading(false);
      return;
    }
    setNpub(extracted);

    const hex = npubToHex(extracted);

    fetch(`${API_BASE}/api/records/by-npub/${extracted}`)
      .then((r) => {
        if (!r.ok) throw new Error(`HTTP ${r.status}`);
        return r.json();
      })
      .then((data: ApiRecordsResponse) => {
        const nodnsRecords = data.records.filter((r) =>
          r.fqdn.includes("nodns.shop")
        );
        setRecords(nodnsRecords);

        const hasRootRecord = nodnsRecords.some(
          (r) => r.name === "" || r.name === "@"
        );
        if (nodnsRecords.length === 0 || !hasRootRecord) {
          fetchNostrEvents(hex);
        }
      })
      .catch((e) => {
        setError(e.message);
        if (hex) fetchNostrEvents(hex);
      })
      .finally(() => setLoading(false));
  }, []);

  if (!npub) return null;

  const grouped = records.reduce<Record<string, DnsRecord[]>>((acc, r) => {
    const key = r.name || "@";
    if (!acc[key]) acc[key] = [];
    acc[key].push(r);
    return acc;
  }, {});

  const shortNpub = `${npub.slice(0, 12)}...${npub.slice(-8)}`;
  const displayName = metadata?.display_name || metadata?.name || shortNpub;
  const hasRootRecord = records.some((r) => r.name === "" || r.name === "@");
  const showNostr = records.length === 0 || !hasRootRecord;

  return (
    <ErrorBoundary>
      <SiteHeader />
      <main className="px-6 pb-20 pt-16">
        <div className="mx-auto max-w-[720px]">
          <div className="mb-10 text-center">
            <div className="mb-4 inline-flex items-center gap-2 rounded-full border border-[#222] bg-[#141414] px-4 py-1.5 text-xs font-mono text-muted-foreground">
              <span className="inline-block h-1.5 w-1.5 rounded-full bg-[#2ecc71] animate-pulse" />
              {records.length > 0 ? "DNS Profile" : "Nostr Profile"}
            </div>

            {metadata?.picture && (
              <img
                src={metadata.picture}
                alt={displayName}
                className="mx-auto mb-4 h-16 w-16 rounded-full border-2 border-[#222]"
              />
            )}

            <h1 className="mb-2 text-3xl font-extrabold tracking-tight">
              <span className="font-mono text-primary">{displayName}</span>
            </h1>
            <p className="text-sm font-mono text-muted-foreground">
              {npub}
            </p>
            {metadata?.nip05 && (
              <p className="mt-1 text-xs text-muted-foreground">
                {metadata.nip05}
              </p>
            )}
            <p className="mt-2 text-xs text-[#666]">
              {npub.slice(0, 12)}...{npub.slice(-8)}.nodns.shop
            </p>
            <a
              href="https://nodns.shop"
              className="mt-3 inline-block text-sm text-primary hover:underline"
            >
              ← nodns.shop
            </a>
          </div>

          {loading && (
            <div className="py-12 text-center text-muted-foreground animate-pulse">
              Loading profile...
            </div>
          )}

          {error && (
            <div className="mb-6 rounded-lg border border-red-900/50 bg-red-950/30 px-4 py-3 text-sm text-red-400">
              Failed to load DNS records: {error}
            </div>
          )}

          {!loading && records.length > 0 && (
            <div className="mb-8">
              <h2 className="mb-4 flex items-center gap-2 text-lg font-bold tracking-tight">
                DNS Records
                <span className="rounded-full bg-primary/10 px-2 py-0.5 text-xs font-mono text-primary">
                  {records.length}
                </span>
              </h2>
              {Object.entries(grouped).map(([name, recs]) => (
                <div
                  key={name}
                  className="mb-3 rounded-lg border border-[#222] bg-[#141414] overflow-hidden"
                >
                  <div className="border-b border-[#222] bg-[#0f0f0f] px-5 py-2.5">
                    <span className="font-mono text-sm font-semibold text-foreground">
                      {name}
                    </span>
                    <span className="ml-2 text-xs text-muted-foreground">
                      {recs.length} record{recs.length !== 1 ? "s" : ""}
                    </span>
                  </div>
                  <div className="divide-y divide-[#222]">
                    {recs.map((r, i) => (
                      <div
                        key={`${r.type}-${r.rdata}-${i}`}
                        className="flex items-center gap-4 px-5 py-2.5 text-sm"
                      >
                        <span className="inline-block min-w-[52px] rounded bg-[#222] px-2 py-0.5 text-center text-xs font-semibold text-primary">
                          {r.type}
                        </span>
                        <span className="flex-1 font-mono text-foreground break-all">
                          {r.rdata}
                        </span>
                        <span className="text-xs text-muted-foreground whitespace-nowrap">
                          {r.ttl}s
                        </span>
                      </div>
                    ))}
                  </div>
                </div>
              ))}
              <div className="text-center text-xs text-[#666]">
                from kind 11111 events ·{" "}
                <a
                  href={`${API_BASE}/api/records/by-npub/${npub}`}
                  className="text-primary hover:underline"
                  target="_blank"
                  rel="noopener noreferrer"
                >
                  API
                </a>
              </div>
            </div>
          )}

          {showNostr && (
            <div className="mb-8">
              <h2 className="mb-4 flex items-center gap-2 text-lg font-bold tracking-tight">
                Recent Nostr Activity
                <span className="rounded-full bg-[#222] px-2 py-0.5 text-xs font-mono text-muted-foreground">
                  verified
                </span>
              </h2>

              {nostrLoading && (
                <div className="py-8 text-center text-sm text-muted-foreground animate-pulse">
                  Fetching from relays...
                </div>
              )}

              {!nostrLoading && metadata && (
                <div className="mb-4 rounded-lg border border-[#222] bg-[#141414] p-5">
                  <div className="flex items-start gap-4">
                    {metadata.picture && (
                      <img
                        src={metadata.picture}
                        alt=""
                        className="h-10 w-10 rounded-full border border-[#222]"
                      />
                    )}
                    <div className="min-w-0 flex-1">
                      {(metadata.display_name || metadata.name) && (
                        <p className="font-semibold text-foreground truncate">
                          {metadata.display_name || metadata.name}
                        </p>
                      )}
                      {metadata.about && (
                        <p className="mt-1 text-sm text-[#bbb] line-clamp-3">
                          {metadata.about}
                        </p>
                      )}
                    </div>
                  </div>
                  <div className="mt-3 flex items-center gap-2 text-xs text-[#666]">
                    <span className="rounded bg-[#222] px-1.5 py-0.5 font-mono">kind 0</span>
                    metadata event
                  </div>
                </div>
              )}

              {!nostrLoading && notes.length > 0 && (
                <div className="space-y-3">
                  {notes.map((note) => (
                    <div
                      key={note.id}
                      className="rounded-lg border border-[#222] bg-[#141414] p-4"
                    >
                      <p className="text-sm text-foreground whitespace-pre-wrap break-words line-clamp-6">
                        {note.content}
                      </p>
                      <div className="mt-3 flex items-center justify-between text-xs text-[#666]">
                        <span className="rounded bg-[#222] px-1.5 py-0.5 font-mono">kind 1</span>
                        <span>{formatTime(note.created_at)}</span>
                      </div>
                    </div>
                  ))}
                </div>
              )}

              {!nostrLoading && !metadata && notes.length === 0 && (
                <div className="rounded-lg border border-[#222] bg-[#141414] px-6 py-10 text-center">
                  <p className="text-muted-foreground">
                    No recent Nostr activity found for this npub.
                  </p>
                </div>
              )}

              {!nostrLoading && (metadata || notes.length > 0) && (
                <div className="mt-4 text-center text-xs text-[#666]">
                  from {RELAYS.length} relays · all events cryptographically signed
                </div>
              )}
            </div>
          )}

          {!loading && records.length === 0 && (
            <div className="mb-8 rounded-lg border border-primary/20 bg-primary/5 p-5 text-center">
              <p className="mb-1 text-sm font-semibold text-primary">
                No DNS records yet
              </p>
              <p className="text-sm text-[#bbb]">
                This npub has not published any kind 11111 events.{" "}
                <a
                  href="https://nodns.shop/dashboard"
                  className="text-primary hover:underline"
                >
                  Get started →
                </a>
              </p>
            </div>
          )}

          <div className="mt-8 rounded-lg border border-[#222] bg-[#141414] p-6">
            <h2 className="mb-3 text-sm font-semibold text-foreground">
              What am I looking at?
            </h2>
            <p className="text-sm text-[#bbb]">
              This page shows data published as Nostr events by the owner of
              this npub. DNS records come from{" "}
              <span className="font-mono text-primary">kind 11111</span>{" "}
              events. The recent activity section shows{" "}
              <span className="font-mono text-primary">kind 0</span> (profile)
              and <span className="font-mono text-primary">kind 1</span> (note)
              events. All content is cryptographically signed — anyone can
              verify authenticity.
            </p>
          </div>
        </div>
      </main>
      <SiteFooter />
    </ErrorBoundary>
  );
}
