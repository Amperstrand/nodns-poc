export function FAQ() {
  const items = [
    {
      q: "How long until my records are live?",
      a: "Typically 3–5 seconds from publishing your Nostr event. The bot subscribes to relays in real-time, validates the event, and pushes changes via DDNS immediately.",
    },
    {
      q: "What should I put in the name field?",
      a: "The name field is the subdomain label only. Use @ for your root domain (npub1abc...xyz.nodns.shop), or a single label like www or blog for subdomains. Never put a full domain path — the bot constructs the FQDN automatically.",
    },
    {
      q: "How do I get a human-readable domain like alice.nodns.shop?",
      a: "Human-readable names require cryptographic delegation from a zone registrar. A registrar publishes a delegation tag assigning a name to your npub for a fixed period. Watch the roadmap for public availability.",
    },
    {
      q: "Can I publish records for a different zone?",
      a: "No. Each zone has its own nodns-bot instance. Publishing to the nodns.shop bot only creates records under *.nodns.shop. Other zones need their own bot infrastructure.",
    },
    {
      q: "Why is my record showing up as a long nested domain?",
      a: "You likely put a full domain path (like blog.alice.nodns.shop) in the name field instead of a simple label (blog). The bot appends .{your_npub}.{zone} automatically. Use just the subdomain part.",
    },
  ];

  return (
    <section className="border-t border-border/40 px-6 py-16">
      <div className="mx-auto max-w-[960px]">
        <h2 className="mb-6 text-[1.75rem] font-bold tracking-tight">
          FAQ
        </h2>
        <div className="space-y-3">
          {items.map((item) => (
            <details
              key={item.q}
              className="group rounded-xl bg-card ring-1 ring-foreground/10"
            >
              <summary className="flex cursor-pointer items-center justify-between px-5 py-4 text-sm font-medium list-none hover:bg-muted/30 transition-colors [&::-webkit-details-marker]:hidden">
                {item.q}
                <span className="ml-2 flex size-6 shrink-0 items-center justify-center rounded-full bg-muted/40 text-foreground transition-transform duration-200 group-open:rotate-45">
                  +
                </span>
              </summary>
              <div className="px-5 pb-4 text-base text-foreground/70">
                {item.a}
              </div>
            </details>
          ))}
        </div>
        <p className="mt-4 text-center text-xs text-foreground/50">
          More questions answered in{" "}
          <a href="/learn" className="text-primary hover:underline">
            Learn →
          </a>
        </p>
      </div>
    </section>
  );
}
