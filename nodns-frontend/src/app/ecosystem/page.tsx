import { SiteHeader } from "@/components/site-header";
import { SiteFooter } from "@/components/site-footer";
import { ErrorBoundary } from "@/components/error-boundary";

const TOOLS = [
  {
    name: "Registrar",
    url: "https://nodns-registrar.pages.dev",
    tag: "Live",
    desc: "Domain registration with zone discovery. Finds zones from kind:31990 Nostr events, validates via DNS TXT cross-signing, and surfaces testing/production status.",
    points: ["Zone discovery", "Testing status badges", "Multi-zone registration"],
  },
  {
    name: "Explorer",
    url: "https://nodns-explorer.pages.dev",
    tag: "Live",
    desc: "Real-time event feed from relay.cashu.email with payment badges, validity checks, and a Zone Monitor that compares records across relay, API, and DNS.",
    points: ["Live event feed", "3-source comparison", "Payment & validity badges"],
  },
  {
    name: "Bot API",
    url: "https://nodns.shop/api",
    tag: "REST",
    desc: "The bot exposes records, health checks, pricing, availability, and a canonical BIND zone export endpoint for any configured zone.",
    points: ["GET /api/records", "GET /api/zone/{zone}/export", "GET /api/health"],
  },
  {
    name: "CLI",
    url: "https://www.npmjs.com/package/nodns",
    tag: "npx",
    desc: "TypeScript CLI with zone validation, conformance testing, and zone-file export from any source. Run it ad-hoc with no install.",
    points: ["npx nodns zone-check", "npx nodns zone-export", "3-source conformance"],
  },
];

const ARCH_COMPONENTS = [
  {
    title: "Nostr Relays",
    accent: "text-foreground",
    lines: [
      "kind:31990 (zones)",
      "kind:11111 (records)",
      "relay.cashu.email",
      "damus, nos.lol, ...",
    ],
  },
  {
    title: "Registrar",
    accent: "text-primary",
    badge: "Discovery",
    lines: [
      "Subscribe to 31990",
      "Validate DNS TXT",
      "Cross-sign zones",
      "Surface status",
    ],
  },
  {
    title: "nodns-bot",
    accent: "text-primary",
    badge: "Rust",
    lines: [
      "Validate 11111",
      "Check authority",
      "Verify Cashu",
      "Push via DDNS",
    ],
  },
  {
    title: "DNS Backend",
    accent: "text-emerald-400",
    lines: [
      "Knot DNS (RFC 2136)",
      "Cloudflare API",
      "EPP (ccTLD)",
      "DNSSEC signed",
    ],
  },
  {
    title: "Explorer",
    accent: "text-blue-400",
    badge: "Monitor",
    lines: [
      "Relay feed",
      "API records",
      "Live DNS lookup",
      "Conformance diff",
    ],
  },
];

const DISCOVERY_STEPS = [
  {
    num: 1,
    title: "Publish Zone Event",
    desc: "A zone operator publishes a kind:31990 Nostr event announcing their zone. The event contains the zone name, endpoints, and a signed proof.",
  },
  {
    num: 2,
    title: "DNS TXT Cross-Sign",
    desc: "The same pubkey published in the Nostr event must appear in a DNS TXT record at the zone apex. This cross-references Nostr identity against DNS authority.",
  },
  {
    num: 3,
    title: "Registrar Discovers",
    desc: "The Registrar subscribes to kind:31990 events, fetches the TXT record, and verifies the pubkey matches. Only cross-signed zones appear in the registry.",
  },
  {
    num: 4,
    title: "Status Surfaced",
    desc: "A status tag on the event signals testing or production readiness. Users see an amber banner for testing zones and can filter accordingly.",
  },
];

