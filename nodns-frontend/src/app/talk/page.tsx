import Link from "next/link";
import { ErrorBoundary } from "@/components/error-boundary";
import { SiteHeader } from "@/components/site-header";
import { SiteFooter } from "@/components/site-footer";

type Bullet = string;

interface SectionCard {
  title: string;
  recommendation: string;
  for: Bullet[];
  against: Bullet[];
  note?: string;
}

const NAV = [
  { id: "thesis", label: "Thesis" },
  { id: "protocol", label: "Protocol" },
  { id: "names", label: "Name model" },
  { id: "decisions", label: "Decisions" },
  { id: "cashu", label: "Cashu" },
  { id: "pricing", label: "Pricing" },
  { id: "sniping", label: "Sniping vs auctions" },
  { id: "future", label: "Future" },
  { id: "ask", label: "Ask" },
];

const TALK_RECOMMENDATION = {
  title: "Minimal v1 recommendation",
  body: "Use 11111 as the special NoDNS kind, keep current-state logic in the bot and viewer, and make custom names a public locked bid with a time-bounded refund path; leave 31111 as a future migration only if relay deduplication becomes worth the churn.",
};

const protocolSteps = [
  {
    title: "1. Identity",
    body: "A Nostr keypair is the source of authority. The npub is the public identity; the nsec is the signing power.",
    detail: "For npub-derived names, ownership is cryptographic, not administrative.",
  },
  {
    title: "2. Choose name class",
    body: "There are two trust models: npub names and string names. They should not be forced into the same rule set.",
    detail: "This is where we decide whether the system is a mirror of identity or a lease market.",
  },
  {
    title: "3. Publish event",
    body: "The user publishes a NoDNS event with record tags, plus payment/claim/delegation data when needed.",
    detail: "Kind 11111 is the special NoDNS event kind for the current protocol.",
  },
  {
    title: "4. Validate",
    body: "The bot checks signature, authority, record shape, policy, and payment before it touches DNS.",
    detail: "If anything fails, the event should be visible and explainable, not silently dropped.",
  },
  {
    title: "5. Apply DNS",
    body: "Valid events become DDNS updates to Knot DNS, which then DNSSEC-signs the result.",
    detail: "This is the convenience layer; the event log is the source of truth.",
  },
  {
    title: "6. Resolve",
    body: "Resolvers can consume the mirrored DNS view or the event log, depending on their trust model.",
    detail: "The same protocol can support standard DNS users and NoDNS-aware users.",
  },
  {
    title: "7. Renew",
    body: "Renewal should be deterministic and verifiable, with the event log acting as the proof.",
    detail: "Lease logic matters only for string names; npub names remain cryptographic.",
  },
];

const decisions: SectionCard[] = [
  {
    title: "$npub.tld only, or also $npub.subdomain.tld?",
    recommendation: "Support both, but treat $npub.tld as the fundamental model and subdomain mirroring as an optional convenience layer.",
    for: [
      "Keeps cryptographic ownership clean and simple.",
      "Lets operators charge only for mirroring, not for ownership.",
      "Makes it easy to explain: npub is the deed; DNS is the mirror.",
    ],
    against: [
      "A subdomain like nodns.shop introduces an operator boundary and extra policy.",
      "If we blur the two, people may think the lease model is the same as cryptographic ownership.",
      "A single namespace story is easier to market than two overlapping ones.",
    ],
    note: "Suggested framing: npub is the protocol; nodns.shop is the first deployment.",
  },
  {
    title: "Should Cashu be locked?",
    recommendation: "Yes — for the first milestone, use locked Cashu with a refund path so bids are public and bidders know the owner must either honor or time out.",
    for: [
      "Makes the bid public and clearly tied to the name.",
      "Creates a credible accept-or-refund path for bidders.",
      "Turns the name claim into an explicit owner honor/timeout flow.",
    ],
    against: [
      "Adds some wallet and recovery complexity.",
      "Needs a clear timeout so bids don’t become hostage payments.",
      "Can feel like auction machinery if we overcomplicate the flow.",
    ],
    note: "Suggested framing: public bid + owner accept + refund if unclaimed; no clawback, but also no permanent lock without a deadline.",
  },
  {
    title: "Should the namespace owner publish pricing or should users bid?",
    recommendation: "Use a public bid model with a published floor/minimum and owner acceptance; let the market discover value, but keep the rules simple.",
    for: [
      "Bids are visible and easy to discuss publicly.",
      "The owner can still set a minimum / reserve price.",
      "The accept-or-refund flow keeps the model understandable.",
    ],
    against: [
      "Bids can create race dynamics if the domain is highly desirable.",
      "Popular names may need blind bidding later to avoid sniping.",
      "Pricing becomes less deterministic than a posted fee schedule.",
    ],
    note: "Minimal standard: public bid with minimum/accept rules now; consider blind/private bid later if sniping becomes a real issue.",
  },
  {
    title: "Name sniping / blind bids vs public auctions",
    recommendation: "Keep public bids for v1, but treat blind/private bids as the future answer when we want to hide demand and reduce racing.",
    for: [
      "Blind bids reduce sniping and front-running.",
      "A sequenced/private path can make first-come-first-served practical without revealing the target too early.",
      "Public bids are easy to teach and are enough for the first milestone.",
    ],
    against: [
      "Public bids make racing visible, so popular names may still need a later upgrade.",
      "Blind bids add reveal/verification surface area.",
      "Relay sequencing or registrar-run relays add operational complexity.",
    ],
    note: "Future options: blind bid, registrar-sequenced relay, or a first-come-first-served policy for low-stakes names.",
  },
];

