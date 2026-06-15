"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { cn } from "@/lib/utils";
import { CopyIcon, CheckIcon } from "lucide-react";

/* ------------------------------------------------------------------ */
/*  Source metadata                                                     */
/* ------------------------------------------------------------------ */

const SOURCE_INFO = {
  api: {
    icon: "\u{1F5C4}\uFE0F",
    name: "Operator API",
    explanation:
      "The bot's database of processed events. This is what the operator has recorded \u2014 not authoritative, but fast.",
    authority: false,
    getCommand: (fqdn: string) =>
      `curl -s https://nodns.shop/api/records?domain=${fqdn} | jq .records[].rdata`,
  },
  nostr: {
    icon: "\u{1F510}",
    name: "Nostr Relays",
    explanation:
      "Cryptographically signed kind 11111 events on Nostr relays. This is the source of truth \u2014 anyone can verify these independently.",
    authority: true,
    getCommand: (_fqdn: string) =>
      `nak req -k 11111 -l 10 wss://relay.damus.io`,
  },
  dns: {
    icon: "\u{1F310}",
    name: "DNS Resolver",
    explanation:
      "Live DNS resolution via Cloudflare DoH (1.1.1.1). This is what the rest of the world sees right now.",
    authority: false,
    getCommand: (fqdn: string) =>
      `dig @1.1.1.1 ${fqdn} A ${fqdn} TXT +short`,
  },
} as const;

type SourceKey = keyof typeof SOURCE_INFO;

/* ------------------------------------------------------------------ */
/*  Status helpers                                                      */
/* ------------------------------------------------------------------ */

const STATUS_DOT: Record<string, string> = {
  ok: "\u{1F7E2}",
  error: "\u{1F534}",
  loading: "\u{1F7E1}",
  unavailable: "\u26AB",
};

const STATUS_COLORS: Record<string, string> = {
  ok: "text-emerald-400",
  error: "text-red-400",
  loading: "text-yellow-400",
  unavailable: "text-zinc-500",
};

const STATUS_BG: Record<string, string> = {
  ok: "bg-emerald-500/10",
  error: "bg-red-500/10",
  loading: "bg-yellow-500/10",
  unavailable: "bg-zinc-500/10",
};

/* ------------------------------------------------------------------ */
/*  Props                                                               */
/* ------------------------------------------------------------------ */

export interface SourceIndicatorProps {
  source: SourceKey;
  status: "ok" | "error" | "loading" | "unavailable";
  fqdn: string;
  compact?: boolean;
}

/* ------------------------------------------------------------------ */
/*  Component                                                           */
/* ------------------------------------------------------------------ */

