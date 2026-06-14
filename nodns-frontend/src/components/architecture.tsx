import { RELAYS } from "@/lib/constants";

export function Architecture() {
  const relayList = RELAYS.map((r) => r.replace("wss://", "")).join(", ");
  const infraCards = [
    {
      title: "nodns-bot",
      desc: "Rust daemon. Subscribes to Nostr relays, validates kind 11111 events, pushes to Knot DNS via DDNS.",
    },
    {
      title: "Knot DNS",
      desc: "Authoritative nameserver. Zone: nodns.shop. DDNS updates via RFC 2136.",
    },
    {
      title: "Relay Pool",
      desc: `${relayList}. Events propagate across all relays.`,
    },
    {
      title: "SQLite",
      desc: "Local state store. DNS records, delegations, registrar keys, rate limiting per npub.",
    },
  ];

  return (
    <div className="px-6 py-8">
      <div className="mx-auto max-w-[960px]">
        <div className="mt-4">
          <div className="flex gap-0 max-[700px]:flex-col max-[700px]:gap-2">
            {/* Nostr Relays */}
            <div className="min-w-[180px] flex-1 rounded-xl border border-border bg-card p-4">
              <div className="mb-2.5 border-b border-border pb-2 text-sm font-bold text-foreground">
                Nostr Relays
              </div>
              {RELAYS.map((relay) => (
                <div key={relay} className="text-xs text-foreground/60 leading-relaxed">
                  {relay}
                </div>
              ))}
            </div>

            {/* Arrow */}
            <div className="flex items-center px-2 text-xl text-muted-foreground max-[700px]:rotate-90 max-[700px]:justify-center max-[700px]:py-1">
              →
            </div>

            {/* nodns-bot */}
            <div className="min-w-[180px] flex-1 rounded-xl border border-border bg-card p-4">
              <div className="mb-2.5 border-b border-border pb-2 text-sm font-bold text-primary">
                nodns-bot{" "}
                <span className="ml-1.5 rounded bg-primary/10 px-1.5 py-0.5 text-[0.65rem] font-semibold text-primary">
                  Rust
                </span>
              </div>
              <div className="text-xs text-foreground/60 leading-relaxed">
                Subscribe to kind 11111
              </div>
              <div className="text-xs text-muted-foreground leading-relaxed">
                Validate signatures
              </div>
              <div className="text-xs text-muted-foreground leading-relaxed">
                Check authority &amp; delegation
              </div>
              <div className="text-xs text-muted-foreground leading-relaxed">
                Verify payments (Cashu)
              </div>
              <div className="text-xs text-muted-foreground leading-relaxed">
                Push via DDNS (RFC 2136)
              </div>
            </div>

            {/* Arrow */}
            <div className="flex items-center px-2 text-xl text-muted-foreground max-[700px]:rotate-90 max-[700px]:justify-center max-[700px]:py-1">
              →
            </div>

            {/* Knot DNS */}
            <div className="min-w-[180px] flex-1 rounded-xl border border-border bg-card p-4">
              <div className="mb-2.5 border-b border-border pb-2 text-sm font-bold text-emerald-400">
                Knot DNS
              </div>
              <div className="text-xs text-foreground/60 leading-relaxed">
                Authoritative nameserver
              </div>
              <div className="text-xs text-muted-foreground leading-relaxed">
                Zone: nodns.shop
              </div>
              <div className="text-xs text-muted-foreground leading-relaxed">
                Primary: ns1.nodns.shop
              </div>
              <div className="text-xs text-muted-foreground leading-relaxed">
                Secondary: puck.nether.net
              </div>
              <div className="text-xs text-muted-foreground leading-relaxed">
                TSIG-signed DDNS updates
              </div>
            </div>

            {/* Arrow */}
            <div className="flex items-center px-2 text-xl text-muted-foreground max-[700px]:rotate-90 max-[700px]:justify-center max-[700px]:py-1">
              →
            </div>

            {/* Internet */}
            <div className="min-w-[180px] flex-1 rounded-xl border border-border bg-card p-4">
              <div className="mb-2.5 border-b border-border pb-2 text-sm font-bold text-blue-400">
                Internet
              </div>
              <div className="text-xs text-foreground/60 leading-relaxed">
                Standard DNS queries
              </div>
              <div className="text-xs text-muted-foreground leading-relaxed">
                Any resolver, any device
              </div>
              <div className="text-xs text-muted-foreground leading-relaxed">
                Records live in seconds
              </div>
            </div>
          </div>
          <p className="mt-4 text-sm text-foreground/60">
            In the future, a ccTLD operator could run their own nodns-bot to
            enable Nostr-native DNS for an entire country-code TLD.
          </p>
        </div>

        {/* Infrastructure cards */}
        <div className="mt-10">
          <h3 className="mb-4 text-lg font-semibold">Infrastructure</h3>
          <div className="grid grid-cols-[repeat(auto-fit,minmax(220px,1fr))] gap-3">
            {infraCards.map((card) => (
              <div
                key={card.title}
                className="rounded-xl border border-border bg-card p-4"
              >
                <h4 className="mb-1 text-sm font-semibold text-foreground">
                  {card.title}
                </h4>
                <p className="mb-0 text-xs text-foreground/60">{card.desc}</p>
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
