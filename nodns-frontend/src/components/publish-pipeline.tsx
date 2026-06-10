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
      className="ml-2 rounded bg-secondary px-2 py-0.5 text-[0.7rem] text-muted-foreground transition-colors hover:text-foreground"
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
            ? "animate-pipeline-pulse bg-primary shadow-[0_0_8px_var(--color-primary)]"
            : state === "done"
              ? "bg-chart-2"
              : "bg-muted-foreground/40"
        }`}
      />
      <span
        className={`text-sm transition-colors duration-300 ${
          state === "active"
            ? "font-semibold text-primary"
            : state === "done"
              ? "font-semibold text-chart-2"
              : "text-muted-foreground"
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
    <div className="rounded-[10px] border border-border bg-card p-5">
      {/* Pipeline stages */}
      <div className="mb-5 flex flex-col gap-3">
        <Stage label="Publishing to Nostr..." state={stage1State} />
        <Stage label="Bot Processing..." state={stage2State} />
        <Stage label="Waiting for DNS..." state={stage3State} />
      </div>

      {/* Connecting line decoration */}
      <div className="relative mb-4 h-0.5 bg-border">
        <div
          className="absolute left-0 top-0 h-full transition-all duration-1000 ease-out"
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
              status === "success" ? "var(--chart-2)" : "var(--primary)",
          }}
        />
      </div>

      {/* Error state */}
      {status === "error" && (
        <div className="rounded-lg border border-red-500/25 bg-red-500/8 px-4 py-3 text-sm text-destructive">
          Pipeline failed. Please try again.
        </div>
      )}

      {/* Success state */}
      {status === "success" && (
        <div className="space-y-3">
          <div className="rounded-lg border border-chart-2/25 bg-chart-2/8 px-4 py-3 text-center">
            <p className="text-sm font-semibold text-chart-2">
              ✅ Record live in {elapsed.toFixed(1)}s!
            </p>
          </div>

          {/* Resolved record data */}
          {resolvedData && (
            <div className="rounded-lg border border-border bg-background p-3">
              <div className="mb-1 text-[0.7rem] uppercase tracking-wider text-muted-foreground">
                Resolved Record
              </div>
              <code className="text-sm text-chart-2">
                TXT &quot;{resolvedData}&quot;
              </code>
              <CopyButton text={`"${resolvedData}"`} label="Copy" />
            </div>
          )}

          {/* Dig command */}
          <div className="rounded-lg border border-border bg-background p-3">
            <div className="mb-1 text-[0.7rem] uppercase tracking-wider text-muted-foreground">
              Verify with dig
            </div>
            <code className="text-xs text-primary">
              dig {fqdn} TXT +short
            </code>
            <CopyButton
              text={`dig ${fqdn} TXT +short`}
              label="Copy"
            />
          </div>

          {/* Event ID */}
          {eventId && (
            <div className="rounded-lg border border-border bg-background p-3">
              <div className="mb-1 text-[0.7rem] uppercase tracking-wider text-muted-foreground">
                Event ID
              </div>
              <code className="block break-all text-xs text-foreground">
                {eventId}
              </code>
              <CopyButton text={eventId} label="Copy" />
            </div>
          )}
        </div>
      )}

      {status !== "success" && status !== "error" && (
        <div className="text-center text-xs text-muted-foreground">
          {elapsed.toFixed(1)}s elapsed
        </div>
      )}
    </div>
  );
}
