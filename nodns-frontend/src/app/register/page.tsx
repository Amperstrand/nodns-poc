"use client";

import { Suspense, useCallback, useMemo, useState } from "react";
import { useSearchParams } from "next/navigation";
import Link from "next/link";
import { SiteHeader } from "@/components/site-header";
import { SiteFooter } from "@/components/site-footer";
import { ErrorBoundary } from "@/components/error-boundary";
import { MINT_URL } from "@/lib/wallet";
import { getPriceForName, sanitizeName, toFqdn } from "@/lib/pricing";
import { publishDnsEvent, keyPairFromNsec } from "@/lib/nostr";
import { useWallet } from "@/contexts/WalletContext";
import { useIdentity } from "@/contexts/IdentityContext";
import { getEncodedToken } from "coco-cashu-core";
import type { KeyPair } from "@/lib/types";

type Step = "review" | "paying" | "success" | "error";

function RegisterContent() {
  const searchParams = useSearchParams();
  const nameParam = searchParams.get("name") || "";

  const { manager, status: walletStatus, balance } = useWallet();
  const { nsec } = useIdentity();

  const name = useMemo(() => sanitizeName(nameParam), [nameParam]);
  const [step, setStep] = useState<Step>("review");
  const [errorMsg, setErrorMsg] = useState("");
  const [txEventId, setTxEventId] = useState<string | null>(null);

  const price = name ? getPriceForName(name) : 0;
  const fqdn = name ? toFqdn(name) : "";
  const sufficient = balance >= price;

  const handleRegister = useCallback(async () => {
    if (!manager || !nsec || !name) return;
    if (balance < price) return;

    setStep("paying");
    setErrorMsg("");

    try {
      const tokenObj = await manager.wallet.send(MINT_URL, price);
      const cashuToken = getEncodedToken(tokenObj);

      const keyPair: KeyPair = keyPairFromNsec(nsec);

      const event = await publishDnsEvent(
        [
          {
            type: "TXT",
            name: name,
            value: "registered via nodns.shop",
            ttl: 3600,
          },
        ],
        keyPair.secretKey,
        cashuToken,
        MINT_URL,
        price,
      );

      setTxEventId(event.id);

      setStep("success");
    } catch (err) {
      setStep("error");
      setErrorMsg(err instanceof Error ? err.message : "Payment failed");
    }
  }, [manager, nsec, name, price, balance]);

  if (!nameParam) {
    return (
      <div className="mx-auto max-w-[560px] py-20 text-center">
        <h1 className="text-2xl font-bold mb-3">No domain selected</h1>
        <p className="text-muted-foreground mb-6">
          Search for a domain first to register it.
        </p>
        <Link
          href="/"
          className="inline-flex items-center justify-center rounded-lg bg-primary px-6 py-2.5 text-sm font-semibold text-primary-foreground hover:bg-primary/80 transition-colors"
        >
          ← Search for a domain
        </Link>
      </div>
    );
  }

  return (
    <div className="mx-auto max-w-[560px] py-12">
      {/* Success state */}
      {step === "success" && (
        <div className="rounded-xl border border-emerald-800 bg-emerald-950/40 p-8 text-center mb-6">
          <div className="text-4xl mb-3">🎉</div>
          <h1 className="text-2xl font-bold text-emerald-400 mb-2">Domain Registered!</h1>
          <p className="text-muted-foreground mb-4">
            <span className="font-mono text-foreground">{fqdn}</span> is now yours.
          </p>
          {txEventId && (
            <p className="text-xs font-mono text-muted-foreground mb-6 truncate">
              Event: {txEventId}
            </p>
          )}
          <div className="flex items-center justify-center gap-3">
            <Link
              href="/dashboard"
              className="rounded-lg bg-primary px-5 py-2 text-sm font-semibold text-primary-foreground hover:bg-primary/80 transition-colors"
            >
              Go to Dashboard
            </Link>
            <Link
              href="/"
              className="rounded-lg border border-border px-5 py-2 text-sm font-medium text-foreground hover:bg-secondary transition-colors"
            >
              Register Another
            </Link>
          </div>
        </div>
      )}

      {/* Order summary */}
      {step !== "success" && (
        <div className="rounded-xl border border-border bg-card p-6 mb-6">
          <h1 className="text-lg font-semibold mb-5">Order Summary</h1>

          <div className="space-y-3 mb-6">
            <div className="flex justify-between text-sm">
              <span className="text-muted-foreground">Domain</span>
              <span className="font-mono text-foreground">{fqdn}</span>
            </div>
            <div className="flex justify-between text-sm">
              <span className="text-muted-foreground">Duration</span>
              <span className="text-foreground">1 year</span>
            </div>
            <div className="border-t border-border pt-3 flex justify-between">
              <span className="text-sm text-muted-foreground">Total</span>
              <span className="text-xl font-bold text-foreground">{price} sats</span>
            </div>
          </div>
        </div>
      )}

      {/* Wallet / Payment section */}
      {step === "review" && (
        <div className="rounded-xl border border-border bg-card p-6 mb-6">
          <h2 className="text-sm font-semibold mb-4 text-muted-foreground uppercase tracking-wider">
            Payment
          </h2>

          <div className="flex justify-between text-sm mb-4">
            <span className="text-muted-foreground">Wallet balance</span>
            <span className={`font-mono ${sufficient ? "text-emerald-400" : "text-red-400"}`}>
              {balance} sats
            </span>
          </div>

          {walletStatus !== "ready" && (
            <div className="rounded-lg border border-yellow-800 bg-yellow-950/40 px-4 py-3 text-sm text-yellow-400 mb-4">
              Wallet is {walletStatus}. Please wait...
            </div>
          )}

          {!sufficient && walletStatus === "ready" && (
            <div className="rounded-lg border border-red-800 bg-red-950/40 px-4 py-3 text-sm text-red-400 mb-4">
              Insufficient balance. You need {price} sats but have {balance} sats.
              <Link href="/wallet" className="block mt-2 text-primary hover:underline">
                Add funds →
              </Link>
            </div>
          )}

          {sufficient && walletStatus === "ready" && (
            <button
              onClick={handleRegister}
              className="w-full rounded-lg bg-primary py-3 text-sm font-semibold text-primary-foreground hover:bg-primary/80 transition-colors"
            >
              Pay {price} sats & Register
            </button>
          )}
        </div>
      )}

      {/* Paying state */}
      {step === "paying" && (
        <div className="rounded-xl border border-primary/30 bg-primary/5 p-8 text-center">
          <div className="animate-pulse text-2xl mb-3">⚡</div>
          <h2 className="text-lg font-semibold mb-2">Processing Payment</h2>
          <p className="text-sm text-muted-foreground">
            Sending {price} sats and publishing Nostr event...
          </p>
        </div>
      )}

      {/* Error state */}
      {step === "error" && (
        <div className="rounded-xl border border-red-800 bg-red-950/40 p-6 text-center">
          <h2 className="text-lg font-semibold text-red-400 mb-2">Payment Failed</h2>
          <p className="text-sm text-muted-foreground mb-4">{errorMsg}</p>
          <button
            onClick={() => setStep("review")}
            className="rounded-lg border border-border px-5 py-2 text-sm font-medium text-foreground hover:bg-secondary transition-colors"
          >
            Try Again
          </button>
        </div>
      )}
    </div>
  );
}

export default function RegisterPage() {
  return (
    <ErrorBoundary>
      <SiteHeader />
      <main className="px-6 pb-16">
        <Suspense
          fallback={
            <div className="mx-auto max-w-[560px] py-20 text-center text-muted-foreground animate-pulse">
              Loading...
            </div>
          }
        >
          <RegisterContent />
        </Suspense>
      </main>
      <SiteFooter />
    </ErrorBoundary>
  );
}
