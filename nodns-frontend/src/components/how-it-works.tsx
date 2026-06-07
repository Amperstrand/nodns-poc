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
        <div className="grid grid-cols-[repeat(auto-fit,minmax(200px,1fr))] gap-6">
          {steps.map((step) => (
            <div
              key={step.num}
              className="rounded-[10px] border border-[#222] bg-[#141414] p-6 text-center"
            >
              <div className="mx-auto mb-2 flex h-8 w-8 items-center justify-center rounded-full bg-[#ff6b35] text-sm font-bold text-white">
                {step.num}
              </div>
              <h3 className="mb-3 text-lg font-semibold">{step.title}</h3>
              <p className="text-[#bbb]">{step.desc}</p>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}
