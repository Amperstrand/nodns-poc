"use client";

import { DualLookupDemo } from "@/components/dual-lookup-demo";

export function Discoveries() {
  return (
    <section className="px-6 py-16">
      <div className="mx-auto max-w-[960px]">
        <h1 className="mb-2 text-[1.75rem] font-bold tracking-tight">
          Discoveries
        </h1>
        <p className="mb-8 text-lg text-foreground/60">
          Things we learned while building NoDNS — the unexpected properties,
          emergent behaviors, and architectural insights.
        </p>

        <div className="mb-5 rounded-xl border border-border bg-card p-6">
          <h3 className="mb-3 text-lg font-semibold">
            The Zone-Agnostic Wire Format
          </h3>
          <p className="mb-3 text-foreground/60">
            Kind 11111 record tags contain no zone information. The wire format
            is purely about the record itself — type, name, data, and metadata.
            For <code className="text-primary">$npub</code> registrations, the
            same event applies to{" "}
            <code className="text-primary">.nostr</code>,{" "}
            <code className="text-primary">.nodns.shop</code>, or any zone
            running a NoDNS bot — zone assignment is infrastructure, not
            protocol.
          </p>
          <div className="my-4 rounded-lg border border-border bg-background p-4">
            <pre className="overflow-x-auto font-mono text-[0.85rem] text-primary">
{`["record", "A", "", "1.2.3.4", "", "", "", "", "", "", "3600"]`}
            </pre>
          </div>
          <p className="text-foreground/60">
            For <code className="text-primary">$string</code> registrations,
            zone awareness lives in the claim and delegation tags — e.g.{" "}
            <code className="text-primary">
              [&quot;claim&quot;, &quot;alice&quot;, &quot;nodns.shop&quot;,
              ...]
            </code>{" "}
            explicitly references the zone to establish ownership. The record
            wire format itself stays zone-agnostic regardless of name class.
          </p>
        </div>

        <div className="mb-5 rounded-xl border border-border bg-card p-6">
          <h3 className="mb-3 text-lg font-semibold">
            The .nostr TLD
          </h3>
          <p className="mb-3 text-foreground/60">
            The <code className="text-primary">.nostr</code> TLD supports only{" "}
            <code className="text-primary">$npub.nostr</code> — cryptographic
            ownership tied to the Nostr keypair. There is no{" "}
            <code className="text-primary">$string.nostr</code> because there is
            no consensus mechanism for string ownership in a decentralized
            namespace. Non-npub queries return NXDOMAIN.
          </p>
          <p className="mb-3 text-foreground/60">
            Resolves via DNS-over-HTTPS at{" "}
            <code className="text-primary">dns.nodns.shop/dns-query</code> or
            through a local resolver. We are the only publicly available NoDNS
            server — Arjen&apos;s{" "}
            <a
              href="https://gitworkshop.dev/npub1hw6amg8p24ne08c9gdq8hhpqx0t0pwanpae9z25crn7m9uy7yarse465gr/nos.lol/nodns-nameserver"
              className="text-primary hover:underline"
              target="_blank"
              rel="noopener noreferrer"
            >
              nodns-nameserver
            </a>{" "}
            is a local resolver.
          </p>
        </div>

        <div className="mb-5 rounded-xl border border-border bg-card p-6">
          <h3 className="mb-3 text-lg font-semibold">
            DNS-over-HTTPS Resolver
          </h3>
          <p className="mb-3 text-foreground/60">
            Our public DoH endpoint at{" "}
            <code className="text-primary">
              https://dns.nodns.shop/dns-query
            </code>{" "}
            routes <code className="text-primary">.nostr</code> and{" "}
            <code className="text-primary">dns4sats.xyz</code> queries to our
            local Knot DNS. Everything else goes to Google and Cloudflare DoH
            upstreams.
          </p>
          <p className="mb-3 text-foreground/60">
            Users can configure their system DNS to use this endpoint for
            one-click <code className="text-primary">.nostr</code> resolution.
            No browser extension, no special software — just point your DNS at
            us.
          </p>
          <div className="my-4 rounded-lg border border-border bg-background p-4">
            <pre className="overflow-x-auto font-mono text-[0.85rem] text-primary">
{`https://dns.nodns.shop/dns-query`}
            </pre>
          </div>
        </div>

        <div className="mb-5 rounded-xl border border-border bg-card p-6">
          <h3 className="mb-3 text-lg font-semibold">
            Demo 1: TXT Record — &ldquo;Liar!&rdquo; vs &ldquo;No you!&rdquo;
          </h3>
          <p className="mb-3 text-foreground/60">
            The Bitcoin &ldquo;Liar / No you&rdquo; meme, rendered in DNS. A
            TXT record on{" "}
            <code className="text-primary break-all">
              truth.npub10mluej6gljwsjx5v4dnr54n9y0yzf8thwr2l60p3e94q72udh8ksz6uw6q.dns4sats.xyz
            </code>{" "}
            returns different values depending on which resolver you ask.
          </p>
          <p className="mb-3 text-foreground/60">
            Standard DNS (Google/Cloudflare) returns{" "}
            <code className="text-primary">&quot;Liar!&quot;</code> because the
            domain&apos;s authoritative path goes through Cloudflare, where we
            set that TXT record. NoDNS resolves from our VPS Knot DNS, which
            has the counter-record{" "}
            <code className="text-primary">&quot;No you!&quot;</code> —
            published by the Nostr keyholder.
          </p>
          <div className="my-4 space-y-3">
            <div className="rounded-lg border border-border bg-background p-4">
              <p className="mb-1 text-xs font-semibold uppercase tracking-wider text-red-400">
                Standard DNS (Cloudflare path)
              </p>
              <pre className="overflow-x-auto font-mono text-[0.85rem] text-primary">
{`dig truth.npub10ml...uw6q.dns4sats.xyz TXT @8.8.8.8

;; ANSWER SECTION:
truth.npub10ml...dns4sats.xyz. 300 IN TXT "Liar!"`}
              </pre>
            </div>
            <div className="rounded-lg border border-border bg-background p-4">
              <p className="mb-1 text-xs font-semibold uppercase tracking-wider text-emerald-400">
                NoDNS (Knot DNS on VPS)
              </p>
              <pre className="overflow-x-auto font-mono text-[0.85rem] text-primary">
{`dig truth.npub10ml...uw6q.dns4sats.xyz TXT @dns.nodns.shop

;; ANSWER SECTION:
truth.npub10ml...dns4sats.xyz. 300 IN TXT "No you!"`}
              </pre>
            </div>
          </div>
          <p className="text-sm text-foreground/60">
            Try it yourself — replace the truncated npub with the full one and
            query both resolvers.
          </p>
        </div>

        <div className="mb-5 rounded-xl border border-border bg-card p-6">
          <h3 className="mb-3 text-lg font-semibold">
            Demo 2: Web — &ldquo;Respect my authority&rdquo; vs &ldquo;Liar&rdquo; &ldquo;No you&rdquo;
          </h3>
          <p className="mb-3 text-foreground/60">
            The same domain resolves to entirely different web pages depending on
            which DNS resolver you use. Standard DNS returns Cloudflare IPs
            (the registrar&apos;s page saying{" "}
            <code className="text-primary">&quot;Respect my authority&quot;</code>).
            NoDNS returns our VPS IP (the keyholder&apos;s page saying{" "}
            <code className="text-primary">&quot;Liar&quot;</code> — published by the Nostr keyholder).
          </p>
          <p className="mb-2 text-sm">
            <strong>Demo domain:</strong>{" "}
            <code className="font-mono text-[0.85rem] text-primary break-all">
              npub10mluej6gljwsjx5v4dnr54n9y0yzf8thwr2l60p3e94q72udh8ksz6uw6q.dns4sats.xyz
            </code>
          </p>
          <div className="my-4 grid grid-cols-1 gap-3 sm:grid-cols-2">
            <div className="rounded-lg border border-red-800/40 bg-red-950/20 p-4">
              <h4 className="mb-2 text-xs font-semibold uppercase tracking-wider text-red-400">
                Standard DNS (8.8.8.8)
              </h4>
              <p className="text-sm text-foreground/60">
                Cloudflare Pages — &ldquo;Respect my authority&rdquo;
              </p>
              <code className="font-mono text-[0.8rem] text-primary">
                188.114.96.3
              </code>
            </div>
            <div className="rounded-lg border border-emerald-800/40 bg-emerald-950/20 p-4">
              <h4 className="mb-2 text-xs font-semibold uppercase tracking-wider text-emerald-400">
                NoDNS (dns.nodns.shop)
              </h4>
              <p className="text-sm text-foreground/60">
                VPS — &ldquo;Liar&rdquo;
              </p>
              <code className="font-mono text-[0.8rem] text-primary">
                46.224.104.12
              </code>
            </div>
          </div>
          <div className="my-4 space-y-3">
            <div className="rounded-lg border border-border bg-background p-4">
              <p className="mb-1 text-xs font-semibold uppercase tracking-wider text-red-400">
                Standard DNS (Cloudflare path)
              </p>
              <pre className="overflow-x-auto font-mono text-[0.85rem] text-primary">
{`dig npub10ml...uw6q.dns4sats.xyz A @8.8.8.8

;; ANSWER SECTION:
npub10ml...dns4sats.xyz. 300 IN A 188.114.96.3`}
              </pre>
            </div>
            <div className="rounded-lg border border-border bg-background p-4">
              <p className="mb-1 text-xs font-semibold uppercase tracking-wider text-emerald-400">
                NoDNS (Knot DNS on VPS)
              </p>
              <pre className="overflow-x-auto font-mono text-[0.85rem] text-primary">
{`dig npub10ml...uw6sats.xyz A @dns.nodns.shop

;; ANSWER SECTION:
npub10ml...dns4sats.xyz. 300 IN A 46.224.104.12`}
              </pre>
            </div>
          </div>
          <p className="text-sm text-foreground/60">
            Cloudflare is the registrar for dns4sats.xyz — their DNS returns their
            own IP. NoDNS resolves from Nostr-published records pointing to the
            VPS. Two completely different realities from one domain name.
          </p>
        </div>

        <div className="mb-5 rounded-xl border border-border bg-card p-6">
          <h3 className="mb-3 text-lg font-semibold">
            Interactive Dual-Resolution Lookup
          </h3>
          <p className="mb-4 text-foreground/60">
            Run the lookups yourself right here. Switch between TXT and A record
            queries to see both demos in action.
          </p>
          <DualLookupDemo />
        </div>
      </div>
    </section>
  );
}
