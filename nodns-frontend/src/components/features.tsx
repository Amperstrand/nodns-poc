export function Features() {
  return (
    <section id="what" className="px-6 py-16">
      <div className="mx-auto max-w-[960px]">
        <h2 className="mb-6 text-[1.75rem] font-bold tracking-tight">
          What is NoDNS?
        </h2>
        <p className="mb-4 text-[#bbb]">
          NoDNS is a protocol that resolves DNS records from Nostr events.
          Instead of registering domains through a traditional registrar and
          configuring DNS through a control panel, users publish
          cryptographically-signed events to Nostr relays. A NoDNS-compatible
          nameserver reads these events and serves them as standard DNS
          responses.
        </p>
        <p className="mb-4 text-[#bbb]">
          No accounts. No passwords. No billing. Your Nostr keypair IS your
          domain credential. Anyone with a Nostr key can claim a subdomain under
          nodns.shop and point it anywhere they want.
        </p>
        <p className="mb-6 text-[#bbb]">
          Custom names like <code className="font-mono text-[#ff6b35]">alice.nodns.shop</code> are
          possible through cryptographic delegation. A zone registrar signs a
          Nostr event granting a pubkey exclusive control over a human-readable
          name for a time period. This delegation is{" "}
          <strong>irrevocable</strong> &mdash; even if the registrar removes the
          DNS records, the Nostr event remains the authority.
        </p>
        <div className="mt-6 grid grid-cols-[repeat(auto-fit,minmax(280px,1fr))] gap-4">
          <div className="rounded-[10px] border border-[#222] bg-[#141414] p-6">
            <h3 className="mb-3 text-lg font-semibold">Decentralized</h3>
            <p className="text-[#bbb]">
              No central authority controls your records. Your private key is
              your proof of ownership. No one can take your domain or modify
              your records without it.
            </p>
          </div>
          <div className="rounded-[10px] border border-[#222] bg-[#141414] p-6">
            <h3 className="mb-3 text-lg font-semibold">Instant</h3>
            <p className="text-[#bbb]">
              Publish an event and your DNS records propagate globally in 3-5
              seconds. The bot subscribes to relays in real-time and pushes
              changes via DDNS immediately.
            </p>
          </div>
          <div className="rounded-[10px] border border-[#222] bg-[#141414] p-6">
            <h3 className="mb-3 text-lg font-semibold">Standard DNS</h3>
            <p className="text-[#bbb]">
              Queries resolve via normal DNS protocol. Any resolver, any
              operating system, any device. No special software needed to look
              up records.
            </p>
          </div>
        </div>
      </div>
    </section>
  );
}
