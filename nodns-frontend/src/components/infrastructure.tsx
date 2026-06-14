import { RELAYS } from "@/lib/constants";

export function Infrastructure() {
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
    <section id="infra" className="px-6 py-16">
      <div className="mx-auto max-w-[960px]">
        <h2 className="mb-6 text-[1.75rem] font-bold tracking-tight">
          Infrastructure
        </h2>
        <div className="grid grid-cols-[repeat(auto-fit,minmax(220px,1fr))] gap-3">
          {infraCards.map((card) => (
            <div
              key={card.title}
              className="rounded-lg border border-border bg-card p-4"
            >
              <h4 className="mb-1 text-sm font-semibold text-foreground">
                {card.title}
              </h4>
              <p className="mb-0 text-xs text-foreground/60">{card.desc}</p>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}
