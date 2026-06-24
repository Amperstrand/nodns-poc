import { useState } from "react";
import { Badge } from "@/components/ui/badge";
import type { ExplorerEvent } from "@/lib/types";
import { pubkeyToNpub } from "@/lib/nostr";
import { timeAgo, truncateNpub } from "@/lib/format";
import { RECORD_KIND, ZONE_HANDLER_KIND } from "@/lib/constants";
import {
  parseRecords,
  parsePayment,
  checkValidity,
  parseZoneEvent,
  type RecordInfo,
  type PaymentInfo,
  type ValidityInfo,
  type ZoneEventInfo,
} from "@/lib/event-analysis";

interface EventRowProps {
  event: ExplorerEvent;
}

const RECORD_TYPE_COLORS: Record<string, string> = {
  A: "border-blue-500/30 text-blue-400 bg-blue-500/10",
  AAAA: "border-purple-500/30 text-purple-400 bg-purple-500/10",
  CNAME: "border-cyan-500/30 text-cyan-400 bg-cyan-500/10",
  TXT: "border-amber-500/30 text-amber-400 bg-amber-500/10",
  MX: "border-green-500/30 text-green-400 bg-green-500/10",
};

function recordTypeClass(type: string): string {
  return RECORD_TYPE_COLORS[type.toUpperCase()] ?? "border-border bg-muted text-foreground";
}

function PaymentBadge({ payment }: { payment: PaymentInfo }) {
  if (payment.status === "paid" && payment.isTestnut) {
    return (
      <Badge className="border-green-500/30 text-green-400 bg-green-500/10">
        {payment.amount} sats · testnut
      </Badge>
    );
  }
  if (payment.status === "paid") {
    return (
      <Badge className="border-amber-500/30 text-amber-400 bg-amber-500/10">
        {payment.amount} sats · other mint
      </Badge>
    );
  }
  if (payment.status === "free") {
    return (
      <Badge className="border-border bg-muted text-muted-foreground">free</Badge>
    );
  }
  return (
    <Badge className="border-red-500/30 text-red-400 bg-red-500/10">unpaid</Badge>
  );
}

function ValidityBadge({ validity }: { validity: ValidityInfo }) {
  if (validity.valid) {
    return (
      <Badge className="border-green-500/30 text-green-400 bg-green-500/10">
        ✓ valid
      </Badge>
    );
  }
  return (
    <Badge
      className="border-red-500/30 text-red-400 bg-red-500/10"
      title={validity.reason}
    >
      ✗ {validity.reason ?? "invalid"}
    </Badge>
  );
}

function SpecVersionBadge({ version }: { version: string }) {
  return (
    <span className="font-mono text-[10px] text-muted-foreground border border-border bg-muted rounded-full px-1.5 py-0.5">
      {version}
    </span>
  );
}

function RecordDisplay({ record }: { record: RecordInfo }) {
  return (
    <div className="flex items-center gap-2 font-mono text-xs flex-wrap">
      <Badge className={`font-semibold ${recordTypeClass(record.type)}`}>
        {record.type}
      </Badge>
      <span className="text-foreground break-all">{record.fqdn}</span>
      {record.rdata && (
        <>
          <span className="text-muted-foreground">→</span>
          <span className="text-primary break-all">
            {record.type === "TXT" ? `"${record.rdata}"` : record.rdata}
          </span>
        </>
      )}
      <span className="text-muted-foreground text-[10px]">TTL {record.ttl}</span>
    </div>
  );
}

