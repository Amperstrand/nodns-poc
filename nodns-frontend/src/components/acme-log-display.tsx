"use client";

import { useEffect, useRef, useState } from "react";
import type { AcmeLogEntry } from "@/lib/api";

interface AcmeLogDisplayProps {
  logs: AcmeLogEntry[];
  isComplete: boolean;
}

const STAGE_ICONS: Record<string, string> = {
  account_create: "📋",
  order_create: "📤",
  challenge_prepare: "🔑",
  challenge_publish: "📡",
  challenge_signal: "🔔",
  challenge_verify: "🔍",
  cert_issue: "📜",
  cert_ready: "✅",
  error: "❌",
};

function formatTimestamp(ts: number): string {
  const d = new Date(ts * 1000);
  return d.toLocaleTimeString("en-US", {
    hour12: false,
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

function LogEntry({
  entry,
  isLatest,
  isComplete,
}: {
  entry: AcmeLogEntry;
  isLatest: boolean;
  isComplete: boolean;
}) {
  const icon = STAGE_ICONS[entry.stage] || "•";
  const [expanded, setExpanded] = useState(false);

  return (
    <div className="flex gap-3">
      {/* Timeline dot and line */}
      <div className="flex flex-col items-center">
        <div
          className={`mt-1 h-2.5 w-2.5 shrink-0 rounded-full ${
            isLatest && !isComplete
              ? "animate-pulse bg-[#ff6b35]"
              : "bg-[#444]"
          }`}
        />
        {!isLatest && <div className="w-px flex-1 bg-[#222]" />}
      </div>

      {/* Content */}
      <div className="min-w-0 flex-1 pb-3">
        <div className="flex items-baseline gap-2">
          <span className="shrink-0 font-mono text-[0.7rem] text-[#666]">
            {formatTimestamp(entry.created_at)}
          </span>
          <span className="text-sm">{icon}</span>
          <span className="text-sm text-[#bbb]">{entry.message}</span>
        </div>
        {entry.details && (
          <button
            onClick={() => setExpanded(!expanded)}
            className="mt-1 text-[0.65rem] text-[#666] hover:text-[#999]"
          >
            {expanded ? "▾ hide details" : "▸ show details"}
          </button>
        )}
        {entry.details && expanded && (
          <pre className="mt-1 max-h-[80px] overflow-y-auto rounded bg-[#0a0a0a] p-2 font-mono text-[0.6rem] text-[#888]">
            {formatDetails(entry.details)}
          </pre>
        )}
      </div>
    </div>
  );
}

function formatDetails(details: string): string {
  try {
    return JSON.stringify(JSON.parse(details), null, 2);
  } catch {
    return details;
  }
}

export function AcmeLogDisplay({ logs, isComplete }: AcmeLogDisplayProps) {
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (containerRef.current) {
      containerRef.current.scrollTop = containerRef.current.scrollHeight;
    }
  }, [logs]);

  if (logs.length === 0) return null;

  return (
    <div className="rounded-lg border border-[#222] bg-[#141414] p-3">
      <div className="mb-2 text-[0.7rem] uppercase tracking-wider text-[#666]">
        ACME Progress
      </div>
      <div
        ref={containerRef}
        className="max-h-[240px] overflow-y-auto"
      >
        {logs.map((entry, i) => (
          <LogEntry
            key={`${entry.created_at}-${entry.stage}-${i}`}
            entry={entry}
            isLatest={i === logs.length - 1}
            isComplete={isComplete}
          />
        ))}
      </div>
    </div>
  );
}
