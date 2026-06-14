"use client";

import { useState, useCallback, useEffect } from "react";
import Link from "next/link";
import { SiteHeader } from "@/components/site-header";
import { SiteFooter } from "@/components/site-footer";
import { ErrorBoundary } from "@/components/error-boundary";
import { useWallet } from "@/contexts/WalletContext";
import { useIdentity } from "@/contexts/IdentityContext";
import { MINT_URL } from "@/lib/wallet";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { getEncodedToken } from "coco-cashu-core";
import type { HistoryEntry } from "coco-cashu-core";

/* ── helpers ────────────────────────────────────────────── */

function truncateMiddle(str: string, start = 10, end = 8): string {
  if (!str) return "";
  if (str.length <= start + end + 3) return str;
  return `${str.slice(0, start)}...${str.slice(-end)}`;
}

function formatTimestamp(ts: number): string {
  const d = new Date(ts);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

function historyLabel(type: HistoryEntry["type"]): string {
  switch (type) {
    case "mint":
      return "Minted";
    case "receive":
      return "Received";
    case "send":
      return "Sent";
    case "melt":
      return "Melted";
  }
}

function historySign(type: HistoryEntry["type"]): string {
  return type === "mint" || type === "receive" ? "+" : "-";
}

function historyColor(type: HistoryEntry["type"]): string {
  return type === "mint" || type === "receive"
    ? "text-emerald-400"
    : "text-orange-400";
}

/* ── sub-components ─────────────────────────────────────── */

function StatusBadge({ status }: { status: string }) {
  const cfg =
    status === "ready"
      ? { border: "border-emerald-800", bg: "bg-emerald-950/60", text: "text-emerald-400", dot: "bg-emerald-400" }
      : status === "error"
        ? { border: "border-red-800", bg: "bg-red-950/60", text: "text-red-400", dot: "bg-red-400" }
        : { border: "border-yellow-800", bg: "bg-yellow-950/60", text: "text-yellow-400", dot: "bg-yellow-400 animate-pulse" };

  const label = status === "degraded" ? "mint offline" : status;

  return (
    <div className={`flex items-center gap-1.5 rounded-md border px-2.5 py-1 text-xs font-mono ${cfg.border} ${cfg.bg} ${cfg.text}`}>
      <span className={`inline-block h-1.5 w-1.5 rounded-full ${cfg.dot}`} />
      {label}
    </div>
  );
}

function FeedbackBanner({
  success,
  message,
}: {
  success: boolean;
  message: string;
}) {
  return (
    <div
      className={`rounded-lg px-4 py-3 text-sm ${
        success
          ? "border border-emerald-800 bg-emerald-950/40 text-emerald-400"
          : "border border-red-800 bg-red-950/40 text-red-400"
      }`}
    >
      {message}
    </div>
  );
}

/* ── main content ───────────────────────────────────────── */

function WalletContent() {
  const { manager, balance, status, mintOnline, topUp, topUpState, topUpError, error: walletError } = useWallet();
  const { npub, nsec, initialized } = useIdentity();

  /* receive state */
  const [tokenInput, setTokenInput] = useState("");
  const [receiving, setReceiving] = useState(false);
  const [receiveResult, setReceiveResult] = useState<{
    success: boolean;
    message: string;
  } | null>(null);

  /* send state */
  const [sendAmount, setSendAmount] = useState("");
  const [sending, setSending] = useState(false);
  const [generatedToken, setGeneratedToken] = useState("");
  const [sendResult, setSendResult] = useState<{
    success: boolean;
    message: string;
  } | null>(null);
  const [copied, setCopied] = useState(false);

  /* history state */
  const [history, setHistory] = useState<HistoryEntry[]>([]);
  const [loadingHistory, setLoadingHistory] = useState(false);
  const [historyOffset, setHistoryOffset] = useState(0);

  /* nsec copy */
  const [nsecCopied, setNsecCopied] = useState(false);
  const [showNsecConfirm, setShowNsecConfirm] = useState(false);

  /* top-up state */
  const [topUpAmount, setTopUpAmount] = useState("");
  const [invoice, setInvoice] = useState("");
  const [invoiceCopied, setInvoiceCopied] = useState(false);

  /* ── history loading ─────────────────────────────────── */

  const loadHistory = useCallback(
    async (offset: number) => {
      if (!manager) return;
      await Promise.resolve();
      setLoadingHistory(true);
      try {
        const entries = await manager.history.getPaginatedHistory(offset, 50);
        setHistory(entries);
        setHistoryOffset(offset);
      } catch {
        // History fetch failure is non-fatal
      } finally {
        setLoadingHistory(false);
      }
    },
    [manager],
  );

  /* ── receive handler ─────────────────────────────────── */

  const handleReceive = useCallback(async () => {
    if (!manager || !tokenInput.trim()) return;
    setReceiving(true);
    setReceiveResult(null);
    try {
      await manager.wallet.receive(tokenInput.trim());
      setReceiveResult({
        success: true,
        message: "Token received successfully! Balance updated.",
      });
      setTokenInput("");
      // refresh history
      loadHistory(0);
    } catch (err) {
      setReceiveResult({
        success: false,
        message:
          err instanceof Error ? err.message : "Failed to receive token",
      });
    } finally {
      setReceiving(false);
    }
  }, [manager, tokenInput, loadHistory]);

  /* ── send handler ────────────────────────────────────── */

  const handleSend = useCallback(async () => {
    if (!manager) return;
    const amount = parseInt(sendAmount, 10);
    if (!amount || amount <= 0) return;

    setSending(true);
    setSendResult(null);
    setGeneratedToken("");
    setCopied(false);

    try {
      const prepared = await manager.ops.send.prepare({ mintUrl: MINT_URL, amount });
      const { token } = await manager.ops.send.execute(prepared.id);
      const encoded = getEncodedToken(token);
      setGeneratedToken(encoded);
      setSendResult({
        success: true,
        message: `Token created for ${amount} sats. Copy and share it.`,
      });
      setSendAmount("");
      // refresh history
      loadHistory(0);
    } catch (err) {
      setSendResult({
        success: false,
        message:
          err instanceof Error ? err.message : "Failed to create token",
      });
    } finally {
      setSending(false);
    }
  }, [manager, sendAmount, loadHistory]);

  /* ── copy token ──────────────────────────────────────── */

  const handleCopyToken = useCallback(async () => {
    if (!generatedToken) return;
    await navigator.clipboard.writeText(generatedToken);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  }, [generatedToken]);

  /* ── top-up handler ──────────────────────────────────── */

  const handleTopUp = useCallback(async () => {
    const amount = parseInt(topUpAmount, 10);
    if (!amount || amount <= 0) return;
    setInvoice("");
    setInvoiceCopied(false);
    try {
      const result = await topUp(amount);
      setInvoice(result.invoice);
    } catch {}
  }, [topUp, topUpAmount]);

  const handleCopyInvoice = useCallback(async () => {
    if (!invoice) return;
    await navigator.clipboard.writeText(invoice);
    setInvoiceCopied(true);
    setTimeout(() => setInvoiceCopied(false), 2000);
  }, [invoice]);

  useEffect(() => {
    if (status === "ready" && manager) {
      const id = requestAnimationFrame(() => loadHistory(0));
      return () => cancelAnimationFrame(id);
    }
  }, [status, manager, loadHistory]);

  /* ── nsec copy ───────────────────────────────────────── */

  const handleCopyNsec = useCallback(async () => {
    if (!nsec) return;
    await navigator.clipboard.writeText(nsec);
    setNsecCopied(true);
    setShowNsecConfirm(false);
    setTimeout(() => setNsecCopied(false), 2000);
  }, [nsec]);

  /* ── loading / error states ──────────────────────────── */

  if (status === "loading") {
    return (
        <div className="mx-auto max-w-[640px] py-8 md:py-12">
        <h1 className="text-2xl font-bold mb-6 flex items-center gap-3">Wallet <span className="rounded-md border border-yellow-800 bg-yellow-950/60 px-2 py-0.5 text-xs font-mono font-medium text-yellow-400">TESTNET</span></h1>
        <div className="rounded-xl border border-border bg-card p-8 sm:p-12 text-center">
          <div className="inline-block h-5 w-5 animate-spin rounded-full border-2 border-muted-foreground border-t-transparent" />
          <p className="text-sm text-muted-foreground mt-4">
            Initializing wallet...
          </p>
        </div>
      </div>
    );
  }

  if (status === "error") {
    return (
      <div className="mx-auto max-w-[640px] py-8 md:py-12">
        <h1 className="text-2xl font-bold mb-6 flex items-center gap-3">Wallet <span className="rounded-md border border-yellow-800 bg-yellow-950/60 px-2 py-0.5 text-xs font-mono font-medium text-yellow-400">TESTNET</span></h1>
        <div className="rounded-xl border border-red-800 bg-red-950/40 text-red-400 p-4 sm:p-6 text-center">
          <p className="text-sm mb-4">Wallet failed to initialize.</p>
          {walletError && (
            <p className="text-xs text-red-300/80 mb-4 font-mono break-all">
              {walletError}
            </p>
          )}
          <p className="text-xs text-red-400/70 mb-4">
            Try refreshing the page.
          </p>
          <Button
            variant="outline"
            onClick={() => window.location.reload()}
          >
            Retry
          </Button>
        </div>
      </div>
    );
  }

  /* ── main render ─────────────────────────────────────── */

  return (
    <div className="mx-auto max-w-[640px] py-8 md:py-12">
      <h1 className="text-2xl font-bold mb-6 flex items-center gap-3">
        Wallet
        <span className="rounded-md border border-yellow-800 bg-yellow-950/60 px-2 py-0.5 text-xs font-mono font-medium text-yellow-400">
          TESTNET
        </span>
      </h1>

      {/* ── Balance Card ──────────────────────────────── */}
      <div className="rounded-xl border border-border bg-card p-4 sm:p-6 mb-4">
        <div className="flex items-center justify-between mb-4">
          <div>
            <div className="text-sm text-muted-foreground mb-1">Balance</div>
            <div className="text-3xl font-bold tracking-tight">
              <span className={balance > 0 ? "text-emerald-400" : "text-yellow-400"}>
                {balance.toLocaleString()}
              </span>{" "}
              <span className="text-lg text-muted-foreground font-normal">sats</span>
            </div>
          </div>
          <StatusBadge status={mintOnline ? status : "degraded"} />
        </div>
        <div className="flex items-center justify-between">
          <div className="text-xs font-mono text-muted-foreground">
            {MINT_URL.replace("https://", "")}
          </div>
          <div className="text-[11px] text-muted-foreground/60">
            1 sat = 0.00000001 BTC
          </div>
        </div>
      </div>

      {!mintOnline && (
        <div className="rounded-xl border border-yellow-800 bg-yellow-950/40 text-yellow-400 p-4 mb-4 text-sm">
          <p className="font-medium mb-1">Mint temporarily unavailable</p>
          <p className="text-xs text-yellow-400/70 mb-3">
            The mint is not responding. Your wallet is ready — send and receive will work once it comes back online.
          </p>
          <Button
            variant="outline"
            size="sm"
            onClick={() => window.location.reload()}
          >
            Retry connection
          </Button>
        </div>
      )}

      {/* ── Top Up Section ─────────────────────────────── */}
      {mintOnline && (
        <div className="rounded-xl border border-border bg-card p-4 sm:p-6 mb-4">
          <h2 className="text-xs font-semibold mb-1 text-foreground/70 uppercase tracking-wider">
            Top Up via Lightning
          </h2>
          <p className="text-xs text-muted-foreground mb-4">
            Request a Lightning invoice. On testnut, it settles automatically.
          </p>

          <div className="flex flex-col sm:flex-row sm:items-center gap-3 mb-3">
            <div className="flex-1 relative">
              <Input
                type="number"
                min={1}
                value={topUpAmount}
                onChange={(e) => setTopUpAmount(e.target.value)}
                placeholder="Amount in sats"
                className="pr-12 font-mono min-h-[44px]"
              />
              <span className="absolute right-3 top-1/2 -translate-y-1/2 text-xs text-muted-foreground pointer-events-none">
                sats
              </span>
            </div>
            <Button
              onClick={handleTopUp}
              disabled={
                topUpState === 'requesting' ||
                topUpState === 'waiting' ||
                topUpState === 'minting' ||
                !topUpAmount ||
                parseInt(topUpAmount, 10) <= 0
              }
              className="min-h-[44px]"
            >
              {topUpState === 'requesting'
                ? "Requesting..."
                : topUpState === 'minting'
                  ? "Minting..."
                  : "Get Invoice"}
            </Button>
          </div>

          {topUpState === 'waiting' && invoice && (
            <div className="space-y-2">
              <p className="text-xs text-muted-foreground">
                Waiting for payment... (auto-settles on testnut)
              </p>
              <div className="rounded-lg border border-input bg-muted/50 p-3">
                <p className="text-xs font-mono text-foreground break-all leading-relaxed">
                  {invoice}
                </p>
              </div>
              <Button
                variant="outline"
                onClick={handleCopyInvoice}
                className="w-full"
              >
                {invoiceCopied ? "Copied!" : "Copy Invoice"}
              </Button>
            </div>
          )}

          {topUpState === 'done' && (
            <FeedbackBanner
              success
              message="Top-up successful! Balance updated."
            />
          )}

          {topUpState === 'error' && topUpError && (
            <FeedbackBanner
              success={false}
              message={topUpError}
            />
          )}
        </div>
      )}

      {/* ── Receive Section ───────────────────────────── */}
      <div className="rounded-xl border border-border bg-card p-4 sm:p-6 mb-4">
        <h2 className="text-xs font-semibold mb-1 text-foreground/70 uppercase tracking-wider">
          Receive Tokens
        </h2>
        <p className="text-xs text-muted-foreground mb-4">
          Paste a Cashu token (cashuA...) to add funds.
        </p>

        <textarea
          value={tokenInput}
          onChange={(e) => setTokenInput(e.target.value)}
          placeholder="Paste your Cashu token here…"
          rows={3}
          className="w-full rounded-lg border border-input bg-transparent px-3 py-2 text-sm font-mono text-foreground placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 outline-none resize-none mb-3"
        />

        {receiveResult && (
          <div className="mb-3">
            <FeedbackBanner
              success={receiveResult.success}
              message={receiveResult.message}
            />
          </div>
        )}

        <Button
          onClick={handleReceive}
          disabled={receiving || !tokenInput.trim() || !mintOnline}
          className="w-full"
        >
          {receiving ? "Receiving..." : "Receive"}
        </Button>
      </div>

      {/* ── Send Section ──────────────────────────────── */}
      <div className="rounded-xl border border-border bg-card p-4 sm:p-6 mb-4">
        <h2 className="text-xs font-semibold mb-1 text-foreground/70 uppercase tracking-wider">
          Send Tokens
        </h2>
        <p className="text-xs text-muted-foreground mb-4">
          Create a token to send sats to someone.
        </p>

        <div className="flex flex-col sm:flex-row sm:items-center gap-3 mb-3">
          <div className="flex-1 relative">
            <Input
              type="number"
              min={1}
              max={balance}
              value={sendAmount}
              onChange={(e) => setSendAmount(e.target.value)}
              placeholder="Amount to send"
              className="pr-12 font-mono min-h-[44px]"
            />
            <span className="absolute right-3 top-1/2 -translate-y-1/2 text-xs text-muted-foreground pointer-events-none">
              sats
            </span>
          </div>
          <Button
            onClick={handleSend}
            disabled={
              sending ||
              !sendAmount ||
              parseInt(sendAmount, 10) <= 0 ||
              parseInt(sendAmount, 10) > balance ||
              !mintOnline
            }
            className="min-h-[44px]"
          >
            {sending ? "Creating..." : "Create Token"}
          </Button>
        </div>

        {sendResult && (
          <div className="mb-3">
            <FeedbackBanner
              success={sendResult.success}
              message={sendResult.message}
            />
          </div>
        )}

        {generatedToken && (
          <div className="space-y-2">
            <div className="rounded-lg border border-input bg-muted/50 p-3">
              <p className="text-xs font-mono text-foreground break-all leading-relaxed">
                {generatedToken}
              </p>
            </div>
            <Button
              variant="outline"
              onClick={handleCopyToken}
              className="w-full"
            >
              {copied ? "Copied!" : "Copy to Clipboard"}
            </Button>
          </div>
        )}
      </div>

      {/* ── Transaction History ───────────────────────── */}
      <div className="rounded-xl border border-border bg-card p-4 sm:p-6 mb-4">
        <h2 className="text-xs font-semibold mb-4 text-foreground/70 uppercase tracking-wider">
          History
        </h2>

        {loadingHistory && history.length === 0 ? (
          <div className="text-center py-6">
            <div className="inline-block h-4 w-4 animate-spin rounded-full border-2 border-muted-foreground border-t-transparent" />
          </div>
        ) : history.length === 0 ? (
          <div className="text-center py-8">
            <p className="text-sm text-muted-foreground">No transactions yet</p>
          </div>
        ) : (
          <div className="space-y-0">
            {history.map((entry) => (
              <div
                key={entry.id}
                className="flex items-center justify-between py-3 border-b border-border last:border-b-0 gap-2"
              >
                <div className="flex items-center gap-3 min-w-0">
                  <span
                    className={`text-xs font-mono font-semibold shrink-0 ${historyColor(entry.type)}`}
                  >
                    {historySign(entry.type)}
                  </span>
                  <div className="min-w-0">
                    <div className="text-sm font-medium">
                      {historyLabel(entry.type)}
                    </div>
                    <div className="text-xs text-muted-foreground font-mono truncate">
                      {formatTimestamp(entry.createdAt)}
                    </div>
                  </div>
                </div>
                <div className="text-right shrink-0">
                  <span
                    className={`text-sm font-mono font-semibold ${historyColor(entry.type)}`}
                  >
                    {historySign(entry.type)}
                    {entry.amount.toLocaleString()} sats
                  </span>
                </div>
              </div>
            ))}

            {history.length >= 50 && (
              <div className="pt-3">
                <Button
                  variant="ghost"
                  size="sm"
                  className="w-full text-xs"
                  onClick={() => loadHistory(historyOffset + 50)}
                  disabled={loadingHistory}
                >
                  {loadingHistory ? "Loading..." : "Load More"}
                </Button>
              </div>
            )}
          </div>
        )}
      </div>

      {/* ── Identity Section ──────────────────────────── */}
      {initialized && (
        <div className="rounded-xl border border-border bg-card p-4 sm:p-6 mb-4">
          <h2 className="text-xs font-semibold mb-4 text-foreground/70 uppercase tracking-wider">
            Identity
          </h2>

          <div className="space-y-3">
            <div className="flex items-center justify-between gap-2">
              <div className="min-w-0">
                <div className="text-xs text-muted-foreground mb-0.5">
                  Public Key (npub)
                </div>
                <div className="text-sm font-mono truncate">
                  {truncateMiddle(npub, 12, 8)}
                </div>
              </div>
              <Button
                variant="ghost"
                size="xs"
                className="shrink-0 min-h-[44px] min-w-[44px]"
                onClick={async () => {
                  await navigator.clipboard.writeText(npub);
                }}
              >
                Copy
              </Button>
            </div>

            <div className="border-t border-border pt-3">
              <div className="flex items-center justify-between gap-2">
                <div className="min-w-0">
                  <div className="text-xs text-muted-foreground mb-0.5">
                    Private Key (nsec)
                  </div>
                  <div className="text-sm font-mono text-muted-foreground">
                    {"*".repeat(20)}
                  </div>
                </div>
                <Button
                  variant="ghost"
                  size="xs"
                  className="shrink-0 min-h-[44px] min-w-[44px]"
                  onClick={() => setShowNsecConfirm(true)}
                >
                  {nsecCopied ? "Copied!" : "Copy"}
                </Button>
              </div>
              <p className="text-[11px] text-muted-foreground/70 mt-2 leading-relaxed">
                Your private key controls your wallet and identity. Never share it
                with anyone.
              </p>
            </div>
          </div>
        </div>
      )}

      {/* ── nsec confirmation dialog ──────────────────── */}
      {showNsecConfirm && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
          <div className="mx-4 w-full max-w-sm rounded-xl border border-border bg-card p-6">
            <h3 className="text-base font-semibold mb-2">Copy Private Key?</h3>
            <p className="text-sm text-muted-foreground mb-6 leading-relaxed">
              This is your private key. <strong className="text-foreground">Never share it</strong> with
              anyone. Anyone with this key can access your wallet and identity.
            </p>
            <div className="flex gap-3">
              <Button
                variant="outline"
                className="flex-1"
                onClick={() => setShowNsecConfirm(false)}
              >
                Cancel
              </Button>
              <Button
                className="flex-1"
                onClick={handleCopyNsec}
              >
                Copy nsec
              </Button>
            </div>
          </div>
        </div>
      )}

      {/* ── Back link ─────────────────────────────────── */}
      <div className="text-center pt-4">
        <Link
          href="/"
          className="text-sm text-muted-foreground hover:text-foreground transition-colors"
        >
          Back to Home
        </Link>
      </div>
    </div>
  );
}

export default function WalletPage() {
  return (
    <ErrorBoundary>
      <SiteHeader />
      <main className="px-6 pb-16">
        <WalletContent />
      </main>
      <SiteFooter />
    </ErrorBoundary>
  );
}