const CONFORMANCE_SOURCES = [
  {
    label: "Relay",
    desc: "Raw kind:11111 events as published. The source of truth for what users intended.",
    color: "text-primary",
  },
  {
    label: "Bot API",
    desc: "Records the bot has accepted and processed. Shows what passed validation.",
    color: "text-amber-400",
  },
  {
    label: "Live DNS",
    desc: "Actual responses from the authoritative nameserver. The ground truth for resolvers.",
    color: "text-emerald-400",
  },
];

const LINKS = [
  {
    label: "Explorer",
    href: "https://nodns-explorer.pages.dev",
    desc: "Real-time event feed & zone monitor",
  },
  {
    label: "Registrar",
    href: "https://nodns-registrar.pages.dev",
    desc: "Discover zones & register domains",
  },
  {
    label: "CLI on npm",
    href: "https://www.npmjs.com/package/nodns",
    desc: "npx nodns — zone checks & export",
  },
  {
    label: "Bot API",
    href: "https://nodns.shop/api",
    desc: "Records, health, zone export",
  },
  {
    label: "Protocol Spec",
    href: "https://github.com/Amperstrand/nodns-poc/blob/main/docs/11-protocol-experimental-draft.md",
    desc: "Kind 11111 event specification",
  },
  {
    label: "Design Docs",
    href: "https://github.com/Amperstrand/nodns-poc/tree/main/docs",
    desc: "Architecture, consensus, bridge design",
  },
];

