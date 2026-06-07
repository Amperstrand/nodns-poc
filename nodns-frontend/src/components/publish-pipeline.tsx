"use client";

import { useCallback } from "react";

export type PipelineStatus =
  | "idle"
  | "publishing"
  | "processing"
  | "resolving"
  | "success"
  | "error";

interface PublishPipelineProps {
  status: PipelineStatus;
  elapsed: number;
  resolvedData: string | null;
  fqdn: string;
  eventId: string | null;
}

function CopyButton({ text, label }: { text: string; label: string }) {
  const handleCopy = useCallback(() => {
    navigator.clipboard.writeText(text);
  }, [text]);

  return (
    <button
      onClick={handleCopy}
      className="ml-2 rounded bg-[#222] px-2 py-0.5 text-[0.7rem] text-[#666] transition-colors hover:text-[#e0e0e0]"
    >
      {label}
    </button>
  );
}

interface StageProps {
  label: string;
  state: "pending" | "active" | "done";
}

function Stage({ label, state }: StageProps) {
  return (
    <div className="flex items-center gap-2">
      <span
        className={`inline-block h-3 w-3 rounded-full transition-all duration-500 ${
          state === "active"
            ? "animate-pipeline-pulse bg-[#ff6b35] shadow-[0_0_8px_rgba(255,107,53,0.6)]"
            : state === "done"
              ? "bg-[#2ecc71]"
              : "bg-[#333]"
        }`}
      />
      <span
        className={`text-sm transition-colors duration-300 ${
          state === "active"
            ? "font-semibold text-[#ff6b35]"
            : state === "done"
              ? "font-semibold text-[#2ecc71]"
              : "text-[#666]"
        }`}
      >
        {state === "done" ? "✓" : ""} {label}
      </span>
    </div>
  );
}

export function PublishPipeline({
  status,
  elapsed,
  resolvedData,
  fqdn,
  eventId,
}: PublishPipelineProps) {
  if (status === "idle") return null;

  const stage1State: "pending" | "active" | "done" =
    status === "publishing" ? "active" : "done";

  const stage2State: "pending" | "active" | "done" =
    status === "processing"
      ? "active"
      : status === "resolving" || status === "success"
        ? "done"
        : "pending";

  const stage3State: "pending" | "active" | "done" =
    status === "resolving"
      ? "active"
      : status === "success"
        ? "done"
        : "pending";

  return (
    <div className="rounded-[10px] border border-[#222] bg-[#141414] p-5">
      {/* Pipeline stages */}
      <div className="mb-5 flex flex-col gap-3">
        <Stage label="Publishing to Nostr..." state={stage1State} />
        <Stage label="Bot Processing..." state={stage2State} />
        <Stage label="Waiting for DNS..." state={stage3State} />
      </div>

      {/* Connecting line decoration */}
      <div className="relative mb-4 h-0.5 bg-[#222]">
        <div
          className="absolute left-0 top-0 h-full bg-[#ff6b35] transition-all duration-1000 ease-out"
          style={{
            width:
              status === "publishing"
                ? "10%"
                : status === "processing"
                  ? "40%"
                  : status === "resolving"
                    ? "75%"
                    : status === "success"
                      ? "100%"
                      : "0%",
            backgroundColor:
              status === "success" ? "#2ecc71" : "#ff6b35",
          }}
        />
      </div>

      {/* Error state */}
      {status === "error" && (
        <div className="rounded-lg border border-[rgba(231,76,60,0.25)] bg-[rgba(231,76,60,0.08)] px-4 py-3 text-sm text-[#e74c3c]">
          Pipeline failed. Please try again.
        </div>
      )}

      {/* Success state */}
      {status === "success" && (
        <div className="space-y-3">
          <div className="rounded-lg border border-[rgba(46,204,113,0.25)] bg-[rgba(46,204,113,0.08)] px-4 py-3 text-center">
            <p className="text-sm font-semibold text-[#2ecc71]">
              ✅ Record live in {elapsed.toFixed(1)}s!
            </p>
          </div>

          {/* Resolved record data */}
          {resolvedData && (
            <div className="rounded-lg border border-[#222] bg-[#0a0a0a] p-3">
              <div className="mb-1 text-[0.7rem] uppercase tracking-wider text-[#666]">
                Resolved Record
              </div>
              <code className="text-sm text-[#2ecc71]">
                TXT &quot;{resolvedData}&quot;
              </code>
              <CopyButton text={`"${resolvedData}"`} label="Copy" />
            </div>
          )}

          {/* Dig command */}
          <div className="rounded-lg border border-[#222] bg-[#0a0a0a] p-3">
            <div className="mb-1 text-[0.7rem] uppercase tracking-wider text-[#666]">
              Verify with dig
            </div>
            <code className="text-xs text-[#ff6b35]">
              dig {fqdn} TXT +short
            </code>
            <CopyButton
              text={`dig ${fqdn} TXT +short`}
              label="Copy"
            />
          </div>

          {/* Event ID */}
          {eventId && (
            <div className="rounded-lg border border-[#222] bg-[#0a0a0a] p-3">
              <div className="mb-1 text-[0.7rem] uppercase tracking-wider text-[#666]">
                Event ID
              </div>
              <code className="block break-all text-xs text-[#bbb]">
                {eventId}
              </code>
              <CopyButton text={eventId} label="Copy" />
            </div>
          )}
        </div>
      )}

      {status !== "success" && status !== "error" && (
        <div className="text-center text-xs text-[#666]">
          {elapsed.toFixed(1)}s elapsed
        </div>
      )}
    </div>
  );
}
