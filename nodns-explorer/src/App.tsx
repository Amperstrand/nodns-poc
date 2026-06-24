import { useState, useEffect, useRef } from 'react';
import { SiteHeader } from '@/components/site-header';
import { ZoneCard } from '@/components/zone-card';
import { EventFeed } from '@/components/event-feed';
import { ZoneMonitor } from '@/components/zone-monitor';
import { TabBar, type MonitorTab } from '@/components/tab-bar';
import { ZoneLoadingState } from '@/components/empty-state';
import { Badge } from '@/components/ui/badge';
import { discoverZones } from '@/lib/zones';
import { subscribeToEvents } from '@/lib/nostr';
import { RECORD_KIND, ZONE_HANDLER_KIND } from '@/lib/constants';
import type { ExplorerEvent, ZoneStatus, FilterState } from '@/lib/types';

export function App() {
  const [tab, setTab] = useState<MonitorTab>("feed");
  const [zones, setZones] = useState<ZoneStatus[]>([]);
  const [zonesLoading, setZonesLoading] = useState(true);
  const [events, setEvents] = useState<ExplorerEvent[]>([]);
  const [isLive, setIsLive] = useState(false);
  const [isLoading, setIsLoading] = useState(true);
  const [filters, setFilters] = useState<FilterState>({
    npub: "",
    recordType: "all",
    kindFilter: "all",
    paymentFilter: "all",
    validityFilter: "all",
  });
  const seenRef = useRef<Set<string>>(new Set());

  useEffect(() => {
    let cancelled = false;
    setZonesLoading(true);
    discoverZones()
      .then((result) => {
        if (!cancelled) {
          setZones(result);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setZones([]);
        }
      })
      .finally(() => {
        if (!cancelled) {
          setZonesLoading(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    setIsLive(false);
    const unsubscribe = subscribeToEvents(
      (nostrEvent) => {
        if (seenRef.current.has(nostrEvent.id)) return;
        if (nostrEvent.kind !== RECORD_KIND && nostrEvent.kind !== ZONE_HANDLER_KIND) return;
        seenRef.current.add(nostrEvent.id);
        const explorerEvent: ExplorerEvent = {
          id: nostrEvent.id,
          kind: nostrEvent.kind,
          pubkey: nostrEvent.pubkey,
          created_at: nostrEvent.created_at,
          content: nostrEvent.content,
          tags: nostrEvent.tags,
          raw: nostrEvent,
        };
        setEvents((prev) => [explorerEvent, ...prev].slice(0, 500));
      },
      () => {
        setIsLive(true);
      },
    );
    const timer = setTimeout(() => setIsLive(true), 3000);
    return () => {
      unsubscribe();
      clearTimeout(timer);
    };
  }, [setEvents]);

  useEffect(() => {
    const timer = setTimeout(() => setIsLoading(false), 2500);
    return () => clearTimeout(timer);
  }, []);

  return (
    <div className="antialiased min-h-screen">
      <SiteHeader />
      <TabBar tab={tab} setTab={setTab} />
      <main className="mx-auto max-w-5xl px-4 py-8 space-y-8">
        {tab === "feed" ? (
          <>
            <section className="space-y-4">
              <div className="flex items-center justify-between">
                <h2 className="text-lg font-semibold text-foreground">
                  Zones
                </h2>
                <Badge className="border-border text-muted-foreground bg-muted">
                  {zones.length} discovered
                </Badge>
              </div>
              {zonesLoading ? (
                <ZoneLoadingState />
              ) : zones.length === 0 ? (
                <div className="rounded-lg border border-border bg-card p-8 text-center">
                  <p className="text-sm text-muted-foreground">
                    No zones discovered from relay.cashu.email
                  </p>
                </div>
              ) : (
                <div className="grid gap-3 sm:grid-cols-2">
                  {zones.map((zone) => (
                    <ZoneCard key={zone.zone} zone={zone} />
                  ))}
                </div>
              )}
            </section>

            <EventFeed
              events={events}
              filters={filters}
              setFilters={setFilters}
              isLive={isLive}
              isLoading={isLoading}
            />
          </>
        ) : (
          <ZoneMonitor events={events} zones={zones} isLive={isLive} />
        )}
      </main>
    </div>
  );
}