export default function EcosystemPage() {
  return (
    <ErrorBoundary>
      <SiteHeader />
      <main id="main-content" className="px-6 pb-16">
        <div className="mx-auto max-w-[960px] py-8 md:py-12">

          <section className="pb-10 pt-4">
            <p className="mb-3 text-sm font-medium uppercase tracking-wider text-primary">
              Ecosystem
            </p>
            <h1 className="mb-4 text-[2.5rem] font-extrabold leading-[1.1] tracking-tight max-[700px]:text-[1.75rem]">
              The tools around the{" "}
              <span className="text-primary">protocol</span>
            </h1>
            <p className="max-w-[640px] text-lg text-foreground/70">
              nodns started as a thought experiment — publish a Nostr event,
              get a DNS record. The ecosystem around it now spans zone
              discovery, real-time monitoring, conformance testing, and a
              CLI that catches discrepancies before users do.
            </p>
          </section>

          <section className="border-t border-border/40 py-14">
            <h2 className="mb-8 text-[1.75rem] font-bold tracking-tight">
              Live Tools
            </h2>
            <div className="grid grid-cols-[repeat(auto-fit,minmax(280px,1fr))] gap-4">
              {TOOLS.map((tool) => (
                <a
                  key={tool.name}
                  href={tool.url}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="group flex flex-col rounded-xl border border-border bg-card p-6 transition-colors hover:border-primary/40"
                >
                  <div className="mb-3 flex items-center justify-between">
                    <h3 className="text-lg font-semibold">{tool.name}</h3>
                    <span className="rounded bg-primary/10 px-2 py-0.5 text-[0.65rem] font-semibold uppercase tracking-wide text-primary">
                      {tool.tag}
                    </span>
                  </div>
                  <p className="mb-4 text-sm text-foreground/70 leading-relaxed">
                    {tool.desc}
                  </p>
                  <ul className="mt-auto space-y-1.5">
                    {tool.points.map((point) => (
                      <li
                        key={point}
                        className="flex items-center gap-2 text-xs text-muted-foreground"
                      >
                        <span className="text-primary/60">&rarr;</span>
                        {point}
                      </li>
                    ))}
                  </ul>
                  <span className="mt-4 inline-flex items-center gap-1 text-xs font-medium text-primary opacity-0 transition-opacity group-hover:opacity-100">
                    Open
                    <svg className="size-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                      <path d="M7 17 17 7M7 7h10v10" />
                    </svg>
                  </span>
                </a>
              ))}
            </div>
          </section>

          <section className="border-t border-border/40 py-14">
            <h2 className="mb-3 text-[1.75rem] font-bold tracking-tight">
              Architecture
            </h2>
            <p className="mb-8 max-w-[640px] text-foreground/70">
              Five independent components talk over open protocols. No
              component trusts another implicitly — every claim is
              cryptographically verified or cross-checked against a second
              source.
            </p>
            <div className="flex flex-col gap-2 max-[700px]:gap-1">
              <div className="flex gap-0 max-[700px]:flex-col max-[700px]:gap-2">
                {ARCH_COMPONENTS.map((comp, i) => (
                  <div key={comp.title} className="contents">
                    <div className="min-w-[150px] flex-1 rounded-xl border border-border bg-card p-4">
                      <div className="mb-2.5 flex items-center gap-1.5 border-b border-border pb-2">
                        <span className={`text-sm font-bold ${comp.accent}`}>
                          {comp.title}
                        </span>
                        {comp.badge && (
                          <span className="rounded bg-primary/10 px-1.5 py-0.5 text-[0.6rem] font-semibold text-primary">
                            {comp.badge}
                          </span>
                        )}
                      </div>
                      {comp.lines.map((line) => (
                        <div
                          key={line}
                          className="text-xs leading-relaxed text-muted-foreground"
                        >
                          {line}
                        </div>
                      ))}
                    </div>
                    {i < ARCH_COMPONENTS.length - 1 && (
                      <div className="flex items-center px-1 text-xl text-primary/40 max-[700px]:rotate-90 max-[700px]:justify-center max-[700px]:py-0.5">
                        &rarr;
                      </div>
                    )}
                  </div>
                ))}
              </div>
            </div>
            <p className="mt-6 text-sm text-foreground/70">
              The bridge architecture is distributed — each DNS backend
              (Knot DDNS, Cloudflare API, EPP) is a pluggable adapter behind
              a uniform interface. See{" "}
              <a
                href="https://github.com/Amperstrand/nodns-poc/tree/main/docs"
                target="_blank"
                rel="noopener noreferrer"
                className="font-mono text-primary hover:underline"
              >
                docs/40
              </a>{" "}
              for the full bridge design.
            </p>
          </section>

          <section className="border-t border-border/40 py-14">
            <h2 className="mb-3 text-[1.75rem] font-bold tracking-tight">
              Zone Discovery Flow
            </h2>
            <p className="mb-8 max-w-[640px] text-foreground/70">
              Zones are not hardcoded. Any operator can announce a zone via a
              Nostr event, and the Registrar verifies it independently
              before listing it.
            </p>
            <div className="grid grid-cols-[repeat(auto-fit,minmax(200px,1fr))] gap-4">
              {DISCOVERY_STEPS.map((step, i) => (
                <div key={step.num} className="relative">
                  {i < DISCOVERY_STEPS.length - 1 && (
                    <div className="hidden md:block absolute top-12 -right-2 z-10 text-primary/40">
                      <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
                        <path d="M2 8h10m0 0L8 4m4 4L8 12" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
                      </svg>
                    </div>
                  )}
                  <div className="h-full rounded-xl border border-border bg-card p-6 text-center transition-colors hover:border-primary/30">
                    <div className="mx-auto mb-3 flex h-10 w-10 items-center justify-center rounded-full bg-primary/10 text-base font-bold text-primary ring-1 ring-primary/20">
                      {step.num}
                    </div>
                    <h3 className="mb-2 text-base font-semibold">
                      {step.title}
                    </h3>
                    <p className="text-sm leading-relaxed text-foreground/70">
                      {step.desc}
                    </p>
                  </div>
                </div>
              ))}
            </div>
            <pre className="mt-8 overflow-x-auto rounded-lg border border-border bg-card p-4 text-[0.85rem] leading-relaxed">
              <code>{`# The kind:31990 event announcing a zone
{
  "kind": 31990,
  "tags": [
    ["zone",   "nodns.shop"],
    ["endpoint", "https://nodns.shop/api"],
    ["status", "testing", "testnet — do not rely on for production"]
  ]
}

# Cross-sign: same pubkey must appear in DNS
dig nodns.shop TXT +short
"npub1..."`}</code>
            </pre>
          </section>

          <section className="border-t border-border/40 py-14">
            <h2 className="mb-3 text-[1.75rem] font-bold tracking-tight">
              Conformance Testing
            </h2>
            <p className="mb-8 max-w-[640px] text-foreground/70">
              The CLI pulls records from three independent sources and diffs
              them. Discrepancies reveal unprocessed events, failed DDNS
              pushes, or stale caches &mdash; before they affect users.
            </p>
            <div className="grid grid-cols-[repeat(auto-fit,minmax(220px,1fr))] gap-4">
              {CONFORMANCE_SOURCES.map((src) => (
                <div
                  key={src.label}
                  className="rounded-xl border border-border bg-card p-5"
                >
                  <div className="mb-2 flex items-center gap-2">
                    <span className={`h-2 w-2 rounded-full bg-current ${src.color}`} />
                    <h3 className={`text-base font-semibold ${src.color}`}>
                      {src.label}
                    </h3>
                  </div>
                  <p className="text-sm leading-relaxed text-foreground/70">
                    {src.desc}
                  </p>
                </div>
              ))}
            </div>
            <pre className="mt-8 overflow-x-auto rounded-lg border border-border bg-card p-4 text-[0.85rem] leading-relaxed">
              <code>{`# Compare all three sources for a zone
npx nodns zone-check nodns.shop

  Source      Records   Match
  ──────────  ───────   ─────
  Relay       42        —
  Bot API     41        1 missing
  Live DNS    41        OK

  Discrepancy: relay has 1 unprocessed event (expired token)`}</code>
            </pre>
            <pre className="mt-6 overflow-x-auto rounded-lg border border-border bg-card p-4 text-[0.85rem] leading-relaxed">
              <code>{`# Export a canonical BIND zone file from any source
npx nodns zone-export nodns.shop --source dns > nodns.shop.zone

# Or pull from the bot API directly
curl -s https://nodns.shop/api/zone/nodns.shop/export`}</code>
            </pre>
          </section>

          <section className="border-t border-border/40 py-14">
            <h2 className="mb-3 text-[1.75rem] font-bold tracking-tight">
              Testing Status Signaling
            </h2>
            <p className="mb-6 max-w-[640px] text-foreground/70">
              A <code className="font-mono text-primary">status</code> tag on
              the kind:31990 event declares whether a zone is safe for
              production use. It is NIP-90 compatible and relay-filterable,
              so clients can automatically exclude testing zones.
            </p>
            <div className="grid grid-cols-[repeat(auto-fit,minmax(260px,1fr))] gap-4">
              <div className="rounded-xl border border-amber-900/40 bg-amber-950/20 p-5">
                <div className="mb-2 flex items-center gap-2">
                  <span className="h-2 w-2 rounded-full bg-amber-400 animate-pulse" />
                  <h3 className="text-base font-semibold text-amber-400">
                    Testing
                  </h3>
                </div>
                <p className="text-sm leading-relaxed text-foreground/70">
                  Amber banner shown. Records may be wiped, mint is
                  testnet, DNSSEC may be unsigned. Safe for experimentation.
                </p>
                <code className="mt-3 block font-mono text-xs text-amber-400/80">
                  [&quot;status&quot;, &quot;testing&quot;, &quot;reason&quot;]
                </code>
              </div>
              <div className="rounded-xl border border-emerald-900/40 bg-emerald-950/20 p-5">
                <div className="mb-2 flex items-center gap-2">
                  <span className="h-2 w-2 rounded-full bg-emerald-400" />
                  <h3 className="text-base font-semibold text-emerald-400">
                    Production
                  </h3>
                </div>
                <p className="text-sm leading-relaxed text-foreground/70">
                  No status tag (or explicit production). DNSSEC signed,
                  stable mint, records persist. Clients treat as durable.
                </p>
                <code className="mt-3 block font-mono text-xs text-emerald-400/80">
                  [&quot;status&quot;, &quot;production&quot;]
                </code>
              </div>
            </div>
            <div className="mt-6 rounded-xl border border-border bg-muted/30 p-4">
              <p className="text-sm text-foreground/70">
                <span className="font-semibold text-foreground">Active test zones:</span>{" "}
                <code className="font-mono text-primary">nodns.shop</code>{" "}
                (Knot DNS / RFC 2136) and{" "}
                <code className="font-mono text-primary">dns4sats.xyz</code>{" "}
                (Cloudflare API) &mdash; both verified testnet.
              </p>
            </div>
          </section>

          <section className="border-t border-border/40 py-14">
            <h2 className="mb-3 text-[1.75rem] font-bold tracking-tight">
              Nostr-over-DNS
              <span className="ml-3 rounded bg-muted px-2 py-0.5 align-middle text-[0.65rem] font-semibold uppercase tracking-wide text-muted-foreground">
                Experimental
              </span>
            </h2>
            <p className="mb-6 max-w-[640px] text-foreground/70">
              The core idea: store the signed Nostr event itself as a DNS TXT
              record at <code className="font-mono text-primary">_nodns.{`{name}`}</code>.
              This makes DNS self-validating &mdash; a resolver can verify a
              record&apos;s authenticity from the DNS response alone, with no
              relay access required.
            </p>
            <div className="grid grid-cols-[repeat(auto-fit,minmax(280px,1fr))] gap-4">
              <div className="rounded-xl border border-border bg-card p-6">
                <h3 className="mb-3 text-base font-semibold">
                  Why it matters
                </h3>
                <p className="text-sm leading-relaxed text-foreground/70">
                  DNS is cached everywhere. Embedding the proof in the record
                  means the chain of authority survives relay downtime,
                  censorship, and network partitions. The signature is
                  verifiable offline.
                </p>
              </div>
              <div className="rounded-xl border border-border bg-card p-6">
                <h3 className="mb-3 text-base font-semibold">
                  How to query
                </h3>
                <pre className="overflow-x-auto text-[0.8rem] leading-relaxed">
                  <code>{`# Fetch the embedded proof
dig _nodns.nodns.shop TXT +short

# Returns the base64-encoded
# kind:11111 Nostr event`}</code>
                </pre>
              </div>
            </div>
          </section>

          <section className="border-t border-border/40 py-14">
            <h2 className="mb-8 text-[1.75rem] font-bold tracking-tight">
              Explore
            </h2>
            <div className="grid grid-cols-[repeat(auto-fit,minmax(260px,1fr))] gap-3">
              {LINKS.map((link) => (
                <a
                  key={link.href}
                  href={link.href}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="group flex items-start justify-between gap-3 rounded-xl border border-border bg-card p-4 transition-colors hover:border-primary/40"
                >
                  <div>
                    <div className="text-sm font-semibold text-foreground">
                      {link.label}
                    </div>
                    <div className="mt-0.5 text-xs text-muted-foreground">
                      {link.desc}
                    </div>
                  </div>
                  <svg className="mt-0.5 size-4 shrink-0 text-muted-foreground transition-colors group-hover:text-primary" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                    <path d="M7 17 17 7M7 7h10v10" />
                  </svg>
                </a>
              ))}
            </div>
          </section>

          <div className="mt-12 flex justify-center">
            <a
              href="/"
              className="inline-flex items-center gap-1.5 rounded-lg border border-border bg-card px-4 py-2.5 text-sm text-foreground/60 transition-colors hover:border-primary/30 hover:text-foreground min-h-[44px]"
            >
              <svg className="size-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <path d="m18 15-6-6-6 6" />
              </svg>
              Back to top
            </a>
          </div>

        </div>
      </main>
      <SiteFooter />
    </ErrorBoundary>
  );
}
