"use client";

import { useState, useCallback, type FormEvent } from "react";
import { useRouter } from "next/navigation";
import { DEFAULT_ZONE } from "@/lib/constants";

export function Hero() {
  const [query, setQuery] = useState("");
  const router = useRouter();

  const handleSearch = useCallback(
    (e: FormEvent) => {
      e.preventDefault();
      const clean = query.trim().toLowerCase().replace(/\s+/g, "");
      if (!clean) return;
      // Strip .nodns.shop suffix if user typed the full domain
      const suffix = `.${DEFAULT_ZONE}`;
      const name = clean.endsWith(suffix)
        ? clean.slice(0, -suffix.length)
        : clean;
      if (!name) return;
      router.push(`/search?q=${encodeURIComponent(name)}`);
    },
    [query, router],
  );

  return (
    <section className="px-6 pb-20 pt-28 text-center">
      <div className="mx-auto max-w-[720px]">
        <h1 className="mb-3 text-[2.75rem] font-extrabold leading-[1.1] tracking-tight text-foreground max-[700px]:text-[1.75rem]">
          Your domain.
          <br />
          <span className="text-primary">No registrar needed.</span>
        </h1>
        <p className="mx-auto mb-10 max-w-[520px] text-lg text-muted-foreground">
          Register a .nodns.shop subdomain instantly. Pay with Cashu sats.
          Records propagate via Nostr in seconds.
        </p>

        {/* Search bar */}
        <form onSubmit={handleSearch} className="relative mx-auto max-w-[560px]">
          <div className="flex items-center rounded-xl border border-border bg-card overflow-hidden shadow-lg shadow-black/30 focus-within:border-primary/50 focus-within:ring-2 focus-within:ring-primary/20 transition-all">
            <input
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Find your .nodns.shop domain"
              className="flex-1 min-w-0 bg-transparent px-4 py-3.5 text-base sm:px-5 sm:py-4 sm:text-lg text-foreground placeholder:text-muted-foreground outline-none"
              autoFocus
            />
            <button
              type="submit"
              className="shrink-0 m-1.5 rounded-lg bg-primary px-5 py-2.5 text-sm font-semibold text-primary-foreground hover:bg-primary/80 transition-colors sm:px-6 min-h-[44px]"
            >
              <span className="hidden sm:inline">Search</span>
              <span className="sm:hidden">Go</span>
            </button>
          </div>
          <p className="mt-3 text-xs text-muted-foreground">
            e.g., <span className="text-foreground/70">alice</span>.nodns.shop
          </p>
        </form>

        {/* Pricing hint */}
        <div className="mt-8 flex flex-wrap items-center justify-center gap-4 text-xs text-muted-foreground">
          <span className="flex items-center gap-1.5">
            <span className="font-mono text-foreground">1-3 chars</span> 200 sats
          </span>
          <span className="text-border">·</span>
          <span className="flex items-center gap-1.5">
            <span className="font-mono text-foreground">4-6 chars</span> 20 sats
          </span>
          <span className="text-border">·</span>
          <span className="flex items-center gap-1.5">
            <span className="font-mono text-foreground">7+ chars</span> 4 sats
          </span>
        </div>

        {/* Trust microcopy */}
        <div className="mt-4 flex flex-wrap items-center justify-center gap-2 text-xs text-muted-foreground/60">
          <span>No account needed</span>
          <span className="text-border">·</span>
          <span>No email</span>
          <span className="text-border">·</span>
          <span>Just sats</span>
        </div>
      </div>
    </section>
  );
}
