"use client";

import { useState, useCallback, useEffect, useRef } from "react";
import {
  generateEphemeralKeyPair,
  keyPairFromNsec,
  publishDnsEvent,
} from "@/lib/nostr";
import { queryDoh } from "@/lib/dns";
import { DEFAULT_ZONE } from "@/lib/constants";
import type { KeyPair, PendingRecord, FeedbackType } from "@/lib/types";
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

  const pipelineTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const pipelineStartRef = useRef<number>(0);
  const dnsPollRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const keyPairRef = useRef<KeyPair | null>(null);

  const domainDisplay = keyPair
    ? `${keyPair.npub.slice(0, 20)}...${"." + DEFAULT_ZONE}`
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

      setTimeout(() => {
        setPipelineStatus("processing");
      }, 1000);

      setTimeout(() => {
        setPipelineStatus("resolving");
        startPipelineDnsPolling(fqdn);
      }, 3000);
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
    [cashuToken, startPipeline],
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

      publishDnsEvent(records, kp.secretKey).then((event) => {
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
  }, [startPipeline]);

  useEffect(() => {
    return () => {
      stopPipelineTimers();
    };
  }, [stopPipelineTimers]);

  const handleGenerateNew = useCallback(() => {
    const kp = generateEphemeralKeyPair();
    setKeyPair(kp);
    keyPairRef.current = kp;
    setFeedback(null);
  }, []);

  const handleLoadNsec = useCallback(() => {
    if (!nsecInput.trim()) return;
    try {
      const kp = keyPairFromNsec(nsecInput.trim());
      setKeyPair(kp);
      keyPairRef.current = kp;
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
    setFeedback(null);
    setPipelineStatus("idle");
    stopPipelineTimers();
  }, [stopPipelineTimers]);

  const handleAddRecord = useCallback(() => {
    if (!recValue.trim()) {
      setFeedback({ message: "Value is required.", type: "error" });
      return;
    }
    const name = recName.trim();
    const displayName = name === "@" || name === "" ? "@ (root)" : name;
    setPendingRecords((prev) => [
      ...prev,
      {
        type: recType,
        name: name === "@" ? "" : name,
        value: recValue.trim(),
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

  return (
    <section
      id="dashboard"
      className="border-t border-[#222] bg-gradient-to-b from-[#0f0f0f] to-[#0a0a0a] px-6 py-12"
    >
      <div className="mx-auto max-w-[960px]">
        <h2 className="mb-6 text-[1.75rem] font-bold tracking-tight">
          Nostr DNS Dashboard
        </h2>
        <p className="mb-6 text-[#bbb]">
          Generate keys, publish DNS records as Nostr events, and manage your
          nodns.shop domain.
        </p>

        <div className="grid gap-4 max-[700px]:grid-cols-1 md:grid-cols-2">
          {/* Key Gen Card */}
          <div className="rounded-[10px] border border-[#222] bg-[#141414] p-6">
            <h3 className="mb-4 text-lg font-semibold">
              Identity &amp; Domain
            </h3>

            {!keyPair ? (
              <div className="flex gap-2">
                <button
                  onClick={handleGenerateNew}
                  className="rounded-lg bg-[#ff6b35] px-4 py-2.5 text-sm font-semibold text-white transition-opacity hover:opacity-90"
                >
                  Generate New Keypair
                </button>
                <button
                  onClick={() => setShowKeyInput(true)}
                  className="rounded-lg bg-[#222] px-4 py-2.5 text-sm font-semibold text-[#e0e0e0] transition-colors hover:bg-[#333]"
                >
                  Load Saved Key
                </button>
              </div>
            ) : (
              <>
                <div className="mb-4 rounded-lg border border-[rgba(255,107,53,0.3)] bg-[rgba(255,107,53,0.15)] p-3.5 text-center font-mono text-base font-semibold text-[#ff6b35]">
                  {domainDisplay}
                </div>

                <div className="relative mb-4 rounded-lg border border-[#222] bg-[#0a0a0a] p-3 font-mono text-xs break-all">
                  <div className="mb-1 font-sans text-[0.7rem] uppercase tracking-wider text-[#666]">
                    Public Key (npub)
                  </div>
                  <div>{keyPair.npub}</div>
                  <button
                    onClick={() => handleCopy(keyPair.npub)}
                    className="absolute right-2 top-2 rounded bg-[#222] px-2 py-1 text-[0.7rem] text-[#666] hover:text-[#e0e0e0]"
                  >
                    Copy
                  </button>
                </div>

                <div className="mb-4 rounded-lg border border-[rgba(231,76,60,0.25)] bg-[rgba(231,76,60,0.08)] px-4 py-3 text-sm text-[#e8a49c]">
                  Save your nsec somewhere safe. If you lose it, you lose your
                  domain permanently. There is no recovery.
                </div>

                <div className="relative mb-4 rounded-lg border border-[#222] bg-[#0a0a0a] p-3 font-mono text-xs break-all">
                  <div className="mb-1 font-sans text-[0.7rem] uppercase tracking-wider text-[#666]">
                    Secret Key (nsec)
                  </div>
                  <div>{keyPair.nsec}</div>
                  <button
                    onClick={() => handleCopy(keyPair.nsec)}
                    className="absolute right-2 top-2 rounded bg-[#222] px-2 py-1 text-[0.7rem] text-[#666] hover:text-[#e0e0e0]"
                  >
                    Copy
                  </button>
                </div>

                {!showKeyInput ? (
                  <div className="space-y-3">
                    <button
                      onClick={() => setShowKeyInput(true)}
                      className="text-sm text-[#ff6b35] hover:underline"
                    >
                      I have an existing key
                    </button>
                    <br />
                    <button
                      onClick={handleClearKeys}
                      className="rounded-lg bg-[rgba(231,76,60,0.15)] px-3 py-1.5 text-xs font-semibold text-[#e74c3c] hover:bg-[rgba(231,76,60,0.25)]"
                    >
                      Clear Keys
                    </button>
                  </div>
                ) : (
                  <div className="space-y-3">
                    <div className="rounded-lg border border-[rgba(231,76,60,0.25)] bg-[rgba(231,76,60,0.08)] px-4 py-3 text-sm text-[#e8a49c]">
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
                      className="w-full rounded-lg border border-[#222] bg-[#0a0a0a] px-3 py-2.5 text-sm text-[#e0e0e0] outline-none transition-colors focus:border-[#ff6b35]"
                    />
                    {nsecError && (
                      <p className="text-xs text-[#e74c3c]">{nsecError}</p>
                    )}
                    <div className="flex gap-2">
                      <button
                        onClick={handleLoadNsec}
                        className="rounded-lg bg-[#ff6b35] px-3 py-2 text-xs font-semibold text-white"
                      >
                        Load Key
                      </button>
                      <button
                        onClick={() => {
                          setShowKeyInput(false);
                          setNsecInput("");
                          setNsecError("");
                        }}
                        className="rounded-lg bg-[#222] px-3 py-2 text-xs font-semibold text-[#e0e0e0]"
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
          <div className="rounded-[10px] border border-[#222] bg-[#141414] p-6">
            <h3 className="mb-4 text-lg font-semibold">
              Publish DNS Records
            </h3>

            {!keyPair ? (
              <p className="text-sm text-[#666]">
                Generate or load a keypair first to publish records.
              </p>
            ) : (
              <>
                <div className="mb-3.5">
                  <label className="mb-1 block text-xs font-medium text-[#666]">
                    Zone
                  </label>
                  <select
                    value={DEFAULT_ZONE}
                    disabled
                    className="w-full rounded-lg border border-[#222] bg-[#0a0a0a] px-3 py-2.5 text-sm text-[#e0e0e0] outline-none"
                  >
                    <option value="nodns.shop">nodns.shop</option>
                  </select>
                </div>

                <div className="mb-3.5 grid gap-2 max-[600px]:grid-cols-2 md:grid-cols-[100px_1fr_1fr_80px_auto]">
                  <div>
                    <label className="mb-1 block text-xs font-medium text-[#666]">
                      Type
                    </label>
                    <select
                      value={recType}
                      onChange={(e) => setRecType(e.target.value)}
                      className="w-full rounded-lg border border-[#222] bg-[#0a0a0a] px-3 py-2.5 text-sm text-[#e0e0e0] outline-none focus:border-[#ff6b35]"
                    >
                      <option value="TXT">TXT</option>
                      <option value="A">A</option>
                      <option value="AAAA">AAAA</option>
                      <option value="CNAME">CNAME</option>
                      <option value="MX">MX</option>
                    </select>
                  </div>
                  <div>
                    <label className="mb-1 block text-xs font-medium text-[#666]">
                      Name
                    </label>
                    <input
                      type="text"
                      value={recName}
                      onChange={(e) => setRecName(e.target.value)}
                      placeholder="@ for root, or subdomain"
                      className="w-full rounded-lg border border-[#222] bg-[#0a0a0a] px-3 py-2.5 text-sm text-[#e0e0e0] outline-none focus:border-[#ff6b35]"
                    />
                  </div>
                  <div>
                    <label className="mb-1 block text-xs font-medium text-[#666]">
                      Value
                    </label>
                    <input
                      type="text"
                      value={recValue}
                      onChange={(e) => setRecValue(e.target.value)}
                      placeholder="IP, hostname, or text"
                      className="w-full rounded-lg border border-[#222] bg-[#0a0a0a] px-3 py-2.5 text-sm text-[#e0e0e0] outline-none focus:border-[#ff6b35]"
                    />
                  </div>
                  <div>
                    <label className="mb-1 block text-xs font-medium text-[#666]">
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
                      className="w-full rounded-lg border border-[#222] bg-[#0a0a0a] px-3 py-2.5 text-sm text-[#e0e0e0] outline-none focus:border-[#ff6b35]"
                    />
                  </div>
                  <div>
                    <label className="mb-1 block text-xs font-medium text-[#666]">
                      &nbsp;
                    </label>
                    <button
                      onClick={handleAddRecord}
                      className="rounded-lg bg-[#222] px-4 py-2.5 text-sm font-semibold text-[#e0e0e0] hover:bg-[#333]"
                    >
                      Add
                    </button>
                  </div>
                </div>

                {pendingRecords.length > 0 && (
                  <div className="mb-3.5 max-h-[200px] overflow-y-auto rounded-lg border border-[#222]">
                    {pendingRecords.map((r, i) => (
                      <div
                        key={i}
                        className="flex items-center justify-between border-b border-[#222] px-3 py-2 text-sm last:border-b-0"
                      >
                        <span>
                          <span className="mr-2 rounded bg-[rgba(255,107,53,0.15)] px-1.5 py-0.5 text-[0.7rem] font-semibold text-[#ff6b35]">
                            {r.type}
                          </span>
                          {r.displayName} → {r.value}{" "}
                          <span className="text-[#666]">(TTL {r.ttl})</span>
                        </span>
                        <button
                          onClick={() => handleRemoveRecord(i)}
                          className="px-1 text-[#e74c3c] opacity-60 hover:opacity-100"
                        >
                          ×
                        </button>
                      </div>
                    ))}
                  </div>
                )}

                {pendingRecords.length > 0 && (
                  <p className="mb-3 text-xs text-[#666]">
                    {pendingRecords.length} record
                    {pendingRecords.length > 1 ? "s" : ""} queued
                  </p>
                )}

                <div className="mb-3">
                  <button
                    onClick={() => setShowCashu(!showCashu)}
                    className="text-xs text-[#666] hover:text-[#e0e0e0]"
                  >
                    {showCashu ? "▾" : "▸"} Payment (optional)
                  </button>
                  {showCashu && (
                    <div className="mt-2">
                      <label className="mb-1 block text-xs font-medium text-[#666]">
                        Cashu Token
                      </label>
                      <input
                        type="text"
                        value={cashuToken}
                        onChange={(e) => setCashuToken(e.target.value)}
                        placeholder="cashuA..."
                        className="w-full rounded-lg border border-[#222] bg-[#0a0a0a] px-3 py-2.5 text-sm text-[#e0e0e0] outline-none focus:border-[#ff6b35]"
                      />
                      <p className="mt-1 text-xs text-[#666]">
                        Required for new records when payment is enabled (250
                        sats)
                      </p>
                    </div>
                  )}
                </div>

                <button
                  onClick={handlePublish}
                  disabled={publishing || pendingRecords.length === 0}
                  className="rounded-lg bg-[#ff6b35] px-4 py-2.5 text-sm font-semibold text-white transition-opacity hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
                >
                  {publishing ? "Publishing..." : "Publish to Nostr"}
                </button>

                {feedback && (
                  <div
                    className={`mt-3 rounded-lg border px-3.5 py-2.5 text-sm ${
                      feedback.type === "success"
                        ? "border-[rgba(46,204,113,0.25)] bg-[rgba(46,204,113,0.1)] text-[#2ecc71]"
                        : "border-[rgba(231,76,60,0.25)] bg-[rgba(231,76,60,0.1)] text-[#e74c3c]"
                    }`}
                  >
                    {feedback.message}
                  </div>
                )}

                {verifyFqdn &&
                  feedback?.type === "success" &&
                  pipelineStatus === "idle" && (
                    <div className="mt-4 border-t border-[#222] pt-4">
                      <h4 className="mb-2 text-sm font-semibold">
                        Verify your record
                      </h4>
                      <pre className="mb-3 overflow-x-auto rounded-lg border border-[#222] bg-[#0a0a0a] p-3 text-xs">
                        <code>
                          dig {keyPair?.npub}.{DEFAULT_ZONE} TXT
                        </code>
                      </pre>
                      <button
                        onClick={handleVerifyDns}
                        className="rounded-lg bg-[#222] px-3 py-2 text-xs font-semibold text-[#e0e0e0] hover:bg-[#333]"
                      >
                        Check DNS
                      </button>
                      {verifyResult && (
                        <p className="mt-2 text-xs text-[#bbb]">
                          {verifyResult}
                        </p>
                      )}
                    </div>
                  )}
              </>
            )}
          </div>
        </div>

        <div className="mt-6">
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
            key={domainDisplay}
            domain={domainDisplay}
            disabled={pipelineStatus !== "success"}
          />
        )}
      </div>
    </section>
  );
}
