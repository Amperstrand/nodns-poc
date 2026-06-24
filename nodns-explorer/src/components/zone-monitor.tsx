import { useState, useMemo, useCallback, useEffect, useRef } from "react";
import { Card, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { truncateMid } from "@/lib/format";
import { parseRecords } from "@/lib/event-analysis";
import { queryZoneRecords } from "@/lib/dns-lookup";
import { generateZoneFile, downloadZoneFile } from "@/lib/zone-file";
import type { ZoneRecord } from "@/lib/zone-file";
import { RECORD_KIND, BOT_API_BASE, DEFAULT_ZONE } from "@/lib/constants";
import type { ExplorerEvent, ZoneStatus } from "@/lib/types";

interface ZoneMonitorProps {
  events: ExplorerEvent[];
  zones: ZoneStatus[];
  isLive: boolean;
}

type DiscrepancyType =
  | "ONLY_IN_RELAY"
  | "ONLY_IN_API"
  | "DNS_MISSING"
  | "DNS_STALE";

interface Discrepancy {
  type: DiscrepancyType;
  record: ZoneRecord;
}

interface Conflict {
  name: string;
  type: string;
  relay: string[];
  api: string[];
  dns: string[];
}

interface ComparisonResult {
  discrepancies: Discrepancy[];
  conflicts: Conflict[];
  stats: {
    match: number;
    onlyRelay: number;
    onlyApi: number;
    dnsMissing: number;
    dnsStale: number;
    conflicts: number;
  };
}

interface ApiRecord {
  npub: string;
  name: string;
  fqdn: string;
  type: string;
  ttl: number;
  rdata: string;
  created_at: number;
}

interface ApiResponse {
  records: ApiRecord[];
  count: number;
}

type FetchState = "idle" | "loading" | "ok" | "error";

interface SourceState {
  records: ZoneRecord[];
  state: FetchState;
  error?: string;
}

function normalizeRdata(rdata: string): string {
  return rdata.toLowerCase().replace(/"/g, "").trim();
}

function recordKey(r: ZoneRecord): string {
  return `${r.name.toLowerCase()}|${r.type.toUpperCase()}|${normalizeRdata(r.rdata)}`;
}

function nameTypeKey(name: string, type: string): string {
  return `${name.toLowerCase()}|${type.toUpperCase()}`;
}

function parseRelayRecords(events: ExplorerEvent[], zone: string): ZoneRecord[] {
  const latest = new Map<string, ZoneRecord>();
  for (const event of events) {
    if (event.kind !== RECORD_KIND) continue;
    const parsed = parseRecords(event);
    for (const rec of parsed) {
      if (!rec.fqdn.toLowerCase().endsWith(`.${zone}`.toLowerCase())) continue;
      const key = nameTypeKey(rec.fqdn, rec.type);
      const existing = latest.get(key);
      if (!existing || event.created_at > existing.created_at) {
        latest.set(key, {
          name: rec.fqdn,
          type: rec.type,
          ttl: rec.ttl,
          rdata: rec.rdata,
          npub: event.pubkey,
          event_id: event.id,
          created_at: event.created_at,
        });
      }
    }
  }
  return [...latest.values()].filter((r) => r.rdata.length > 0);
}

async function fetchApiRecords(zone: string): Promise<ZoneRecord[]> {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 15_000);
  try {
    const res = await fetch(`${BOT_API_BASE}/api/records`, {
      signal: controller.signal,
      headers: { accept: "application/json" },
    });
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const data: unknown = await res.json();
    if (typeof data !== "object" || data === null || !("records" in data)) {
      throw new Error("invalid response");
    }
    const response = data as ApiResponse;
    return response.records
      .filter((r) => r.fqdn.toLowerCase().endsWith(`.${zone}`.toLowerCase()))
      .map<ZoneRecord>((r) => ({
        name: r.fqdn,
        type: r.type,
        ttl: r.ttl,
        rdata: r.rdata,
        npub: r.npub,
        event_id: "",
        created_at: r.created_at,
      }));
  } finally {
    clearTimeout(timeout);
  }
}

async function fetchDnsRecords(
  zone: string,
  relayRecords: ZoneRecord[],
  apiRecords: ZoneRecord[],
): Promise<ZoneRecord[]> {
  const fqdns = new Set<string>([zone.toLowerCase()]);
  for (const r of [...relayRecords, ...apiRecords]) {
    fqdns.add(r.name.toLowerCase());
  }
  const fqdnList = [...fqdns].slice(0, 30);
  const results = await Promise.all(fqdnList.map((fqdn) => queryZoneRecords(fqdn)));
  return results.flat();
}

function compareRecords(
  relay: ZoneRecord[],
  api: ZoneRecord[],
  dns: ZoneRecord[],
): ComparisonResult {
  const relayMap = new Map<string, ZoneRecord>();
  const apiMap = new Map<string, ZoneRecord>();
  const dnsMap = new Map<string, ZoneRecord>();
  for (const r of relay) relayMap.set(recordKey(r), r);
  for (const r of api) apiMap.set(recordKey(r), r);
  for (const r of dns) dnsMap.set(recordKey(r), r);

  const allKeys = new Set<string>([
    ...relayMap.keys(),
    ...apiMap.keys(),
    ...dnsMap.keys(),
  ]);

  const discrepancies: Discrepancy[] = [];
  let match = 0;

  for (const key of allKeys) {
    const inRelay = relayMap.has(key);
    const inApi = apiMap.has(key);
    const inDns = dnsMap.has(key);

    if (inRelay && inApi && inDns) {
      match++;
      continue;
    }
    if (!inRelay && !inApi && !inDns) continue;

    const record = relayMap.get(key) ?? apiMap.get(key) ?? dnsMap.get(key)!;

    if (inRelay && !inApi) {
      discrepancies.push({ type: "ONLY_IN_RELAY", record });
    } else if (!inRelay && inApi) {
      discrepancies.push({ type: "ONLY_IN_API", record });
    } else if (inApi && !inDns) {
      discrepancies.push({ type: "DNS_MISSING", record });
    } else if (!inApi && inDns) {
      discrepancies.push({ type: "DNS_STALE", record });
    }
  }

  const groups = new Map<
    string,
    { relay: Set<string>; api: Set<string>; dns: Set<string> }
  >();
  const addToGroup = (
    map: Map<string, { relay: Set<string>; api: Set<string>; dns: Set<string> }>,
    r: ZoneRecord,
    source: "relay" | "api" | "dns",
  ) => {
    const k = nameTypeKey(r.name, r.type);
    if (!map.has(k)) map.set(k, { relay: new Set(), api: new Set(), dns: new Set() });
    map.get(k)![source].add(normalizeRdata(r.rdata));
  };
  for (const r of relay) addToGroup(groups, r, "relay");
  for (const r of api) addToGroup(groups, r, "api");
  for (const r of dns) addToGroup(groups, r, "dns");

  const conflicts: Conflict[] = [];
  for (const [k, g] of groups) {
    const sourcesPresent =
      (g.relay.size > 0 ? 1 : 0) + (g.api.size > 0 ? 1 : 0) + (g.dns.size > 0 ? 1 : 0);
    if (sourcesPresent < 2) continue;
    const allRdatas = new Set([...g.relay, ...g.api, ...g.dns]);
    if (allRdatas.size <= 1) continue;
    let hasMismatch = false;
    const relayArr = [...g.relay];
    const apiArr = [...g.api];
    const dnsArr = [...g.dns];
    if (relayArr.length > 0 && apiArr.length > 0) {
      hasMismatch = hasMismatch || !relayArr.every((r) => apiArr.includes(r));
    }
    if (apiArr.length > 0 && dnsArr.length > 0) {
      hasMismatch = hasMismatch || !apiArr.every((r) => dnsArr.includes(r));
    }
    if (relayArr.length > 0 && dnsArr.length > 0) {
      hasMismatch = hasMismatch || !relayArr.every((r) => dnsArr.includes(r));
    }
    if (hasMismatch) {
      const [name, type] = k.split("|");
      conflicts.push({
        name,
        type,
        relay: relayArr,
        api: apiArr,
        dns: dnsArr,
      });
    }
  }

  return {
    discrepancies,
    conflicts,
    stats: {
      match,
      onlyRelay: discrepancies.filter((d) => d.type === "ONLY_IN_RELAY").length,
      onlyApi: discrepancies.filter((d) => d.type === "ONLY_IN_API").length,
      dnsMissing: discrepancies.filter((d) => d.type === "DNS_MISSING").length,
      dnsStale: discrepancies.filter((d) => d.type === "DNS_STALE").length,
      conflicts: conflicts.length,
    },
  };
}

const DISCREPANCY_META: Record<
  DiscrepancyType,
  { label: string; badge: string; icon: string }
> = {
  ONLY_IN_RELAY: {
    label: "ONLY_IN_RELAY",
    badge: "border-amber-500/30 text-amber-400 bg-amber-500/10",
    icon: "\u26A0",
  },
  ONLY_IN_API: {
    label: "ONLY_IN_API",
    badge: "border-purple-500/30 text-purple-400 bg-purple-500/10",
    icon: "\u26A0",
  },
  DNS_MISSING: {
    label: "DNS_MISSING",
    badge: "border-red-500/30 text-red-400 bg-red-500/10",
    icon: "\u26A0",
  },
  DNS_STALE: {
    label: "DNS_STALE",
    badge: "border-cyan-500/30 text-cyan-400 bg-cyan-500/10",
    icon: "\u26A0",
  },
};

function SourceCard({
  title,
  records,
  state,
  error,
}: {
  title: string;
  records: ZoneRecord[];
  state: FetchState;
  error?: string;
}) {
  const statusText =
    state === "loading"
      ? "fetching..."
      : state === "error"
        ? error ?? "failed"
        : state === "ok"
          ? records.length > 0
            ? "\u2713 ready"
            : "\u2713 empty"
          : "waiting";
  const statusColor =
    state === "error"
      ? "border-red-500/30 text-red-400 bg-red-500/10"
      : state === "ok"
        ? "border-green-500/30 text-green-400 bg-green-500/10"
        : "border-border text-muted-foreground bg-muted";

  return (
    <Card className="overflow-hidden">
      <CardContent className="p-4 space-y-2">
        <div className="flex items-center justify-between">
          <span className="text-sm font-semibold text-foreground">{title}</span>
          <Badge className={statusColor}>{statusText}</Badge>
        </div>
        <div className="text-2xl font-bold font-mono text-primary">
          {state === "loading" ? "\u2014" : records.length}
        </div>
        <p className="text-xs text-muted-foreground">records</p>
      </CardContent>
    </Card>
  );
}

function ConformanceBar({ result }: { result: ComparisonResult }) {
  const s = result.stats;
  const totalIssues =
    s.onlyRelay + s.onlyApi + s.dnsMissing + s.dnsStale + s.conflicts;
  const levelColor =
    totalIssues === 0
      ? "border-green-500/30 text-green-400 bg-green-500/10"
      : "border-amber-500/30 text-amber-400 bg-amber-500/10";

  return (
    <Card className="p-4 space-y-2">
      <div className="flex items-center gap-2">
        <span className="text-sm font-semibold text-foreground">Conformance</span>
        <Badge className={levelColor}>
          {totalIssues === 0 ? "\u2713 FULL" : "\u26A0 PARTIAL"}
        </Badge>
      </div>
      <div className="space-y-1 text-xs text-muted-foreground">
        {s.match > 0 && (
          <div>
            <span className="text-green-400">{s.match}</span> records match across all sources
          </div>
        )}
        {s.onlyRelay > 0 && (
          <div>
            <span className="text-amber-400">{s.onlyRelay}</span> only in relay (not processed by bot)
          </div>
        )}
        {s.onlyApi > 0 && (
          <div>
            <span className="text-purple-400">{s.onlyApi}</span> in API but deleted from relay
          </div>
        )}
        {s.dnsMissing > 0 && (
          <div>
            <span className="text-red-400">{s.dnsMissing}</span> in API but missing from DNS
          </div>
        )}
        {s.dnsStale > 0 && (
          <div>
            <span className="text-cyan-400">{s.dnsStale}</span> orphaned in DNS (not in API)
          </div>
        )}
        {s.conflicts > 0 && (
          <div>
            <span className="text-red-400">{s.conflicts}</span> conflicts (same record, different data)
          </div>
        )}
        {totalIssues === 0 && (
          <div className="text-green-400">All sources in agreement</div>
        )}
      </div>
    </Card>
  );
}

function DiscrepancyRow({ discrepancy }: { discrepancy: Discrepancy }) {
  const meta = DISCREPANCY_META[discrepancy.type];
  const r = discrepancy.record;
  const rdataDisplay = r.type === "TXT" ? `"${r.rdata}"` : r.rdata;

  return (
    <div className="flex items-center gap-2 px-4 py-2 font-mono text-xs flex-wrap border-b border-border last:border-b-0">
      <Badge className={meta.badge}>{meta.icon} {meta.label}</Badge>
      <span className="text-foreground/90 break-all flex-1 min-w-0">
        {truncateMid(r.name, 40)}
      </span>
      <span className="text-muted-foreground uppercase">{r.type}</span>
      <span className="text-primary break-all">{rdataDisplay}</span>
      {r.event_id && (
        <span className="text-muted-foreground text-[10px]">
          ({r.event_id.slice(0, 10)})
        </span>
      )}
    </div>
  );
}

function ConflictRow({ conflict }: { conflict: Conflict }) {
  const renderValues = (label: string, values: string[], color: string) => {
    if (values.length === 0) return null;
    return (
      <span className={`font-mono text-[11px] ${color}`}>
        {label}: {values.join(", ")}
      </span>
    );
  };
  return (
    <div className="px-4 py-2 space-y-1 border-b border-border last:border-b-0">
      <div className="flex items-center gap-2 font-mono text-xs">
        <Badge className="border-red-500/30 text-red-400 bg-red-500/10">
          CONFLICT
        </Badge>
        <span className="text-foreground/90 break-all flex-1 min-w-0">
          {truncateMid(conflict.name, 40)}
        </span>
        <span className="text-muted-foreground uppercase">{conflict.type}</span>
      </div>
      <div className="flex items-center gap-3 flex-wrap pl-2">
        {renderValues("relay", conflict.relay, "text-blue-400")}
        {renderValues("api", conflict.api, "text-purple-400")}
        {renderValues("dns", conflict.dns, "text-cyan-400")}
      </div>
    </div>
  );
}

export function ZoneMonitor({ events, zones, isLive }: ZoneMonitorProps) {
  const zoneOptions = useMemo(() => {
    const discovered = zones.map((z) => z.zone);
    return [...new Set([DEFAULT_ZONE, ...discovered])];
  }, [zones]);

  const [selectedZone, setSelectedZone] = useState(DEFAULT_ZONE);
  const [apiState, setApiState] = useState<SourceState>({ records: [], state: "idle" });
  const [dnsState, setDnsState] = useState<SourceState>({ records: [], state: "idle" });

  const relayRecords = useMemo(
    () => parseRelayRecords(events, selectedZone),
    [events, selectedZone],
  );

  const relayRecordsRef = useRef<ZoneRecord[]>([]);
  relayRecordsRef.current = relayRecords;

  const refresh = useCallback(async () => {
    setApiState({ records: [], state: "loading" });
    setDnsState({ records: [], state: "loading" });

    const apiResult = await fetchApiRecords(selectedZone).then(
      (records) => ({ records, state: "ok" as const }),
      (err: unknown) => ({
        records: [],
        state: "error" as const,
        error: err instanceof Error ? err.message : "fetch failed",
      }),
    );
    setApiState(apiResult);

    const dnsResult = await fetchDnsRecords(
      selectedZone,
      relayRecordsRef.current,
      apiResult.records,
    ).then(
      (records) => ({ records, state: "ok" as const }),
      (err: unknown) => ({
        records: [],
        state: "error" as const,
        error: err instanceof Error ? err.message : "dns query failed",
      }),
    );
    setDnsState(dnsResult);
  }, [selectedZone]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const comparison = useMemo(
    () => compareRecords(relayRecords, apiState.records, dnsState.records),
    [relayRecords, apiState.records, dnsState.records],
  );

  const zoneFile = useMemo(
    () => generateZoneFile(apiState.records.length > 0 ? apiState.records : relayRecords, selectedZone),
    [apiState.records, relayRecords, selectedZone],
  );

  const handleDownload = () => {
    downloadZoneFile(zoneFile, `${selectedZone}.zone`);
  };

  const relayState: FetchState = isLive ? "ok" : "loading";

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between flex-wrap gap-3">
        <div className="flex items-center gap-3">
          <h2 className="text-lg font-semibold text-foreground">Zone Monitor</h2>
          <select
            value={selectedZone}
            onChange={(e) => setSelectedZone(e.target.value)}
            className="h-8 px-2 text-xs font-mono rounded-md border border-border bg-secondary text-foreground focus:outline-none focus:ring-1 focus:ring-ring"
          >
            {zoneOptions.map((z) => (
              <option key={z} value={z}>{z}</option>
            ))}
          </select>
        </div>
        <Button
          variant="outline"
          size="sm"
          onClick={refresh}
          disabled={apiState.state === "loading" || dnsState.state === "loading"}
        >
          {apiState.state === "loading" || dnsState.state === "loading"
            ? "Refreshing..."
            : "Refresh"}
        </Button>
      </div>

      <div className="grid gap-3 sm:grid-cols-3">
        <SourceCard
          title="Relay"
          records={relayRecords}
          state={relayState}
        />
        <SourceCard
          title="Bot API"
          records={apiState.records}
          state={apiState.state}
          error={apiState.error}
        />
        <SourceCard
          title="Live DNS"
          records={dnsState.records}
          state={dnsState.state}
          error={dnsState.error}
        />
      </div>

      <ConformanceBar result={comparison} />

      <div className="flex items-center gap-2">
        <Button size="sm" onClick={handleDownload} disabled={comparison.stats.match === 0 && relayRecords.length === 0 && apiState.records.length === 0}>
          Download Zone File
        </Button>
        <a
          href={`https://relay.cashu.email/?zone=${encodeURIComponent(selectedZone)}`}
          target="_blank"
          rel="noopener noreferrer"
          className="text-xs text-primary hover:text-primary/80 transition-colors"
        >
          View relay {"\u2192"}
        </a>
      </div>

      {comparison.discrepancies.length > 0 && (
        <Card className="overflow-hidden">
          <div className="px-4 py-2 border-b border-border bg-muted/30">
            <span className="text-sm font-semibold text-foreground">
              Discrepancies ({comparison.discrepancies.length})
            </span>
          </div>
          {comparison.discrepancies.map((d, i) => (
            <DiscrepancyRow key={`${d.type}-${i}`} discrepancy={d} />
          ))}
        </Card>
      )}

      {comparison.conflicts.length > 0 && (
        <Card className="overflow-hidden">
          <div className="px-4 py-2 border-b border-border bg-muted/30">
            <span className="text-sm font-semibold text-foreground">
              Conflicts ({comparison.conflicts.length})
            </span>
          </div>
          {comparison.conflicts.map((c, i) => (
            <ConflictRow key={`conflict-${i}`} conflict={c} />
          ))}
        </Card>
      )}

      <Card className="overflow-hidden">
        <div className="px-4 py-2 border-b border-border bg-muted/30 flex items-center justify-between">
          <span className="text-sm font-semibold text-foreground">
            Zone File Preview
          </span>
          <Button variant="ghost" size="sm" onClick={handleDownload}>
            .zone
          </Button>
        </div>
        <pre className="p-4 text-xs font-mono text-foreground/90 overflow-x-auto max-h-96 leading-relaxed">
          {zoneFile}
        </pre>
      </Card>
    </div>
  );
}