const futureTopics = [
  "31111 migration as the 'we are live' moment",
  "Blind/private bids to hide demand and reduce sniping",
  "Relay sequencing or registrar-run relay for FCFS fairness",
  "P2PK locking and stronger payment acceptance semantics",
  "Takeover / renewal / namespace expiry mechanics",
  "Multi-operator namespaces and federated trust",
  "Anti-spam alternatives (PoW, burn, escrow variants)",
];

function SlideTitle({ eyebrow, title, body }: { eyebrow: string; title: string; body: string }) {
  return (
    <div className="max-w-3xl">
      <p className="text-xs font-semibold uppercase tracking-[0.3em] text-primary/80">{eyebrow}</p>
      <h2 className="mt-3 text-3xl font-bold tracking-tight sm:text-4xl">{title}</h2>
      <p className="mt-4 text-base leading-7 text-foreground/70 sm:text-lg">{body}</p>
    </div>
  );
}

function DebateCard({ item }: { item: SectionCard }) {
  return (
    <article className="rounded-2xl border border-border bg-card p-6 shadow-[0_0_0_1px_rgba(255,255,255,0.02),0_20px_60px_rgba(0,0,0,0.35)]">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div className="max-w-2xl">
          <h3 className="text-xl font-semibold tracking-tight">{item.title}</h3>
          <p className="mt-3 text-sm leading-6 text-foreground/70">{item.recommendation}</p>
          {item.note && <p className="mt-3 text-sm text-primary/90">{item.note}</p>}
        </div>
        <div className="rounded-full border border-primary/20 bg-primary/10 px-3 py-1 text-xs font-semibold uppercase tracking-[0.24em] text-primary">
          Decision
        </div>
      </div>

      <div className="mt-6 grid gap-4 md:grid-cols-2">
        <div className="rounded-xl border border-emerald-900/40 bg-emerald-950/20 p-4">
          <p className="text-xs font-semibold uppercase tracking-[0.24em] text-emerald-400">Arguments for</p>
          <ul className="mt-3 space-y-2 text-sm leading-6 text-foreground/75">
            {item.for.map((line) => (
              <li key={line} className="flex gap-2">
                <span className="text-emerald-400">+</span>
                <span>{line}</span>
              </li>
            ))}
          </ul>
        </div>
        <div className="rounded-xl border border-amber-900/40 bg-amber-950/20 p-4">
          <p className="text-xs font-semibold uppercase tracking-[0.24em] text-amber-400">Arguments against</p>
          <ul className="mt-3 space-y-2 text-sm leading-6 text-foreground/75">
            {item.against.map((line) => (
              <li key={line} className="flex gap-2">
                <span className="text-amber-400">−</span>
                <span>{line}</span>
              </li>
            ))}
          </ul>
        </div>
      </div>
    </article>
  );
}