function ExpandedDetail({ event }: { event: ExplorerEvent }) {
  const npub = pubkeyToNpub(event.pubkey);
  const createdDate = new Date(event.created_at * 1000).toISOString();
  const relayUrl = `https://relay.cashu.email/${event.id}`;

  return (
    <div className="mt-3 pt-3 border-t border-border space-y-2 text-xs">
      <div>
        <p className="text-muted-foreground mb-1 uppercase tracking-wide text-[10px]">Tags</p>
        <div className="space-y-0.5 font-mono text-[11px] break-all">
          {event.tags.map((tag, i) => (
            <div key={i} className="text-foreground/80">
              <span className="text-muted-foreground">{i}:</span>{" "}
              {JSON.stringify(tag)}
            </div>
          ))}
        </div>
      </div>
      <div className="flex flex-col gap-1 font-mono text-[11px]">
        <div>
          <span className="text-muted-foreground">Event ID:</span>{" "}
          <span className="text-foreground/80 break-all">{event.id}</span>
        </div>
        <div>
          <span className="text-muted-foreground">Pubkey:</span>{" "}
          <span className="text-foreground/80 break-all">{npub}</span>
        </div>
        <div>
          <span className="text-muted-foreground">Created:</span>{" "}
          <span className="text-foreground/80">{createdDate}</span>
        </div>
        {event.content && (
          <div>
            <span className="text-muted-foreground">Content:</span>{" "}
            <span className="text-foreground/80 break-all">{event.content}</span>
          </div>
        )}
      </div>
      <a
        href={relayUrl}
        target="_blank"
        rel="noopener noreferrer"
        className="inline-flex items-center gap-1 text-primary hover:text-primary/80 transition-colors font-mono text-[11px]"
      >
        View on relay →
      </a>
    </div>
  );
}

function RecordEventRow({ event }: { event: ExplorerEvent }) {
  const [expanded, setExpanded] = useState(false);
  const npub = pubkeyToNpub(event.pubkey);
  const records = parseRecords(event);
  const payment = parsePayment(event);
  const validity = checkValidity(event);

  return (
    <div
      className="cursor-pointer select-none"
      onClick={(e) => {
        const target = e.target as HTMLElement;
        if (target.closest("a")) return;
        setExpanded((v) => !v);
      }}
    >
      <div className="flex items-start gap-3">
        <div className="flex flex-col items-start gap-1 min-w-0 flex-1">
          <div className="flex items-center gap-2 flex-wrap">
            <span className="text-muted-foreground text-xs w-4 flex-shrink-0">
              {expanded ? "▾" : "▸"}
            </span>
            <Badge className="border-green-500/30 text-green-400 bg-green-500/10">
              RECORD
            </Badge>
            <span className="text-xs text-muted-foreground whitespace-nowrap">
              {timeAgo(event.created_at)}
            </span>
            <span className="font-mono text-xs text-muted-foreground truncate" title={npub}>
              {truncateNpub(npub, 10, 6)}
            </span>
          </div>

          {records.length === 0 ? (
            <p className="text-xs text-muted-foreground italic ml-6">No record tags</p>
          ) : (
            <div className="space-y-1 w-full mt-1 ml-6">
              {records.map((rec, i) => (
                <RecordDisplay key={i} record={rec} />
              ))}
            </div>
          )}

          <div className="flex items-center gap-1.5 flex-wrap ml-6 mt-1">
            <PaymentBadge payment={payment} />
            <ValidityBadge validity={validity} />
            <SpecVersionBadge version={validity.specVersion} />
          </div>

          {expanded && <div className="ml-6 w-full"><ExpandedDetail event={event} /></div>}
        </div>

        <a
          href={`https://relay.cashu.email/${event.id}`}
          target="_blank"
          rel="noopener noreferrer"
          className="font-mono text-xs text-muted-foreground hover:text-primary transition-colors whitespace-nowrap"
          title={event.id}
        >
          {event.id.slice(0, 8)}
        </a>
      </div>
    </div>
  );
}

