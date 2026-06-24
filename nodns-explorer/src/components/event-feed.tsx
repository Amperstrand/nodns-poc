import { useMemo } from "react";
import { Card } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { EventRow } from "@/components/event-row";
import { FilterBar } from "@/components/filter-bar";
import { LoadingState, EmptyState } from "@/components/empty-state";
import { RECORD_KIND, ZONE_HANDLER_KIND } from "@/lib/constants";
import { parsePayment, checkValidity } from "@/lib/event-analysis";
import type { ExplorerEvent, FilterState } from "@/lib/types";

interface EventFeedProps {
  events: ExplorerEvent[];
  filters: FilterState;
  setFilters: (filters: FilterState) => void;
  isLive: boolean;
  isLoading: boolean;
}

export function EventFeed({
  events,
  filters,
  setFilters,
  isLive,
  isLoading,
}: EventFeedProps) {
  const filteredEvents = useMemo(() => {
    return events.filter((event) => {
      if (filters.npub.trim()) {
        const q = filters.npub.trim().toLowerCase();
        if (
          !event.pubkey.toLowerCase().includes(q) &&
          !event.id.toLowerCase().includes(q)
        ) {
          return false;
        }
      }

      if (filters.kindFilter === "records" && event.kind !== RECORD_KIND) {
        return false;
      }
      if (filters.kindFilter === "zones" && event.kind !== ZONE_HANDLER_KIND) {
        return false;
      }

      if (filters.recordType !== "all") {
        if (event.kind !== RECORD_KIND) return false;
        const hasType = event.tags.some(
          (t) => t[0] === "record" && t[1]?.toUpperCase() === filters.recordType,
        );
        if (!hasType) return false;
      }

      if (filters.paymentFilter !== "all") {
        if (event.kind !== RECORD_KIND) return false;
        const payment = parsePayment(event);
        if (payment.status !== filters.paymentFilter) return false;
      }

      if (filters.validityFilter !== "all") {
        if (event.kind !== RECORD_KIND) return false;
        const validity = checkValidity(event);
        if (filters.validityFilter === "valid" && !validity.valid) return false;
        if (filters.validityFilter === "invalid" && validity.valid) return false;
      }

      return true;
    });
  }, [events, filters]);

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h2 className="text-lg font-semibold text-foreground">
          Event Feed
        </h2>
        <Badge className="border-border text-muted-foreground bg-muted">
          {filteredEvents.length} shown · {events.length} total
        </Badge>
      </div>

      <FilterBar filters={filters} onChange={setFilters} isLive={isLive} />

      {isLoading ? (
        <LoadingState />
      ) : filteredEvents.length === 0 ? (
        <EmptyState message={events.length === 0 ? "No events received yet. Waiting for relay..." : "No events match your filters."} />
      ) : (
        <Card className="divide-y divide-border overflow-hidden">
          {filteredEvents.map((event) => (
            <EventRow key={event.id} event={event} />
          ))}
        </Card>
      )}
    </div>
  );
}
