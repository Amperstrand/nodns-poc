"use client";

import { useState, useCallback, useEffect, useRef } from "react";
import {
  generateEphemeralKeyPair,
  keyPairFromNsec,
  publishDnsEvent,
  publishDeleteEvent,
} from "@/lib/nostr";
import { queryDoh } from "@/lib/dns";
import { DEFAULT_ZONE } from "@/lib/constants";
import { validateRecord } from "@/lib/validation";
import { type ZonePricing } from "@/lib/api";
import { fetchPricing } from "@/lib/sources";
import type { KeyPair, PendingRecord, DnsRecord, FeedbackType } from "@/lib/types";

const API_BASE = process.env.NEXT_PUBLIC_API_BASE ?? "";
import {
  PublishPipeline,
  type PipelineStatus,
} from "@/components/publish-pipeline";
import { CertRequest } from "@/components/cert-request";

interface DemoPublishEvent extends CustomEvent {
  detail: { message: string };
}

export function Dashboard() {
  const [keyPair, setKeyPair] = useState<KeyPair | null>(null);
  const [showKeyInput, setShowKeyInput] = useState(false);
  const [nsecInput, setNsecInput] = useState("");
  const [nsecError, setNsecError] = useState("");
  const [pendingRecords, setPendingRecords] = useState<PendingRecord[]>([]);
  const [feedback, setFeedback] = useState<{
    message: string;
    type: FeedbackType;
  } | null>(null);
  const [publishing, setPublishing] = useState(false);
  const [cashuToken, setCashuToken] = useState("");
  const [showCashu, setShowCashu] = useState(false);
  const [verifyFqdn, setVerifyFqdn] = useState("");
  const [verifyResult, setVerifyResult] = useState<string>("");

  const [recType, setRecType] = useState("TXT");
  const [recName, setRecName] = useState("");
  const [recValue, setRecValue] = useState("");
  const [recTtl, setRecTtl] = useState(300);

  const [pipelineStatus, setPipelineStatus] = useState<PipelineStatus>("idle");
  const [pipelineElapsed, setPipelineElapsed] = useState(0);
  const [pipelineResolvedData, setPipelineResolvedData] = useState<
    string | null
  >(null);
  const [pipelineFqdn, setPipelineFqdn] = useState("");
  const [pipelineEventId, setPipelineEventId] = useState<string | null>(null);

  const [publishedRecords, setPublishedRecords] = useState<DnsRecord[]>([]);
  const [deleting, setDeleting] = useState(false);
  const [pricing, setPricing] = useState<ZonePricing | null>(null);

  const pipelineTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const pipelineStartRef = useRef<number>(0);
  const dnsPollRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const keyPairRef = useRef<KeyPair | null>(null);
  const pipelineContainerRef = useRef<HTMLDivElement | null>(null);

  const pipelineTimeoutRefs = useRef<ReturnType<typeof setTimeout>[]>([]);
  const deleteTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const domainDisplay = keyPair
    ? `${keyPair.npub.slice(0, 20)}...${"." + DEFAULT_ZONE}`
    : "";

  const fullDomain = keyPair
    ? `${keyPair.npub}.${DEFAULT_ZONE}`
    : "";

  const handleCopy = useCallback((text: string) => {
    navigator.clipboard.writeText(text);
  }, []);

  const stopPipelineTimers = useCallback(() => {
    if (pipelineTimerRef.current) {
      clearInterval(pipelineTimerRef.current);
      pipelineTimerRef.current = null;
    }
    if (dnsPollRef.current) {
      clearInterval(dnsPollRef.current);
      dnsPollRef.current = null;
    }
    for (const t of pipelineTimeoutRefs.current) {
      clearTimeout(t);
    }
    pipelineTimeoutRefs.current = [];
  }, []);

  const startPipelineDnsPolling = useCallback(
    (fqdn: string) => {
      let attempts = 0;
      const maxAttempts = 10;

      dnsPollRef.current = setInterval(async () => {
        attempts++;
        try {
          const data = await queryDoh(fqdn, "TXT");
          if (data.Answer && data.Answer.length > 0) {
            const recordData = data.Answer[0].data.replace(/^"|"$/g, "");
            setPipelineResolvedData(recordData);
            const elapsed = (Date.now() - pipelineStartRef.current) / 1000;
            setPipelineElapsed(elapsed);
            setPipelineStatus("success");
            stopPipelineTimers();
          }
        } catch {
          // Silently retry
        }
        if (attempts >= maxAttempts) {
          setPipelineStatus("error");
          stopPipelineTimers();
        }
      }, 2000);
    },
    [stopPipelineTimers],
  );

  const startPipeline = useCallback(
    (fqdn: string, eventId: string) => {
      stopPipelineTimers();
      pipelineStartRef.current = Date.now();
      setPipelineFqdn(fqdn);
      setPipelineEventId(eventId);
      setPipelineResolvedData(null);
      setPipelineElapsed(0);
      setPipelineStatus("publishing");

      pipelineTimerRef.current = setInterval(() => {
        setPipelineElapsed((Date.now() - pipelineStartRef.current) / 1000);
      }, 100);

      const t1 = setTimeout(() => {
        pipelineContainerRef.current?.scrollIntoView({ behavior: "smooth", block: "center" });
      }, 100);

      const t2 = setTimeout(() => {
        setPipelineStatus("processing");
      }, 1000);

      const t3 = setTimeout(() => {
        setPipelineStatus("resolving");
        startPipelineDnsPolling(fqdn);
      }, 3000);

      pipelineTimeoutRefs.current = [t1, t2, t3];
    },
    [stopPipelineTimers, startPipelineDnsPolling],
  );

  const handlePublishWithPipeline = useCallback(
    async (records: PendingRecord[], kp: KeyPair) => {
      if (records.length === 0) return;
      setPublishing(true);
      setFeedback(null);

      try {
        const event = await publishDnsEvent(
          records,
          kp.secretKey,
          cashuToken || undefined,
          pricing?.mint_url,
          pricing?.create_price,
        );
        setFeedback({
          message: `Published event with ${records.length} record(s). Event ID: ${event.id.slice(0, 16)}...`,
          type: "success",
        });

        const fqdn = `${kp.npub}.${DEFAULT_ZONE}`;
        setVerifyFqdn(fqdn);

        startPipeline(fqdn, event.id);

        setPendingRecords([]);
        setCashuToken("");
      } catch (err) {
        const msg = err instanceof Error ? err.message : "Unknown error";
        setFeedback({ message: `Publish failed: ${msg}`, type: "error" });
        setPipelineStatus("error");
      }
      setPublishing(false);
    },
    [cashuToken, startPipeline, pricing],
  );

  useEffect(() => {
    const handler = (e: Event) => {
      const demoEvent = e as DemoPublishEvent;
      if (!demoEvent.detail?.message) return;

      const message = demoEvent.detail.message;
      let kp = keyPairRef.current;
      if (!kp) {
        kp = generateEphemeralKeyPair();
        setKeyPair(kp);
        keyPairRef.current = kp;
      }

      const records: PendingRecord[] = [
        {
          type: "TXT",
          name: "",
          value: message,
          ttl: 300,
          displayName: "@ (root)",
        },
      ];

      publishDnsEvent(records, kp.secretKey, undefined, pricing?.mint_url, pricing?.create_price).then((event) => {
        setFeedback({
          message: `Published demo record. Event ID: ${event.id.slice(0, 16)}...`,
          type: "success",
        });
        const fqdn = `${kp!.npub}.${DEFAULT_ZONE}`;
        setVerifyFqdn(fqdn);
        startPipeline(fqdn, event.id);
      }).catch((err) => {
        const msg = err instanceof Error ? err.message : "Unknown error";
        setFeedback({ message: `Demo publish failed: ${msg}`, type: "error" });
        setPipelineStatus("error");
      });
    };

    window.addEventListener("nodns-demo-publish", handler);
    return () => window.removeEventListener("nodns-demo-publish", handler);
  }, [startPipeline, pricing]);

  useEffect(() => {
    return () => {
      stopPipelineTimers();
      if (deleteTimeoutRef.current) {
        clearTimeout(deleteTimeoutRef.current);
        deleteTimeoutRef.current = null;
      }
    };
  }, [stopPipelineTimers]);

  useEffect(() => {
    let cancelled = false;
    fetchPricing()
      .then((data) => {
        if (!cancelled) setPricing(data);
      })
      .catch(() => {
      });
    return () => { cancelled = true; };
  }, []);

  const handleGenerateNew = useCallback(() => {
    const kp = generateEphemeralKeyPair();
    setKeyPair(kp);
    keyPairRef.current = kp;
    setPublishedRecords([]);
    setFeedback(null);
  }, []);

  const handleLoadNsec = useCallback(() => {
    if (!nsecInput.trim()) return;
    try {
      const kp = keyPairFromNsec(nsecInput.trim());
      setKeyPair(kp);
      keyPairRef.current = kp;
      setPublishedRecords([]);
      setShowKeyInput(false);
      setNsecInput("");
      setNsecError("");
      setFeedback(null);
    } catch {
      setNsecError("Invalid nsec key. Please check and try again.");
    }
  }, [nsecInput]);

  const handleClearKeys = useCallback(() => {
    setKeyPair(null);
    keyPairRef.current = null;
    setPendingRecords([]);
    setPublishedRecords([]);
    setFeedback(null);
    setPipelineStatus("idle");
    stopPipelineTimers();
  }, [stopPipelineTimers]);

  const handleAddRecord = useCallback(() => {
    const name = recName.trim();
    const value = recValue.trim();

    if (!value) {
      setFeedback({ message: "Value is required.", type: "error" });
      return;
    }

    const validationError = validateRecord(recType, name, value);
    if (validationError) {
      setFeedback({ message: validationError, type: "error" });
      return;
    }

    const displayName = name === "@" || name === "" ? "@ (root)" : name;
    setPendingRecords((prev) => [
      ...prev,
      {
        type: recType,
        name: name === "@" ? "" : name,
        value,
        ttl: recTtl,
        displayName,
      },
    ]);
    setRecValue("");
    setFeedback(null);
  }, [recType, recName, recValue, recTtl]);

  const handleRemoveRecord = useCallback((index: number) => {
    setPendingRecords((prev) => prev.filter((_, i) => i !== index));
  }, []);

  const handlePublish = useCallback(async () => {
    if (!keyPair || pendingRecords.length === 0) return;
    await handlePublishWithPipeline(pendingRecords, keyPair);
  }, [keyPair, pendingRecords, handlePublishWithPipeline]);

  const handleVerifyDns = useCallback(async () => {
    if (!keyPair) return;
    const fqdn = `${keyPair.npub}.${DEFAULT_ZONE}`;
    try {
      const data = await queryDoh(fqdn, "TXT");
      if (data.Answer && data.Answer.length > 0) {
        const records = data.Answer.map((a) => a.data).join(", ");
        setVerifyResult(`DNS resolved: ${records}`);
      } else {
        setVerifyResult(
          "No DNS records found yet. It may take a few seconds to propagate.",
        );
      }
    } catch {
      setVerifyResult("DNS query failed. Try again in a moment.");
    }
  }, [keyPair]);

  const fetchRecords = useCallback(async () => {
    if (!keyPair) return;
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), 30_000);
    try {
      const res = await fetch(`${API_BASE}/api/records`, { signal: controller.signal });
      if (!res.ok) return;
      const data = await res.json();
      const mine = (data.records || []).filter((r: DnsRecord) => r.npub === keyPair.npub);
      setPublishedRecords(mine);
    } catch {
      // Silently fail — not critical
    } finally {
      clearTimeout(timer);
    }
  }, [keyPair]);

  const handleDelete = useCallback(async (record: DnsRecord) => {
    if (!keyPair) return;
    setDeleting(true);
    try {
      await publishDeleteEvent(
        [{ type: record.type, name: record.name === "@" ? "" : record.name }],
        keyPair.secretKey,
      );
      setFeedback({ message: `Delete event published for ${record.type} record`, type: "success" });
      deleteTimeoutRef.current = setTimeout(() => fetchRecords(), 3000);
    } catch (err) {
      const msg = err instanceof Error ? err.message : "Delete failed";
      setFeedback({ message: msg, type: "error" });
    }
    setDeleting(false);
  }, [keyPair, fetchRecords]);

  useEffect(() => {
    if (!keyPair) return;
    const id = requestAnimationFrame(() => fetchRecords());
    return () => cancelAnimationFrame(id);
  }, [keyPair, fetchRecords]);

  return (
    <section
      id="dashboard"
      className="border-t border-border bg-gradient-to-b from-background to-background px-6 py-12"
    >
      <div className="mx-auto max-w-[960px]">
        <h2 className="mb-6 text-[1.75rem] font-bold tracking-tight">
          Nostr DNS Dashboard
        </h2>
        <p className="mb-6 text-foreground">
          Generate keys, publish DNS records as Nostr events, and manage your
          nodns.shop domain.
        </p>

        <div className="grid gap-4 max-[700px]:grid-cols-1 md:grid-cols-2">
          {/* Key Gen Card */}
          <div className="rounded-[10px] border border-border bg-card p-6">
            <h3 className="mb-4 text-lg font-semibold">
              Identity &amp; Domain
            </h3>

            {!keyPair ? (
              <div className="flex gap-2">
                <button
                  onClick={handleGenerateNew}
                  className="rounded-lg bg-primary px-4 py-2.5 text-sm font-semibold text-primary-foreground transition-opacity hover:opacity-90"
                >
                  Generate New Keypair
                </button>
                <button
                  onClick={() => setShowKeyInput(true)}
                  className="rounded-lg bg-secondary px-4 py-2.5 text-sm font-semibold text-foreground transition-colors hover:bg-border"
                >
                  Load Saved Key
                </button>
              </div>
            ) : (
              <>
                <div data-testid="domain-display" className="mb-4 rounded-lg border border-primary/30 bg-primary/15 p-3.5 text-center font-mono text-base font-semibold text-primary">
                  {domainDisplay}
                </div>

                <div className="relative mb-4 rounded-lg border border-border bg-background p-3 font-mono text-xs break-all">
                  <div className="mb-1 font-sans text-[0.7rem] uppercase tracking-wider text-muted-foreground">
                    Public Key (npub)
                  </div>
                  <div data-testid="npub-value">{keyPair.npub}</div>
                  <button
                    onClick={() => handleCopy(keyPair.npub)}
                    className="absolute right-2 top-2 rounded bg-secondary px-2 py-1 text-[0.7rem] text-muted-foreground hover:text-foreground"
                  >
                    Copy
                  </button>
                </div>

                <div className="mb-4 rounded-lg border border-red-500/25 bg-red-500/8 px-4 py-3 text-sm text-red-400">
                  Save your nsec somewhere safe. If you lose it, you lose your
                  domain permanently. There is no recovery.
                </div>

                <div className="relative mb-4 rounded-lg border border-border bg-background p-3 font-mono text-xs break-all">
                  <div className="mb-1 font-sans text-[0.7rem] uppercase tracking-wider text-muted-foreground">
                    Secret Key (nsec)
                  </div>
                  <div data-testid="nsec-value">{keyPair.nsec}</div>
                  <button
                    onClick={() => handleCopy(keyPair.nsec)}
                    className="absolute right-2 top-2 rounded bg-secondary px-2 py-1 text-[0.7rem] text-muted-foreground hover:text-foreground"
                  >
                    Copy
                  </button>
                </div>

                {!showKeyInput ? (
                  <div className="space-y-3">
                    <button
                      onClick={() => setShowKeyInput(true)}
                      className="text-sm text-primary hover:underline"
                    >
                      I have an existing key
                    </button>
                    <br />
                    <button
                      onClick={handleClearKeys}
                      className="rounded-lg bg-red-500/15 px-3 py-1.5 text-xs font-semibold text-destructive hover:bg-red-500/25"
                    >
                      Clear Keys
                    </button>
                  </div>
                ) : (
                  <div className="space-y-3">
                    <div className="rounded-lg border border-red-500/25 bg-red-500/8 px-4 py-3 text-sm text-red-400">
                      ⚠️ Privacy: Using your personal nsec ties your IP address
                      to your Nostr identity. Consider using an ephemeral key
                      for DNS records.
                    </div>
                    <input
                      type="password"
                      value={nsecInput}
                      onChange={(e) => {
                        setNsecInput(e.target.value);
                        setNsecError("");
                      }}
                      placeholder="Enter nsec..."
                      className="w-full rounded-lg border border-border bg-background px-3 py-2.5 text-sm text-foreground outline-none transition-colors focus:border-primary"
                    />
                    {nsecError && (
                      <p className="text-xs text-destructive">{nsecError}</p>
                    )}
                    <div className="flex gap-2">
                      <button
                        onClick={handleLoadNsec}
                        className="rounded-lg bg-primary px-3 py-2 text-xs font-semibold text-primary-foreground"
                      >
                        Load Key
                      </button>
                      <button
                        onClick={() => {
                          setShowKeyInput(false);
                          setNsecInput("");
                          setNsecError("");
                        }}
                        className="rounded-lg bg-secondary px-3 py-2 text-xs font-semibold text-foreground"
                      >
                        Cancel
                      </button>
                    </div>
                  </div>
                )}
              </>
            )}
          </div>

          {/* Publish Card */}
          <div className="rounded-[10px] border border-border bg-card p-6">
            <h3 className="mb-4 text-lg font-semibold">
              Publish DNS Records
            </h3>
            {pricing?.enabled && (
              <p className="mb-3 text-xs text-muted-foreground">
                {pricing.create_price} sat{pricing.create_price !== 1 ? 's' : ''} per new record
                {pricing.update_price === 0 ? ' · Free updates' : ` · ${pricing.update_price} sats to update`}
                {pricing.delete_price === 0 ? ' · Free deletes' : ` · ${pricing.delete_price} sats to delete`}
              </p>
            )}

            {!keyPair ? (
              <p className="text-sm text-muted-foreground">
                Generate or load a keypair first to publish records.
              </p>
            ) : (
              <>
                <div className="mb-3.5">
                  <label className="mb-1 block text-xs font-medium text-muted-foreground">
                    Zone
                  </label>
                  <select
                    value={DEFAULT_ZONE}
                    disabled
                    className="w-full rounded-lg border border-border bg-background px-3 py-2.5 text-sm text-foreground outline-none"
                  >
                    <option value="nodns.shop">nodns.shop</option>
                  </select>
                </div>

                  <div className="mb-3.5 grid gap-2 max-[600px]:grid-cols-2 md:grid-cols-[100px_1fr_1fr_80px_auto]">
                  <div>
                    <label className="mb-1 block text-xs font-medium text-muted-foreground">
                      Type
                    </label>
                    <select
                      data-testid="rec-type"
                      value={recType}
                      onChange={(e) => setRecType(e.target.value)}
                      className="w-full rounded-lg border border-border bg-background px-3 py-2.5 text-sm text-foreground outline-none focus:border-primary"
                    >
                      <option value="TXT">TXT</option>
                      <option value="A">A</option>
                      <option value="AAAA">AAAA</option>
                      <option value="CNAME">CNAME</option>
                      <option value="MX">MX</option>
                    </select>
                  </div>
                  <div>
                    <label className="mb-1 block text-xs font-medium text-muted-foreground">
                      Name
                    </label>
                    <input
                      type="text"
                      data-testid="rec-name"
                      value={recName}
                      onChange={(e) => setRecName(e.target.value)}
                      placeholder="@ for root, or subdomain"
                      className="w-full rounded-lg border border-border bg-background px-3 py-2.5 text-sm text-foreground outline-none focus:border-primary"
                    />
                  </div>
                  <div>
                    <label className="mb-1 block text-xs font-medium text-muted-foreground">
                      Value
                    </label>
                    <input
                      type="text"
                      data-testid="rec-value"
                      value={recValue}
                      onChange={(e) => setRecValue(e.target.value)}
                      placeholder="IP, hostname, or text"
                      className="w-full rounded-lg border border-border bg-background px-3 py-2.5 text-sm text-foreground outline-none focus:border-primary"
                    />
                  </div>
                  <div>
                    <label className="mb-1 block text-xs font-medium text-muted-foreground">
                      TTL
                    </label>
                    <input
                      type="number"
                      value={recTtl}
                      onChange={(e) =>
                        setRecTtl(
                          Math.min(
                            86400,
                            Math.max(60, Number(e.target.value) || 300),
                          ),
                        )
                      }
                      min={60}
                      max={86400}
                      className="w-full rounded-lg border border-border bg-background px-3 py-2.5 text-sm text-foreground outline-none focus:border-primary"
                    />
                  </div>
                  <div>
                    <label className="mb-1 block text-xs font-medium text-muted-foreground">
                      &nbsp;
                    </label>
                    <button
                      onClick={handleAddRecord}
                      className="rounded-lg bg-secondary px-4 py-2.5 text-sm font-semibold text-foreground hover:bg-border"
                    >
                      Add
                    </button>
                  </div>
                </div>

                {pendingRecords.length > 0 && (
                  <div data-testid="record-list" className="mb-3.5 max-h-[200px] overflow-y-auto rounded-lg border border-border">
                    {pendingRecords.map((r, i) => (
                      <div
                        key={i}
                        className="flex items-center justify-between border-b border-border px-3 py-2 text-sm last:border-b-0"
                      >
                        <span>
                          <span className="mr-2 rounded bg-primary/15 px-1.5 py-0.5 text-[0.7rem] font-semibold text-primary">
                            {r.type}
                          </span>
                          {r.displayName} → {r.value}{" "}
                          <span className="text-muted-foreground">(TTL {r.ttl})</span>
                        </span>
                        <button
                          data-testid="remove-record-btn"
                          onClick={() => handleRemoveRecord(i)}
                          className="px-1 text-destructive opacity-60 hover:opacity-100"
                        >
                          ×
                        </button>
                      </div>
                    ))}
                  </div>
                )}

                {pendingRecords.length > 0 && (
                  <p data-testid="record-count" className="mb-3 text-xs text-muted-foreground">
                    {pendingRecords.length} record
                    {pendingRecords.length > 1 ? "s" : ""} queued
                  </p>
                )}

                <div className="mb-3">
                  <button
                    onClick={() => setShowCashu(!showCashu)}
                    className="text-xs text-muted-foreground hover:text-foreground"
                  >
                    {showCashu ? "▾" : "▸"} Payment (optional)
                  </button>
                  {showCashu && (
                    <div className="mt-2">
                      <label className="mb-1 block text-xs font-medium text-muted-foreground">
                        Cashu Token
                      </label>
                      <input
                        type="text"
                        value={cashuToken}
                        onChange={(e) => setCashuToken(e.target.value)}
                        placeholder="cashuA..."
                        className="w-full rounded-lg border border-border bg-background px-3 py-2.5 text-sm text-foreground outline-none focus:border-primary"
                      />
                      <p className="mt-1 text-xs text-muted-foreground">
                        {pricing?.enabled
                          ? `Required for new records (${pricing.create_price} sat${pricing.create_price !== 1 ? 's' : ''})`
                          : 'Optional Cashu token for payment'}
                      </p>
                    </div>
                  )}
                </div>

                <button
                  onClick={handlePublish}
                  disabled={publishing || pendingRecords.length === 0}
                  className="rounded-lg bg-primary px-4 py-2.5 text-sm font-semibold text-primary-foreground transition-opacity hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
                >
                  {publishing ? "Publishing..." : "Publish to Nostr"}
                </button>

                {feedback && (
                  <div
                    data-testid="publish-feedback"
                    className={`mt-3 rounded-lg border px-3.5 py-2.5 text-sm ${
                      feedback.type === "success"
                        ? "border-chart-2/25 bg-chart-2/10 text-chart-2"
                        : "border-red-500/25 bg-red-500/10 text-destructive"
                    }`}
                  >
                    {feedback.message}
                  </div>
                )}

                {verifyFqdn &&
                  feedback?.type === "success" &&
                  pipelineStatus === "idle" && (
                    <div className="mt-4 border-t border-border pt-4">
                      <h4 className="mb-2 text-sm font-semibold">
                        Verify your record
                      </h4>
                      <pre className="mb-3 overflow-x-auto rounded-lg border border-border bg-background p-3 text-xs">
                        <code>
                          dig {keyPair?.npub}.{DEFAULT_ZONE} TXT
                        </code>
                      </pre>
                      <button
                        onClick={handleVerifyDns}
                        className="rounded-lg bg-secondary px-3 py-2 text-xs font-semibold text-foreground hover:bg-border"
                      >
                        Check DNS
                      </button>
                      {verifyResult && (
                        <p className="mt-2 text-xs text-foreground">
                          {verifyResult}
                        </p>
                      )}
                    </div>
                  )}
              </>
            )}
          </div>
        </div>

        {keyPair && publishedRecords.length > 0 && (
          <div className="mt-4 rounded-[10px] border border-border bg-card p-6">
            <h3 className="mb-4 text-lg font-semibold">Your Records</h3>
            <div className="space-y-2">
              {publishedRecords.map((r, i) => (
                <div key={i} className="flex items-center justify-between rounded-lg border border-border px-3 py-2">
                  <span className="text-sm">
                    <span className="mr-2 rounded bg-primary/15 px-1.5 py-0.5 text-[0.7rem] font-semibold text-primary">
                      {r.type}
                    </span>
                    <span className="text-foreground">{r.name === "@" ? "(root)" : r.name}</span>
                    <span className="mx-2 text-muted-foreground">→</span>
                    <span className="text-foreground">{r.rdata}</span>
                  </span>
                  <button
                    onClick={() => handleDelete(r)}
                    disabled={deleting}
                    className="rounded px-2 py-1 text-xs font-semibold text-destructive transition-colors hover:bg-red-500/15 disabled:opacity-50"
                  >
                    Delete
                  </button>
                </div>
              ))}
            </div>
          </div>
        )}

        <div ref={pipelineContainerRef} className="mt-6">
          <PublishPipeline
            status={pipelineStatus}
            elapsed={pipelineElapsed}
            resolvedData={pipelineResolvedData}
            fqdn={pipelineFqdn}
            eventId={pipelineEventId}
          />
        </div>

        {keyPair && (
          <CertRequest
            key={fullDomain}
            domain={fullDomain}
            disabled={pipelineStatus !== "success"}
            nsecBytes={keyPair.secretKey}
            npub={keyPair.npub}
          />
        )}
      </div>
    </section>
  );
}
