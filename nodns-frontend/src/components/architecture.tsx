export function Architecture() {
  return (
    <section id="architecture" className="px-6 py-16">
      <div className="mx-auto max-w-[960px]">
        <h2 className="mb-6 text-[1.75rem] font-bold tracking-tight">
          Architecture
        </h2>
        <div className="mt-4">
          <div className="flex gap-0 max-[700px]:flex-col max-[700px]:gap-2">
            {/* Nostr Relays */}
            <div className="min-w-[180px] flex-1 rounded-[10px] border border-[#222] bg-[#141414] p-4">
              <div className="mb-2.5 border-b border-[#222] pb-2 text-sm font-bold text-[#e0e0e0]">
                Nostr Relays
              </div>
              <div className="text-xs text-[#666] leading-relaxed">
                wss://relay.damus.io
              </div>
              <div className="text-xs text-[#666] leading-relaxed">
                wss://nos.lol
              </div>
              <div className="text-xs text-[#666] leading-relaxed">
                wss://relay.nostr.band
              </div>
              <div className="text-xs text-[#666] leading-relaxed">
                wss://nostr.wine
              </div>
              <div className="text-xs text-[#666] leading-relaxed">
                wss://relay.tollgate.me
              </div>
            </div>

            {/* Arrow */}
            <div className="flex items-center px-2 text-xl text-[#666] max-[700px]:rotate-90 max-[700px]:justify-center max-[700px]:py-1">
              →
            </div>

            {/* nodns-bot */}
            <div className="min-w-[180px] flex-1 rounded-[10px] border border-[#222] bg-[#141414] p-4">
              <div className="mb-2.5 border-b border-[#222] pb-2 text-sm font-bold text-[#ff6b35]">
                nodns-bot{" "}
                <span className="ml-1.5 rounded bg-[rgba(255,107,53,0.15)] px-1.5 py-0.5 text-[0.65rem] font-semibold text-[#ff6b35]">
                  Rust
                </span>
              </div>
              <div className="text-xs text-[#666] leading-relaxed">
                Subscribe to kind 11111
              </div>
              <div className="text-xs text-[#666] leading-relaxed">
                Validate signatures
              </div>
              <div className="text-xs text-[#666] leading-relaxed">
                Check authority &amp; delegation
              </div>
              <div className="text-xs text-[#666] leading-relaxed">
                Verify payments (Cashu)
              </div>
              <div className="text-xs text-[#666] leading-relaxed">
                Push via DDNS (RFC 2136)
              </div>
            </div>

            {/* Arrow */}
            <div className="flex items-center px-2 text-xl text-[#666] max-[700px]:rotate-90 max-[700px]:justify-center max-[700px]:py-1">
              →
            </div>

            {/* Knot DNS */}
            <div className="min-w-[180px] flex-1 rounded-[10px] border border-[#222] bg-[#141414] p-4">
              <div className="mb-2.5 border-b border-[#222] pb-2 text-sm font-bold text-[#2ecc71]">
                Knot DNS
              </div>
              <div className="text-xs text-[#666] leading-relaxed">
                Authoritative nameserver
              </div>
              <div className="text-xs text-[#666] leading-relaxed">
                Zone: nodns.shop
              </div>
              <div className="text-xs text-[#666] leading-relaxed">
                Primary: ns1.nodns.shop
              </div>
              <div className="text-xs text-[#666] leading-relaxed">
                Secondary: puck.nether.net
              </div>
              <div className="text-xs text-[#666] leading-relaxed">
                TSIG-signed DDNS updates
              </div>
            </div>

            {/* Arrow */}
            <div className="flex items-center px-2 text-xl text-[#666] max-[700px]:rotate-90 max-[700px]:justify-center max-[700px]:py-1">
              →
            </div>

            {/* Internet */}
            <div className="min-w-[180px] flex-1 rounded-[10px] border border-[#222] bg-[#141414] p-4">
              <div className="mb-2.5 border-b border-[#222] pb-2 text-sm font-bold text-[#5dade2]">
                Internet
              </div>
              <div className="text-xs text-[#666] leading-relaxed">
                Standard DNS queries
              </div>
              <div className="text-xs text-[#666] leading-relaxed">
                Any resolver, any device
              </div>
              <div className="text-xs text-[#666] leading-relaxed">
                Records live in seconds
              </div>
            </div>
          </div>
          <p className="mt-4 text-sm text-[#666]">
            In the future, a ccTLD operator could run their own nodns-bot to
            enable Nostr-native DNS for an entire country-code TLD.
          </p>
        </div>
      </div>
    </section>
  );
}
