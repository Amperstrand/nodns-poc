import { Card } from "@/components/ui/card";
import type { FilterState } from "@/lib/types";
import { DNS_TYPES } from "@/lib/constants";

interface FilterBarProps {
  filters: FilterState;
  onChange: (filters: FilterState) => void;
  isLive: boolean;
}

export function FilterBar({ filters, onChange, isLive }: FilterBarProps) {
  return (
    <Card className="p-4">
      <div className="flex flex-wrap items-center gap-3">
        <div className="flex items-center gap-2">
          {isLive ? (
            <span className="relative flex h-2.5 w-2.5">
              <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-green-400 opacity-75"></span>
              <span className="relative inline-flex rounded-full h-2.5 w-2.5 bg-green-500"></span>
            </span>
          ) : (
            <span className="relative flex h-2.5 w-2.5">
              <span className="relative inline-flex rounded-full h-2.5 w-2.5 bg-muted-foreground"></span>
            </span>
          )}
          <span className="text-xs font-medium text-muted-foreground">
            {isLive ? "live" : "connecting"}
          </span>
        </div>

        <div className="h-4 w-px bg-border" />

        <input
          type="text"
          placeholder="filter by npub or pubkey..."
          value={filters.npub}
          onChange={(e) => onChange({ ...filters, npub: e.target.value })}
          className="h-8 px-3 text-xs font-mono rounded-md border border-border bg-secondary text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-ring w-48"
        />

        <select
          value={filters.recordType}
          onChange={(e) => onChange({ ...filters, recordType: e.target.value })}
          className="h-8 px-2 text-xs rounded-md border border-border bg-secondary text-foreground focus:outline-none focus:ring-1 focus:ring-ring"
        >
          <option value="all">All Types</option>
          {DNS_TYPES.map((t) => (
            <option key={t} value={t}>{t}</option>
          ))}
        </select>

        <select
          value={filters.kindFilter}
          onChange={(e) => onChange({ ...filters, kindFilter: e.target.value as FilterState["kindFilter"] })}
          className="h-8 px-2 text-xs rounded-md border border-border bg-secondary text-foreground focus:outline-none focus:ring-1 focus:ring-ring"
        >
          <option value="all">All Events</option>
          <option value="records">Records Only</option>
          <option value="zones">Zones Only</option>
        </select>

        <select
          value={filters.paymentFilter}
          onChange={(e) => onChange({ ...filters, paymentFilter: e.target.value as FilterState["paymentFilter"] })}
          className="h-8 px-2 text-xs rounded-md border border-border bg-secondary text-foreground focus:outline-none focus:ring-1 focus:ring-ring"
        >
          <option value="all">All Payments</option>
          <option value="paid">Paid</option>
          <option value="free">Free</option>
          <option value="unpaid">Unpaid</option>
        </select>

        <select
          value={filters.validityFilter}
          onChange={(e) => onChange({ ...filters, validityFilter: e.target.value as FilterState["validityFilter"] })}
          className="h-8 px-2 text-xs rounded-md border border-border bg-secondary text-foreground focus:outline-none focus:ring-1 focus:ring-ring"
        >
          <option value="all">All Validity</option>
          <option value="valid">Valid</option>
          <option value="invalid">Invalid</option>
        </select>
      </div>
    </Card>
  );
}
