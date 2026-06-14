export function HowItWorks() {
  const steps = [
    {
      num: 1,
      title: "Generate Keys",
      desc: "Create a Nostr keypair. Your public key (npub) becomes your domain identifier under nodns.shop.",
    },
    {
      num: 2,
      title: "Publish Event",
      desc: "Construct a kind 11111 Nostr event with your DNS records as tags. Sign it with your private key and publish to relays.",
    },
    {
      num: 3,
      title: "Bot Processes",
      desc: "The nodns-bot subscribes to relays, validates your event, checks policy rules, and pushes records to the authoritative DNS server via DDNS.",
    },
    {
      num: 4,
      title: "DNS Resolves",
      desc: "Within seconds, your records are live. Standard DNS queries return your published records. Zero special software needed on the resolver side.",
    },
  ];

  return (
    <section id="how" className="px-6 py-16">
      <div className="mx-auto max-w-[960px]">
        <h2 className="mb-6 text-[1.75rem] font-bold tracking-tight">
          How It Works
        </h2>
        <div className="grid grid-cols-[repeat(auto-fit,minmax(200px,1fr))] gap-4">
          {steps.map((step, i) => (
            <div key={step.num} className="relative">
              {i < steps.length - 1 && (
                <div className="hidden md:block absolute top-12 -right-2 z-10 text-primary/40">
                  <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
                    <path d="M2 8h10m0 0L8 4m4 4L8 12" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"/>
                  </svg>
                </div>
              )}
              <div className="h-full rounded-xl border border-border bg-card p-6 text-center transition-colors hover:border-primary/30">
                <div className="mx-auto mb-3 flex h-10 w-10 items-center justify-center rounded-full bg-primary/10 text-base font-bold text-primary ring-1 ring-primary/20">
                  {step.num}
                </div>
                <h3 className="mb-2 text-base font-semibold">{step.title}</h3>
                <p className="text-sm text-foreground/60 leading-relaxed">{step.desc}</p>
              </div>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}