function ZoneEventRow({ event }: { event: ExplorerEvent }) {
  const [expanded, setExpanded] = useState(false);
  const npub = pubkeyToNpub(event.pubkey);
  const zoneInfo: ZoneEventInfo | null = parseZoneEvent(event);

  return (
    <div
      className="cursor-pointer select-none"
      onClick={(e) => {
        const target = e.target as HTMLElement;
        if (target.closest("a")) return;
        setExpanded((v) => !v);
      }}
    >
      <div className="flex items-start gap-3">
        <div className="flex flex-col items-start gap-1 min-w-0 flex-1">
          <div className="flex items-center gap-2 flex-wrap">
            <span className="text-muted-foreground text-xs w-4 flex-shrink-0">
              {expanded ? "▾" : "▸"}
            </span>
            <Badge className="border-primary/30 text-primary bg-primary/10">
              ZONE
            </Badge>
            <span className="text-xs text-muted-foreground whitespace-nowrap">
              {timeAgo(event.created_at)}
            </span>
            <span className="font-mono text-xs text-muted-foreground truncate" title={npub}>
              {truncateNpub(npub, 10, 6)}
            </span>
          </div>

          {zoneInfo ? (
            <div className="space-y-1 w-full mt-1 ml-6">
              <div className="flex items-center gap-2 font-mono text-xs flex-wrap">
                <span className="text-foreground font-semibold break-all">{zoneInfo.zone}</span>
                {zoneInfo.status && (
                  <Badge className="border-border bg-muted text-muted-foreground capitalize">
                    {zoneInfo.status}
                  </Badge>
                )}
                {zoneInfo.testnet && (
                  <Badge className="border-amber-500/30 text-amber-400 bg-amber-500/10">
                    testnet
                  </Badge>
                )}
              </div>
              <div className="flex items-center gap-1.5 flex-wrap">
                {zoneInfo.pricing && (
                  <Badge className="border-border bg-muted text-muted-foreground">
                    create: {zoneInfo.pricing.create} · update: {zoneInfo.pricing.update} · delete: {zoneInfo.pricing.del}
                  </Badge>
                )}
                {zoneInfo.mint && (
                  <Badge className="border-border bg-muted text-muted-foreground font-mono text-[10px]">
                    {zoneInfo.mint}
                  </Badge>
                )}
                {zoneInfo.dnskeyHash && (
                  <Badge className="border-border bg-muted text-muted-foreground font-mono text-[10px]">
                    dnskey: {zoneInfo.dnskeyHash.slice(0, 16)}...
                  </Badge>
                )}
              </div>
              <div className="font-mono text-[10px] text-muted-foreground">
                operator: {truncateNpub(zoneInfo.operatorNpub, 12, 8)}
              </div>
            </div>
          ) : (
            <p className="text-xs text-muted-foreground italic ml-6">No zone tags</p>
          )}

          {expanded && <div className="ml-6 w-full"><ExpandedDetail event={event} /></div>}
        </div>

        <a
          href={`https://relay.cashu.email/${event.id}`}
          target="_blank"
          rel="noopener noreferrer"
          className="font-mono text-xs text-muted-foreground hover:text-primary transition-colors whitespace-nowrap"
          title={event.id}
        >
          {event.id.slice(0, 8)}
        </a>
      </div>
    </div>
  );
}

function GenericEventRow({ event }: { event: ExplorerEvent }) {
  const npub = pubkeyToNpub(event.pubkey);

  return (
    <div className="flex items-start gap-3">
      <div className="flex flex-col items-start gap-1 min-w-0 flex-1">
        <div className="flex items-center gap-2 flex-wrap">
          <Badge className="border-border bg-muted text-muted-foreground">
            KIND {event.kind}
          </Badge>
          <span className="text-xs text-muted-foreground whitespace-nowrap">
            {timeAgo(event.created_at)}
          </span>
          <span className="font-mono text-xs text-muted-foreground truncate" title={npub}>
            {truncateNpub(npub, 10, 6)}
          </span>
        </div>
        {event.content && (
          <p className="text-xs text-muted-foreground truncate max-w-full" title={event.content}>
            {event.content}
          </p>
        )}
      </div>

      <a
        href={`https://relay.cashu.email/${event.id}`}
        target="_blank"
        rel="noopener noreferrer"
        className="font-mono text-xs text-muted-foreground hover:text-primary transition-colors whitespace-nowrap"
        title={event.id}
      >
        {event.id.slice(0, 8)}
      </a>
    </div>
  );
}

export function EventRow({ event }: EventRowProps) {
  const isRecord = event.kind === RECORD_KIND;
  const isZone = event.kind === ZONE_HANDLER_KIND;

  return (
    <div className="border-b border-border px-4 py-3 hover:bg-secondary/30 transition-colors">
      {isRecord ? (
        <RecordEventRow event={event} />
      ) : isZone ? (
        <ZoneEventRow event={event} />
      ) : (
        <GenericEventRow event={event} />
      )}
    </div>
  );
}
