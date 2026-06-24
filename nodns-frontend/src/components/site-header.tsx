"use client";

import { useState } from "react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { useWallet } from "@/contexts/WalletContext";
import { useIdentity } from "@/contexts/IdentityContext";

const NAV_ITEMS = [
  { href: "/", label: "Home" },
  { href: "/records", label: "Records" },
  { href: "/dashboard", label: "Dashboard" },
  { href: "/learn", label: "Learn" },
  { href: "/ecosystem", label: "Ecosystem" },
  { href: "/discoveries", label: "Discoveries" },
  { href: "/wallet", label: "Wallet" },
];

function truncateNpub(npub: string, chars = 6): string {
  if (!npub) return "";
  return `${npub.slice(0, chars + 6)}...${npub.slice(-chars)}`;
}

export function SiteHeader() {
  const pathname = usePathname();
  const { balance, status } = useWallet();
  const { npub, initialized } = useIdentity();
  const [mobileOpen, setMobileOpen] = useState(false);

  const isActive = (href: string) => {
    if (href === "/") return pathname === "/";
    return pathname.startsWith(href);
  };

  return (
    <header className="sticky top-0 z-50 border-b border-border bg-background/95 backdrop-blur-[12px]">
      <div className="mx-auto flex max-w-[960px] items-center justify-between gap-3 px-6 py-4">
        {/* Logo */}
        <Link href="/" className="text-xl font-bold tracking-tight shrink-0">
          No<span className="text-primary">DNS</span>
          <span className="text-muted-foreground text-base">.shop</span>
        </Link>

        {/* Desktop nav */}
        <nav className="hidden md:flex items-center gap-1">
          {NAV_ITEMS.map((item) => (
            <Link
              key={item.href}
              href={item.href}
              className={`rounded-md px-3 py-1.5 text-sm transition-colors ${
                isActive(item.href)
                  ? "text-foreground bg-secondary"
                  : "text-muted-foreground hover:bg-secondary hover:text-foreground"
              }`}
            >
              {item.label}
            </Link>
          ))}
        </nav>

        {/* Wallet info (desktop) */}
        <div className="hidden md:flex items-center gap-3 shrink-0">
          {initialized && (
            <span className="text-xs font-mono text-muted-foreground">
              {truncateNpub(npub)}
            </span>
          )}
          <div
            className={`flex items-center gap-1.5 rounded-md border px-2.5 py-1 text-xs font-mono ${
              status === "ready"
                ? "border-emerald-800 bg-emerald-950/60 text-emerald-400"
                : status === "error"
                  ? "border-red-800 bg-red-950/60 text-red-400"
                  : "border-yellow-800 bg-yellow-950/60 text-yellow-400"
            }`}
          >
            <span
              className={`inline-block h-1.5 w-1.5 rounded-full ${
                status === "ready"
                  ? "bg-emerald-400"
                  : status === "error"
                    ? "bg-red-400"
                    : "bg-yellow-400 animate-pulse"
              }`}
            />
            {balance} sats
          </div>
        </div>

        {/* Mobile hamburger */}
        <button
          onClick={() => setMobileOpen(!mobileOpen)}
          className="md:hidden flex flex-col gap-1 p-1"
          aria-label="Toggle menu"
        >
          <span
            className={`block h-0.5 w-5 bg-foreground transition-all ${
              mobileOpen ? "rotate-45 translate-y-1.5" : ""
            }`}
          />
          <span
            className={`block h-0.5 w-5 bg-foreground transition-all ${
              mobileOpen ? "opacity-0" : ""
            }`}
          />
          <span
            className={`block h-0.5 w-5 bg-foreground transition-all ${
              mobileOpen ? "-rotate-45 -translate-y-1.5" : ""
            }`}
          />
        </button>
      </div>

      {/* Mobile menu */}
      {mobileOpen && (
        <div className="md:hidden border-t border-border bg-background">
          <nav className="flex flex-col px-6 py-3 gap-1">
            {NAV_ITEMS.map((item) => (
              <Link
                key={item.href}
                href={item.href}
                onClick={() => setMobileOpen(false)}
                className={`rounded-md px-3 py-2 text-sm transition-colors ${
                  isActive(item.href)
                    ? "text-foreground bg-secondary"
                    : "text-muted-foreground hover:bg-secondary hover:text-foreground"
                }`}
              >
                {item.label}
              </Link>
            ))}
          </nav>
          <div className="flex items-center gap-3 px-6 pb-3 border-t border-border pt-3">
            {initialized && (
              <span className="text-xs font-mono text-muted-foreground">
                {truncateNpub(npub)}
              </span>
            )}
            <span
              className={`text-xs font-mono ${
                status === "ready"
                  ? "text-emerald-400"
                  : status === "error"
                    ? "text-red-400"
                    : "text-yellow-400"
              }`}
            >
              {balance} sats
            </span>
          </div>
        </div>
      )}
    </header>
  );
}