export default function TalkPage() {
  return (
    <ErrorBoundary>
      <SiteHeader />
      <main id="main-content" className="overflow-x-hidden px-4 pb-20 sm:px-6 lg:px-8">
        <div className="mx-auto max-w-[1440px] py-6 md:py-10">
          <div className="grid gap-8 xl:grid-cols-[300px_minmax(0,1fr)]">
            <aside className="xl:sticky xl:top-24 xl:h-[calc(100vh-7rem)]">
              <div className="rounded-2xl border border-border bg-card/80 p-5 backdrop-blur">
                <p className="text-xs font-semibold uppercase tracking-[0.3em] text-primary/80">NoDNS talk</p>
                <h1 className="mt-3 text-2xl font-bold tracking-tight">Minimal consensus, explained</h1>
                <p className="mt-3 text-sm leading-6 text-foreground/70">
                  A browser-openable talk deck for walking through the protocol and the decisions we still need to make.
                </p>

                <nav className="mt-6 space-y-1.5">
                  {NAV.map((item) => (
                    <a
                      key={item.id}
                      href={`#${item.id}`}
                      className="block rounded-lg px-3 py-2 text-sm text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
                    >
                      {item.label}
                    </a>
                  ))}
                </nav>

                <div className="mt-6 rounded-xl border border-border bg-background p-4">
                  <p className="text-xs font-semibold uppercase tracking-[0.24em] text-muted-foreground">Open questions</p>
                  <p className="mt-2 text-sm leading-6 text-foreground/70">
                    The point of the talk is not to lock everything down. It is to make the decisions visible, argue both sides, and leave a crisp minimal path forward.
                  </p>
                </div>
              </div>
            </aside>

            <div className="min-w-0 space-y-8 md:space-y-10">
              <section id="thesis" className="scroll-mt-24 rounded-3xl border border-border bg-[radial-gradient(circle_at_top_left,rgba(255,107,53,0.12),transparent_35%),linear-gradient(180deg,rgba(20,20,20,0.98),rgba(10,10,10,0.98))] p-6 md:p-8 xl:p-10">
                <div className="max-w-4xl">
                  <p className="text-xs font-semibold uppercase tracking-[0.3em] text-primary/80">Thesis</p>
                  <h2 className="mt-4 text-4xl font-bold tracking-tight sm:text-5xl">
                    A minimal standard for NoDNS should be easy to understand, easy to argue about, and easy to implement.
                  </h2>
                  <p className="mt-5 max-w-3xl text-base leading-7 text-foreground/70 sm:text-lg">
                    The goal is a shared baseline for npub names, string names, and record publishing — not a final answer to every future market or governance problem.
                  </p>
                </div>
                <div className="mt-8 grid gap-4 lg:grid-cols-3">
                  {[
                    ["Truth", "Nostr events are the source of truth."],
                    ["Consensus", "Resolvers and operators agree on the same log."],
                    ["Convenience", "Traditional DNS mirrors the log for normal users."],
                  ].map(([k, v]) => (
                    <div key={k} className="rounded-2xl border border-border bg-background/80 p-5">
                      <p className="text-xs font-semibold uppercase tracking-[0.24em] text-primary/80">{k}</p>
                      <p className="mt-3 text-sm leading-6 text-foreground/75">{v}</p>
                    </div>
                  ))}
                </div>
              </section>

              <section id="protocol" className="scroll-mt-24 space-y-4 rounded-3xl border border-border bg-card p-6 md:p-8 xl:p-10">
                <SlideTitle
                  eyebrow="Protocol walkthrough"
                  title="How a record becomes DNS"
                  body="This is the end-to-end flow we should be able to explain without hand-waving: identity, event, validation, payment, DNS, resolution, and renewal."
                />
                <div className="mt-8 grid gap-4 xl:grid-cols-2 2xl:grid-cols-3">
                  {protocolSteps.map((step, index) => (
                    <div key={step.title} className="rounded-2xl border border-border bg-background p-5">
                      <div className="flex items-start justify-between gap-4">
                        <h3 className="text-lg font-semibold">{step.title}</h3>
                        <span className="rounded-full border border-primary/20 bg-primary/10 px-2.5 py-1 text-xs font-semibold text-primary">
                          {String(index + 1).padStart(2, "0")}
                        </span>
                      </div>
                      <p className="mt-3 text-sm leading-6 text-foreground/75">{step.body}</p>
                      <p className="mt-3 text-sm leading-6 text-muted-foreground">{step.detail}</p>
                    </div>
                  ))}
                </div>
              </section>

              <section id="names" className="scroll-mt-24 space-y-4 rounded-3xl border border-border bg-card p-6 md:p-8 xl:p-10">
                <SlideTitle
                  eyebrow="Name model"
                  title="Should we focus on $npub.tld, or also $npub.subdomain.tld?"
                  body="This is the biggest conceptual fork. One path makes identity the protocol. The other path makes the operator’s zone part of the product."
                />
                <div className="grid gap-4 xl:grid-cols-2">
                  <div className="rounded-2xl border border-emerald-900/40 bg-emerald-950/15 p-6">
                    <p className="text-xs font-semibold uppercase tracking-[0.24em] text-emerald-400">For focusing on $npub.tld</p>
                    <ul className="mt-4 space-y-2 text-sm leading-6 text-foreground/75">
                      <li>• Clean cryptographic ownership story.</li>
                      <li>• Simple to explain: the key is the name.</li>
                      <li>• No operator trust needed for the core model.</li>
                    </ul>
                  </div>
                  <div className="rounded-2xl border border-amber-900/40 bg-amber-950/15 p-6">
                    <p className="text-xs font-semibold uppercase tracking-[0.24em] text-amber-400">For also supporting $npub.subdomain.tld</p>
                    <ul className="mt-4 space-y-2 text-sm leading-6 text-foreground/75">
                      <li>• Gives operators a visible deployment zone.</li>
                      <li>• Supports mirror fees and convenience pricing.</li>
                      <li>• Makes nodns.shop a concrete, browser-friendly example.</li>
                    </ul>
                  </div>
                </div>
                <div className="rounded-2xl border border-primary/20 bg-primary/10 p-5 text-sm leading-6 text-foreground/80">
                  <strong className="text-foreground">Suggested position:</strong> keep the protocol centered on $npub.tld, but allow subdomain namespaces as opt-in operator deployments.
                </div>
              </section>

              <section id="decisions" className="scroll-mt-24 space-y-4 rounded-3xl border border-border bg-card p-6 md:p-8 xl:p-10">
                <SlideTitle
                  eyebrow="Decision table"
                  title="The choices we need to make explicit"
                  body="This is the room discussion section: state the option, show both sides, then mark the provisional recommendation."
                />
                <div className="space-y-4">
                  {decisions.map((item) => (
                    <DebateCard key={item.title} item={item} />
                  ))}
                </div>
                <div className="rounded-2xl border border-primary/20 bg-primary/10 p-5 text-sm leading-6 text-foreground/80">
                  <strong className="text-foreground">{TALK_RECOMMENDATION.title}:</strong> {TALK_RECOMMENDATION.body}
                </div>
              </section>

              <section id="cashu" className="scroll-mt-24 space-y-4 rounded-3xl border border-border bg-card p-6 md:p-8 xl:p-10">
                <SlideTitle
                  eyebrow="Payment semantics"
                  title="Should Cashu be locked?"
                  body="For the first milestone, the answer can be yes: a public locked bid with a refund path makes the owner’s choice visible and the bidder’s risk bounded."
                />
                <DebateCard
                  item={{
                    title: "Locking Cashu tokens",
                    recommendation: "Use locked Cashu with a refund path in v1 so the bid is public, time-bounded, and clearly honor-or-reclaim.",
                    for: [
                      "Makes the bid public and bound to the name.",
                      "Gives bidders a clear refund deadline if the owner does not accept.",
                      "Feels like a real market signal without needing a full auction system.",
                    ],
                    against: [
                      "Needs a timeout and reclaim path to avoid hostage payments.",
                      "Adds wallet recovery and claim-state complexity.",
                      "Can drift into auction machinery if we keep adding rules.",
                    ],
                    note: "Recommendation: public locked bid + refund deadline now; later, consider blind bids if public bidding attracts too much sniping.",
                  }}
                />
              </section>

              <section id="pricing" className="scroll-mt-24 space-y-4 rounded-3xl border border-border bg-card p-6 md:p-8 xl:p-10">
                <SlideTitle
                  eyebrow="Economics"
                  title="Should the owner publish pricing, or should people bid?"
                  body="The first milestone can be a public bid market with a published floor price and owner acceptance, then evolve toward blind bids later if necessary."
                />
                <div className="grid gap-4">
                  <DebateCard
                    item={{
                      title: "Published floor vs public bids",
                      recommendation: "Use a public bid model with a floor/reserve price and owner acceptance for v1.",
                      for: [
                        "Lets the market discover value while still keeping a floor.",
                        "The owner can say yes or no without changing the rules.",
                        "Public bids are easy to explain in the talk and easy to inspect later.",
                      ],
                      against: [
                        "Public bids can still be raced or sniped for popular names.",
                        "A floor price does not fully solve sequencing fairness.",
                        "Highly desirable names may still need blind bids later.",
                      ],
                    }}
                  />
                </div>
              </section>

              <section id="sniping" className="scroll-mt-24 space-y-4 rounded-3xl border border-border bg-card p-6 md:p-8 xl:p-10">
                <SlideTitle
                  eyebrow="Conflict resolution"
                  title="Public bids now, blind/private bids later?"
                  body="Popular names are where the system stops being purely technical and starts becoming governance. Public bids are fine for v1; hiding the target is the future tool for sniping and sequencing."
                />
                <div className="grid gap-4 lg:grid-cols-2">
                  <div className="rounded-2xl border border-sky-900/40 bg-sky-950/15 p-6">
                    <p className="text-xs font-semibold uppercase tracking-[0.24em] text-sky-400">Public bids (v1)</p>
                    <ul className="mt-4 space-y-2 text-sm leading-6 text-foreground/75">
                      <li>• Easy to understand and demo.</li>
                      <li>• Works with a clear refund deadline.</li>
                      <li>• Good enough for the first milestone.</li>
                    </ul>
                  </div>
                  <div className="rounded-2xl border border-violet-900/40 bg-violet-950/15 p-6">
                    <p className="text-xs font-semibold uppercase tracking-[0.24em] text-violet-400">Blind / private bids (future)</p>
                    <ul className="mt-4 space-y-2 text-sm leading-6 text-foreground/75">
                      <li>• Hides demand from squatters and snipers.</li>
                      <li>• Can support sequencing / FCFS without exposing the target.
                      </li>
                      <li>• Could be run through a registrar relay or private sequencing path.</li>
                    </ul>
                  </div>
                </div>
                <div className="rounded-2xl border border-primary/20 bg-primary/10 p-5 text-sm leading-6 text-foreground/80">
                  <strong className="text-foreground">Suggested position:</strong> keep public locked bids for v1, then decide later whether blind/private bids or sequenced first-come-first-serve deserve a protocol path.
                </div>
              </section>

              <section id="future" className="scroll-mt-24 space-y-4 rounded-3xl border border-border bg-card p-6 md:p-8 xl:p-10">
                <SlideTitle
                  eyebrow="Future ideas"
                  title="What should stay out of the first standard?"
                  body="These are good ideas, but they are not part of the minimal POC. Put them on the board, not in the baseline."
                />
                <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
                  {futureTopics.map((topic) => (
                    <div key={topic} className="rounded-2xl border border-border bg-background p-5 text-sm leading-6 text-foreground/75">
                      {topic}
                    </div>
                  ))}
                </div>
              </section>

              <section id="ask" className="scroll-mt-24 rounded-3xl border border-primary/20 bg-[linear-gradient(180deg,rgba(255,107,53,0.12),rgba(20,20,20,0.96))] p-6 md:p-8 xl:p-10">
                <p className="text-xs font-semibold uppercase tracking-[0.3em] text-primary/80">Discussion prompt</p>
                <h2 className="mt-4 text-3xl font-bold tracking-tight sm:text-4xl">What do we standardize now?</h2>
                <div className="mt-6 grid gap-4 md:grid-cols-2">
                  {[
                    "Which name classes are mandatory in v1?",
                    "Do we need pricing or just acceptance?",
                    "Should browser and CLI tools share the same record model?",
                    "Which future ideas deserve their own issue instead of landing in the spec?",
                  ].map((q) => (
                    <div key={q} className="rounded-2xl border border-border bg-background/80 p-5 text-sm leading-6 text-foreground/80">
                      {q}
                    </div>
                  ))}
                </div>
                <div className="mt-8 flex flex-wrap gap-3">
                  <Link href="/learn" className="rounded-full border border-border bg-card px-4 py-2 text-sm text-foreground hover:border-primary/30 hover:text-primary transition-colors">
                    Open Learn
                  </Link>
                  <Link href="/records" className="rounded-full border border-border bg-card px-4 py-2 text-sm text-foreground hover:border-primary/30 hover:text-primary transition-colors">
                    Open Records
                  </Link>
                  <Link href="/register" className="rounded-full bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/80 transition-colors">
                    Open Register
                  </Link>
                </div>
                <div className="mt-6 rounded-2xl border border-border bg-background/80 p-5 text-sm leading-6 text-foreground/80">
                  <strong className="text-foreground">Bottom line:</strong> 11111 is the special NoDNS kind for the current protocol. 31111 is only a future migration if we decide relay dedupe is worth the change.
                </div>
              </section>
            </div>
          </div>
        </div>
      </main>
      <SiteFooter />
    </ErrorBoundary>
  );
}