export function SourceIndicator({
  source,
  status,
  fqdn,
  compact = false,
}: SourceIndicatorProps) {
  const info = SOURCE_INFO[source];
  const [open, setOpen] = useState(false);
  const [copied, setCopied] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  // Close on click outside
  useEffect(() => {
    if (!open) return;
    const handleClick = (e: MouseEvent) => {
      if (
        containerRef.current &&
        !containerRef.current.contains(e.target as Node)
      ) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [open]);

  // Close on Escape
  useEffect(() => {
    if (!open) return;
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("keydown", handleKey);
    return () => document.removeEventListener("keydown", handleKey);
  }, [open]);

  const copyCommand = useCallback(async () => {
    const cmd = info.getCommand(fqdn);
    await navigator.clipboard.writeText(cmd);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  }, [info, fqdn]);

  const command = info.getCommand(fqdn);

  /* ---- Compact mode: just the emoji with a tiny popover ---- */
  if (compact) {
    return (
      <div ref={containerRef} className="relative">
        <button
          type="button"
          onClick={() => setOpen((v) => !v)}
          onMouseEnter={() => setOpen(true)}
          onFocus={() => setOpen(true)}
          className={cn(
            "text-[10px] px-1 py-0.5 rounded bg-secondary text-muted-foreground cursor-default",
            "hover:bg-secondary/80 transition-colors",
            open && "bg-secondary/80"
          )}
          aria-label={`${info.name}: ${status}`}
          aria-haspopup="dialog"
          aria-expanded={open}
        >
          {info.icon}
        </button>
        {open && (
          <div
            role="dialog"
            aria-label={`${info.name} details`}
            className={cn(
              "absolute z-50 bottom-full left-1/2 -translate-x-1/2 mb-2",
              "w-72 p-3 rounded-lg",
              "bg-popover text-popover-foreground ring-1 ring-foreground/10 shadow-xl",
              "animate-in fade-in-0 zoom-in-95"
            )}
          >
            <PopoverContent
              info={info}
              status={status}
              command={command}
              copied={copied}
              onCopy={copyCommand}
            />
          </div>
        )}
      </div>
    );
  }

  /* ---- Full mode: emoji + status dot + status text ---- */
  return (
    <div ref={containerRef} className="relative">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        onMouseEnter={() => setOpen(true)}
        onFocus={() => setOpen(true)}
        className={cn(
          "flex items-center gap-1.5 text-sm rounded-md px-2 py-1 -mx-2 -my-1",
          "hover:bg-muted/50 transition-colors cursor-default",
          open && "bg-muted/50"
        )}
        aria-label={`${info.name}: ${status}`}
        aria-haspopup="dialog"
        aria-expanded={open}
      >
        <span>{info.icon}</span>
        <span>{STATUS_DOT[status]}</span>
        <span className={cn("text-xs", STATUS_COLORS[status])}>
          {status}
        </span>
      </button>
      {open && (
        <div
          role="dialog"
          aria-label={`${info.name} details`}
          className={cn(
            "absolute z-50 top-full left-0 mt-2",
            "w-80 p-4 rounded-xl",
            "bg-popover text-popover-foreground ring-1 ring-foreground/10 shadow-xl",
            "animate-in fade-in-0 zoom-in-95"
          )}
        >
          <PopoverContent
            info={info}
            status={status}
            command={command}
            copied={copied}
            onCopy={copyCommand}
          />
        </div>
      )}
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Shared popover body                                                 */
/* ------------------------------------------------------------------ */

function PopoverContent({
  info,
  status,
  command,
  copied,
  onCopy,
}: {
  info: (typeof SOURCE_INFO)[SourceKey];
  status: string;
  command: string;
  copied: boolean;
  onCopy: () => void;
}) {
  return (
    <div className="space-y-3">
      {/* Header */}
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-2">
          <span className="text-lg">{info.icon}</span>
          <span className="font-semibold text-sm text-foreground">
            {info.name}
          </span>
        </div>
        <span
          className={cn(
            "inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[10px] font-medium uppercase tracking-wider",
            STATUS_BG[status],
            STATUS_COLORS[status]
          )}
        >
          {STATUS_DOT[status]} {status}
        </span>
      </div>

      {/* Authority badge */}
      {info.authority && (
        <span className="inline-flex items-center gap-1 rounded-full bg-emerald-500/10 text-emerald-400 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider ring-1 ring-emerald-500/20">
          Source of truth
        </span>
      )}

      {/* Explanation */}
      <p className="text-xs text-foreground/70 leading-relaxed">
        {info.explanation}
      </p>

      {/* Terminal command */}
      <div className="rounded-md bg-zinc-900 ring-1 ring-foreground/5 p-2.5 group">
        <div className="flex items-center justify-between gap-2 mb-1.5">
          <span className="text-[10px] text-muted-foreground uppercase tracking-wider font-medium">
            Terminal
          </span>
          <button
            type="button"
            onClick={onCopy}
            className={cn(
              "inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-[10px] font-medium transition-colors",
              copied
                ? "text-emerald-400 bg-emerald-500/10"
                : "text-muted-foreground hover:text-foreground hover:bg-foreground/5"
            )}
            aria-label={copied ? "Copied" : "Copy command"}
          >
            {copied ? (
              <>
                <CheckIcon className="size-3" /> Copied!
              </>
            ) : (
              <>
                <CopyIcon className="size-3" /> Copy
              </>
            )}
          </button>
        </div>
        <code className="block text-[11px] font-mono text-foreground/80 leading-relaxed break-all whitespace-pre-wrap">
          {command}
        </code>
      </div>
    </div>
  );
}
