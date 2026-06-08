"use client";

import { useState, useEffect, useCallback } from "react";
import { subscribeToDnsEvents } from "@/lib/nostr";
import type { NostrEvent } from "@/lib/types";

interface FeedItem {
  event: NostrEvent;
  relay: string;
}

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

    // Only mark connected when we actually receive an event (handleEvent sets connected=true)
    // Remove the premature 10s timeout that marked connected regardless

    return () => {
      cleanup();
    };
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

  return (
    <section id="live-feed-section" className="px-6 py-16">
      <div className="mx-auto max-w-[960px]">
        <h2 className="mb-6 text-[1.75rem] font-bold tracking-tight">
          Live Event Feed
        </h2>
        <div data-testid="live-feed-entries" className="max-h-[300px] overflow-y-auto rounded-[10px] border border-[#222] bg-[#141414]">
          {!connected ? (
            <div className="py-8 text-center text-sm text-[#666]">
              Connecting to relays...
            </div>
          ) : events.length === 0 ? (
            <div className="py-8 text-center text-sm text-[#666]">
              No events yet. Waiting for kind 11111 events...
            </div>
          ) : (
            events.map(({ event: ev }) => {
              const time = new Date(ev.created_at * 1000).toLocaleTimeString();
              const shortPk = ev.pubkey.slice(0, 12) + "...";
              return (
                <div
                  key={ev.id}
                  className="border-b border-[#222] px-3 py-2 text-sm last:border-b-0"
                >
                  <span className="mr-2 text-[#666]">{time}</span>
                  <strong className="font-mono">{shortPk}</strong>{" "}
                  {getRecordSummary(ev)}
                </div>
              );
            })
          )}
        </div>
      </div>
    </section>
  );
}
