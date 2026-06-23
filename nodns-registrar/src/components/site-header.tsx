"use client";

import { useState, useEffect } from "react";
import Link from "next/link";
import { useIdentity } from "@/contexts/IdentityContext";
import { useWallet } from "@/contexts/WalletContext";
import { Button } from "@/components/ui/button";
import { LoginModal } from "@/components/login-modal";

const BANNER_DISMISSED_KEY = "nodns_registrar_banner_dismissed";

function BetaBanner() {
  const [dismissed, setDismissed] = useState(true);

  useEffect(() => {
    setDismissed(localStorage.getItem(BANNER_DISMISSED_KEY) === "1");
  }, []);

  if (dismissed) return null;

  return (
    <div className="bg-destructive/15 border-b border-destructive/40">
      <div className="mx-auto flex max-w-6xl items-center justify-between gap-3 px-4 py-2">
        <p className="text-xs font-medium text-destructive">
          EXPERIMENTAL PILOT — Not production ready. Use test sats only (testnut.cashu.space). Never send real Cashu tokens.
        </p>
        <button
          className="text-muted-foreground hover:text-foreground"
          onClick={() => {
            localStorage.setItem(BANNER_DISMISSED_KEY, "1");
            setDismissed(true);
          }}
        >
          <span className="sr-only">Dismiss</span>
          ✕
        </button>
      </div>
    </div>
  );
}

export function SiteHeader() {
  const { session, npub, logout } = useIdentity();
  const { balance, ready } = useWallet();
  const [loginOpen, setLoginOpen] = useState(false);

  const shortNpub = npub
    ? `${npub.slice(0, 12)}...${npub.slice(-8)}`
    : null;

  return (
    <>
      <BetaBanner />
      <header className="sticky top-0 z-50 border-b border-border bg-background/95 backdrop-blur">
        <div className="mx-auto flex h-14 max-w-6xl items-center justify-between px-4">
          <Link href="/" className="flex items-center gap-2">
            <span className="text-lg font-bold tracking-tight">
              NoDNS
            </span>
            <span className="rounded bg-primary/15 px-1.5 py-0.5 text-xs font-medium text-primary">
              registrar
            </span>
            <span className="rounded bg-destructive/15 px-1.5 py-0.5 text-[10px] font-bold uppercase text-destructive">
              beta
            </span>
          </Link>

          <nav className="flex items-center gap-2">
            {session ? (
              <>
                <Link href="/dashboard">
                  <Button variant="ghost" size="sm">
                    Dashboard
                  </Button>
                </Link>
                <Link href="/wallet">
                  <Button variant="ghost" size="sm">
                    Wallet
                  </Button>
                </Link>
                <Link href="/wallet" className="hidden sm:block">
                  <span className="rounded-md bg-secondary px-2 py-1 font-mono text-xs text-primary">
                    {ready ? balance : "..."} sats
                  </span>
                </Link>
                <div className="flex items-center gap-2">
                  <code className="hidden text-xs text-muted-foreground sm:block">
                    {shortNpub}
                  </code>
                  <Button variant="outline" size="sm" onClick={logout}>
                    Logout
                  </Button>
                </div>
              </>
            ) : (
              <Button size="sm" onClick={() => setLoginOpen(true)}>
                Sign In
              </Button>
            )}
          </nav>
        </div>
      </header>

      <LoginModal open={loginOpen} onClose={() => setLoginOpen(false)} />
    </>
  );
}
