"use client";

import { useState, useEffect, useCallback } from "react";
import Link from "next/link";
import { subscribeToDnsEvents } from "@/lib/nostr";
import { countLeadingZeroBits, DEFAULT_POW_DIFFICULTY } from "@/lib/constants";
import type { NostrEvent } from "@/lib/types";

interface FeedItem {
  event: NostrEvent;
  relay: string;
}

const MAX_VISIBLE = 5;

export function LiveFeed() {
  const [events, setEvents] = useState<FeedItem[]>([]);
  const [connected, setConnected] = useState(false);

  const handleEvent = useCallback((event: NostrEvent, relay: string) => {
    setConnected(true);
    setEvents((prev) => {
      if (prev.some((e) => e.event.id === event.id)) return prev;
      const updated = [{ event, relay }, ...prev].slice(0, 50);
      return updated;
    });
  }, []);

  useEffect(() => {
    const cleanup = subscribeToDnsEvents(handleEvent);
    return () => cleanup();
  }, [handleEvent]);

  const getRecordSummary = (event: NostrEvent): string => {
    const recordTags = event.tags.filter((t) => t[0] === "record");
    return recordTags
      .map((t) => {
        const rtype = t[1];
        const name = t[2] || "@";
        const rdata = t[3];
        return `${rtype} ${name} → ${rdata}`;
      })
      .join(", ");
  };

  const visible = events.slice(0, MAX_VISIBLE);

  return (
    <section className="px-6 py-16">
      <div className="mx-auto max-w-[960px]">
        <div className="flex items-center justify-between mb-4">
          <div className="flex items-center gap-2">
            <h2 className="text-[1.75rem] font-bold tracking-tight">
              Live Event Feed
            </h2>
            {connected && (
              <span className="inline-block h-2 w-2 rounded-full bg-emerald-400 animate-pulse" />
            )}
          </div>
          <Link
            href="/records"
            className="text-sm text-muted-foreground hover:text-foreground transition-colors px-2 py-1 rounded-md hover:bg-muted/30"
          >
            See more →
          </Link>
        </div>
        <div className="max-h-[220px] overflow-y-auto rounded-xl bg-card ring-1 ring-foreground/10">
          {!connected ? (
            <div className="py-8 text-center text-sm text-muted-foreground">
              Connecting to relays...
            </div>
          ) : events.length === 0 ? (
            <div className="py-8 text-center text-sm text-muted-foreground">
              No events yet. Waiting for kind 11111 events...
            </div>
          ) : (
            visible.map(({ event: ev }) => {
              const time = new Date(ev.created_at * 1000).toLocaleTimeString();
              const shortPk = ev.pubkey.slice(0, 12) + "...";
              const pow = countLeadingZeroBits(ev.id);
              const powPasses = pow >= DEFAULT_POW_DIFFICULTY;
              return (
                <div
                  key={ev.id}
                  className="flex items-center gap-2 sm:gap-3 border-b border-border px-4 py-3.5 text-sm last:border-b-0"
                >
                  <span className="shrink-0 font-mono text-foreground/70">{time}</span>
                  <span className="shrink-0 font-mono font-medium text-foreground">{shortPk}</span>
                  <span
                    className={`shrink-0 font-mono text-xs px-1.5 py-0.5 rounded font-semibold ${
                      powPasses
                        ? "bg-emerald-500/10 text-emerald-400"
                        : "bg-red-500/10 text-red-400"
                    }`}
                    title={`PoW difficulty: ${pow} bits (min ${DEFAULT_POW_DIFFICULTY})`}
                  >
                    ⚡{pow}
                  </span>
                  <span className="min-w-0 truncate text-foreground/70">{getRecordSummary(ev)}</span>
                </div>
              );
            })
          )}
        </div>
      </div>
    </section>
  );
}
