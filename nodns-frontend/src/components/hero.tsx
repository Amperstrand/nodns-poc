"use client";

import { useCallback, useEffect, useRef, useState } from "react";

const DEMO_MESSAGES = [
  "NoDNS was here! 🌍",
  "DNS from Nostr ✨",
  "Hello from NoDNS! 👋",
  "Decentralized DNS rocks 🚀",
];

function getRandomMessage(): string {
  const hex = Math.random().toString(16).slice(2, 10);
  return DEMO_MESSAGES[Math.floor(Math.random() * DEMO_MESSAGES.length)] + " " + hex;
}

export function Hero() {
  const [launching, setLaunching] = useState(false);
  const timeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    return () => {
      if (timeoutRef.current) clearTimeout(timeoutRef.current);
    };
  }, []);

  const handleTryIt = useCallback(() => {
    if (launching) return;
    setLaunching(true);

    const message = getRandomMessage();

    document.getElementById("dashboard")?.scrollIntoView({ behavior: "smooth" });

    timeoutRef.current = setTimeout(() => {
      window.dispatchEvent(
        new CustomEvent("nodns-demo-publish", {
          detail: { message },
        }),
      );
      setLaunching(false);
    }, 600);
  }, [launching]);

  return (
    <section className="px-6 pb-16 pt-24 text-center">
      <div className="mx-auto max-w-[960px]">
        <h2 className="mb-4 text-[2.5rem] font-extrabold leading-[1.15] tracking-tight text-[#e0e0e0] max-[700px]:text-[1.75rem]">
          DNS Records from
          <br />
          Nostr Events
        </h2>
        <p className="mx-auto mb-8 max-w-[640px] text-lg text-[#bbb]">
          No registrars. No control panels. Publish a cryptographically-signed
          event to Nostr, and your DNS records propagate globally in seconds.
        </p>

        <button
          onClick={handleTryIt}
          disabled={launching}
          className="animate-cta-glow mx-auto mb-8 inline-block rounded-xl bg-[#ff6b35] px-10 py-4 text-xl font-bold tracking-wide text-white transition-all hover:scale-[1.03] hover:bg-[#ff7f4f] active:scale-[0.98] disabled:opacity-60 max-[700px]:px-7 max-[700px]:py-3.5 max-[700px]:text-lg"
        >
          {launching ? "Launching..." : "Try it in 5 seconds →"}
        </button>

        <div>
          <code className="inline-block rounded-lg border border-[#222] bg-[#141414] px-5 py-3 font-mono text-[0.95rem] text-[#ff6b35]">
            your-key.nodns.shop
          </code>
        </div>
        <p className="mt-3 text-sm text-[#666]">
          Powered by Nostr &middot; Resolves globally via standard DNS
        </p>
        <div className="mt-4 flex flex-wrap items-center justify-center gap-1.5 text-xs text-[#666]">
          <span className="inline-block h-1.5 w-1.5 rounded-full bg-[#2ecc71]" />
          <span className="font-medium text-[#e0e0e0]/70">Primary:</span>
          <span>ns1.nodns.shop (46.224.104.12)</span>
          <span className="mx-1 text-[#222]">&middot;</span>
          <span className="inline-block h-1.5 w-1.5 rounded-full bg-[#2ecc71]" />
          <span className="font-medium text-[#e0e0e0]/70">Secondary:</span>
          <span>puck.nether.net (204.42.254.5)</span>
        </div>
      </div>
    </section>
  );
}
