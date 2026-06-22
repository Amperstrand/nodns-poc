"use client";

import { useState, useEffect, useCallback } from "react";
import { useWallet } from "@/contexts/WalletContext";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Card, CardHeader, CardTitle, CardDescription, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";

function CopyableText({ value }: { value: string }) {
  const [copied, setCopied] = useState(false);
  const copy = useCallback(() => {
    navigator.clipboard.writeText(value);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  }, [value]);

  return (
    <div className="flex items-start gap-2 rounded-md border border-border bg-background p-3">
      <code className="flex-1 break-all font-mono text-xs leading-relaxed text-muted-foreground">
        {value}
      </code>
      <Button variant="outline" size="sm" onClick={copy} className="shrink-0">
        {copied ? "Copied" : "Copy"}
      </Button>
    </div>
  );
}

export default function WalletPage() {
  const wallet = useWallet();

  const [topupAmount, setTopupAmount] = useState("");
  const [invoice, setInvoice] = useState<string | null>(null);
  const [paymentHash, setPaymentHash] = useState<string | null>(null);
  const [invoiceStatus, setInvoiceStatus] = useState<
    "idle" | "generating" | "waiting" | "paid" | "error"
  >("idle");
  const [invoiceError, setInvoiceError] = useState<string | null>(null);

  const [sendAmount, setSendAmount] = useState("");
  const [sendResult, setSendResult] = useState<string | null>(null);
  const [sendStatus, setSendStatus] = useState<"idle" | "sending" | "done" | "error">("idle");
  const [sendError, setSendError] = useState<string | null>(null);

  const [receiveToken, setReceiveToken] = useState("");
  const [receiveResult, setReceiveResult] = useState<number | null>(null);
  const [receiveStatus, setReceiveStatus] = useState<
    "idle" | "receiving" | "done" | "error"
  >("idle");
  const [receiveError, setReceiveError] = useState<string | null>(null);

  const [prAmount, setPrAmount] = useState("");
  const [prDesc, setPrDesc] = useState("");
  const [prResult, setPrResult] = useState<string | null>(null);
  const [prStatus, setPrStatus] = useState<"idle" | "creating" | "done" | "error">("idle");
  const [prError, setPrError] = useState<string | null>(null);

  useEffect(() => {
    if (!paymentHash || invoiceStatus !== "waiting") return;
    let active = true;
    const interval = setInterval(async () => {
      try {
        const paid = await wallet.checkTopUpStatus(paymentHash);
        if (paid && active) {
          setInvoiceStatus("paid");
          await wallet.refreshBalance();
        }
      } catch {
        // transient poll errors ignored
      }
    }, 3000);
    return () => {
      active = false;
      clearInterval(interval);
    };
  }, [paymentHash, invoiceStatus, wallet]);

  const handleGenerateInvoice = useCallback(async () => {
    const amount = parseInt(topupAmount, 10);
    if (!amount || amount <= 0) return;
    setInvoiceStatus("generating");
    setInvoiceError(null);
    setInvoice(null);
    setPaymentHash(null);
    try {
      const result = await wallet.topUp(amount);
      setInvoice(result.invoice);
      setPaymentHash(result.operationId);
      setInvoiceStatus("waiting");
    } catch (e) {
      setInvoiceError(e instanceof Error ? e.message : "Failed to generate invoice");
      setInvoiceStatus("error");
    }
  }, [topupAmount, wallet]);

  const handleSend = useCallback(async () => {
    const amount = parseInt(sendAmount, 10);
    if (!amount || amount <= 0) return;
    setSendStatus("sending");
    setSendError(null);
    setSendResult(null);
    try {
      const token = await wallet.sendTokens(amount);
      setSendResult(token);
      setSendStatus("done");
    } catch (e) {
      setSendError(e instanceof Error ? e.message : "Failed to send tokens");
      setSendStatus("error");
    }
  }, [sendAmount, wallet]);

  const handleReceive = useCallback(async () => {
    const token = receiveToken.trim();
    if (!token) return;
    setReceiveStatus("receiving");
    setReceiveError(null);
    setReceiveResult(null);
    try {
      const amount = await wallet.receiveTokens(token);
      setReceiveResult(amount);
      setReceiveStatus("done");
      setReceiveToken("");
    } catch (e) {
      setReceiveError(e instanceof Error ? e.message : "Failed to receive tokens");
      setReceiveStatus("error");
    }
  }, [receiveToken, wallet]);

  const handleCreatePaymentRequest = useCallback(async () => {
    const amount = parseInt(prAmount, 10);
    if (!amount || amount <= 0) return;
    setPrStatus("creating");
    setPrError(null);
    setPrResult(null);
    try {
      const creqA = await wallet.createPaymentRequest(amount, prDesc);
      setPrResult(creqA);
      setPrStatus("done");
    } catch (e) {
      setPrError(e instanceof Error ? e.message : "Failed to create payment request");
      setPrStatus("error");
    }
  }, [prAmount, prDesc, wallet]);

  if (!wallet.ready) {
    return (
      <div className="flex min-h-[60vh] flex-col items-center justify-center gap-4">
        <div className="h-8 w-8 animate-spin rounded-full border-2 border-border border-t-primary" />
        <p className="text-sm text-muted-foreground">Initializing wallet...</p>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between gap-4">
        <h1 className="text-2xl font-bold tracking-tight">Wallet</h1>
        <Badge className="border-border font-mono text-xs text-muted-foreground">
          {wallet.mintUrl}
        </Badge>
      </div>

      <Card className="overflow-hidden border-border bg-gradient-to-br from-card to-muted">
        <CardContent className="p-8">
          <p className="text-sm font-medium text-muted-foreground">Balance</p>
          <div className="mt-2 flex items-baseline gap-3">
            <span className="font-mono text-5xl font-bold text-primary tabular-nums">
              {wallet.balance.toLocaleString()}
            </span>
            <span className="text-lg text-muted-foreground">Test sats</span>
          </div>
          <div className="mt-4 flex items-center gap-3">
            <Badge className="border-none bg-secondary text-secondary-foreground">
              testnut.cashu.space
            </Badge>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => wallet.refreshBalance()}
              className="text-muted-foreground"
            >
              Refresh
            </Button>
          </div>
        </CardContent>
      </Card>

      <div className="grid gap-6 md:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle>Top Up</CardTitle>
            <CardDescription>
              Generate a Lightning invoice to fund your wallet with test sats.
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="flex gap-2">
              <Input
                type="number"
                min="1"
                placeholder="Amount to top up"
                value={topupAmount}
                onChange={(e) => setTopupAmount(e.target.value)}
                disabled={invoiceStatus === "generating" || invoiceStatus === "waiting"}
              />
              <Button
                onClick={handleGenerateInvoice}
                disabled={
                  !topupAmount ||
                  invoiceStatus === "generating" ||
                  invoiceStatus === "waiting"
                }
                className="shrink-0"
              >
                {invoiceStatus === "generating" ? "Generating..." : "Generate Invoice"}
              </Button>
            </div>

            {invoiceStatus === "waiting" && invoice && (
              <div className="space-y-2">
                <CopyableText value={invoice} />
                <div className="flex items-center gap-3">
                  <a href={`lightning:${invoice}`}>
                    <Button variant="outline" size="sm">
                      Open in Wallet
                    </Button>
                  </a>
                  <span className="flex items-center gap-2 text-xs text-muted-foreground">
                    <span className="h-2 w-2 animate-live-pulse rounded-full bg-primary" />
                    Waiting for payment...
                  </span>
                </div>
              </div>
            )}

            {invoiceStatus === "paid" && (
              <div className="rounded-md border border-primary/30 bg-accent p-4">
                <p className="text-sm font-medium text-accent-foreground">
                  Invoice paid. Balance updated.
                </p>
                <Button
                  variant="ghost"
                  size="sm"
                  className="mt-2"
                  onClick={() => {
                    setInvoiceStatus("idle");
                    setInvoice(null);
                    setPaymentHash(null);
                    setTopupAmount("");
                  }}
                >
                  New Top-Up
                </Button>
              </div>
            )}

            {invoiceStatus === "error" && invoiceError && (
              <p className="text-sm text-destructive">{invoiceError}</p>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Send Tokens</CardTitle>
            <CardDescription>
              Mint Cashu tokens from your balance to send to another wallet.
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="flex gap-2">
              <Input
                type="number"
                min="1"
                placeholder="Amount to send"
                value={sendAmount}
                onChange={(e) => setSendAmount(e.target.value)}
                disabled={sendStatus === "sending"}
              />
              <Button
                onClick={handleSend}
                disabled={!sendAmount || sendStatus === "sending" || wallet.balance <= 0}
                className="shrink-0"
              >
                {sendStatus === "sending" ? "Sending..." : "Send"}
              </Button>
            </div>

            {sendStatus === "done" && sendResult && (
              <div className="space-y-2">
                <CopyableText value={sendResult} />
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => {
                    setSendResult(null);
                    setSendStatus("idle");
                    setSendAmount("");
                  }}
                >
                  Send More
                </Button>
              </div>
            )}

            {sendStatus === "error" && sendError && (
              <p className="text-sm text-destructive">{sendError}</p>
            )}

            {wallet.balance <= 0 && (
              <p className="text-xs text-muted-foreground">
                Balance is zero. Top up first to send tokens.
              </p>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Receive Tokens</CardTitle>
            <CardDescription>
              Redeem Cashu tokens sent to you into your wallet balance.
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <textarea
              className="flex min-h-[80px] w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50"
              placeholder="Paste a Cashu token (cashuA...)"
              value={receiveToken}
              onChange={(e) => setReceiveToken(e.target.value)}
              disabled={receiveStatus === "receiving"}
            />
            <Button
              onClick={handleReceive}
              disabled={!receiveToken.trim() || receiveStatus === "receiving"}
            >
              {receiveStatus === "receiving" ? "Receiving..." : "Receive"}
            </Button>

            {receiveStatus === "done" && receiveResult !== null && (
              <div className="rounded-md border border-primary/30 bg-accent p-4">
                <p className="text-sm font-medium text-accent-foreground">
                  Received {receiveResult.toLocaleString()} test sats.
                </p>
              </div>
            )}

            {receiveStatus === "error" && receiveError && (
              <p className="text-sm text-destructive">{receiveError}</p>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Payment Request</CardTitle>
            <CardDescription>
              Create a NUT-18 payment request that any Cashu wallet can pay.
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="grid gap-2 sm:grid-cols-[1fr_2fr]">
              <Input
                type="number"
                min="1"
                placeholder="Sats"
                value={prAmount}
                onChange={(e) => setPrAmount(e.target.value)}
                disabled={prStatus === "creating"}
              />
              <Input
                type="text"
                placeholder="Description (optional)"
                value={prDesc}
                onChange={(e) => setPrDesc(e.target.value)}
                disabled={prStatus === "creating"}
              />
            </div>
            <Button
              onClick={handleCreatePaymentRequest}
              disabled={!prAmount || prStatus === "creating"}
            >
              {prStatus === "creating" ? "Creating..." : "Create Payment Request"}
            </Button>

            {prStatus === "done" && prResult && (
              <div className="space-y-3">
                <CopyableText value={prResult} />
                <p className="text-xs text-muted-foreground">
                  Share this payment request — payer can scan with any Cashu wallet.
                </p>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => {
                    setPrResult(null);
                    setPrStatus("idle");
                    setPrAmount("");
                    setPrDesc("");
                  }}
                >
                  New Request
                </Button>
              </div>
            )}

            {prStatus === "error" && prError && (
              <p className="text-sm text-destructive">{prError}</p>
            )}
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
