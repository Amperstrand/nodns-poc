type RoadmapStatus = "done" | "wip" | "planned";

interface RoadmapEntry {
  title: string;
  description: string;
  status: RoadmapStatus;
}

const ROADMAP_ITEMS: RoadmapEntry[] = [
  {
    title: "nodns-bot",
    description:
      "Rust daemon bridging Nostr relays to Knot DNS via DDNS. Multi-zone, delegation, authority checking, Cashu payments.",
    status: "done",
  },
  {
    title: "nodns.shop live",
    description:
      "Public demo running nodns.shop zone with real DNS resolution",
    status: "done",
  },
  {
    title: "Web dashboard",
    description:
      "In-browser key generation and event publishing (this page)",
    status: "done",
  },
  {
    title: "Multi-zone support",
    description: "Ready for additional zones when needed",
    status: "done",
  },
  {
    title: "Custom names via delegation",
    description:
      "Registrar assigns alice.nodns.shop → npub, irrevocable within validity period",
    status: "done",
  },
  {
    title: "Cashu anti-spam payments",
    description:
      "250 sats per new record via Cashu ecash tokens",
    status: "wip",
  },
  {
    title: "ccTLD integration",
    description:
      "Enable country-code TLD operators to run their own nodns-bot for Nostr-native DNS at scale",
    status: "wip",
  },
  {
    title: "DNSSEC signing",
    description: "Sign zones, establish chain of trust",
    status: "planned",
  },
  {
    title: "Zap payments (NIP-57)",
    description: "Alternative payment flow via Lightning zaps",
    status: "planned",
  },
  {
    title: "Browser extension",
    description: "Manage NoDNS records directly from the browser",
    status: "planned",
  },
];

const STATUS_STYLES: Record<RoadmapStatus, string> = {
  done: "bg-emerald-950/30 text-emerald-400",
  wip: "bg-primary/10 text-primary",
  planned: "bg-secondary text-muted-foreground",
};

const STATUS_LABELS: Record<RoadmapStatus, string> = {
  done: "Done",
  wip: "WIP",
  planned: "Planned",
};

export function Roadmap() {
  return (
    <section id="roadmap" className="px-6 py-16">
      <div className="mx-auto max-w-[960px]">
        <h2 className="mb-6 text-[1.75rem] font-bold tracking-tight">
          Roadmap
        </h2>
        <div className="rounded-xl border border-border bg-card p-6">
          {ROADMAP_ITEMS.map((item, i) => (
            <div
              key={item.title}
              className={`flex items-center gap-3 py-3 ${i < ROADMAP_ITEMS.length - 1 ? "border-b border-border" : ""}`}
            >
              <div className="min-w-[48px]">
                <span
                  className={`inline-block rounded px-2 py-0.5 text-[0.7rem] font-semibold uppercase tracking-wider ${STATUS_STYLES[item.status]}`}
                >
                  {STATUS_LABELS[item.status]}
                </span>
              </div>
              <div>
                <strong>{item.title}</strong> &mdash; {item.description}
              </div>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}
