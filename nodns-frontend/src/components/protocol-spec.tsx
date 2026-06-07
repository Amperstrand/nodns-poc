export function ProtocolSpec() {
  return (
    <section id="protocol" className="px-6 py-16">
      <div className="mx-auto max-w-[960px]">
        <h2 className="mb-6 text-[1.75rem] font-bold tracking-tight">
          Protocol Specification
        </h2>
        <p className="mb-4 text-[#bbb]">
          Nostr kind 11111 events with structured tags for DNS records,
          delegation, payments, and registrar management.
        </p>

        <h3 className="mb-3 text-lg font-semibold">Event Structure</h3>
        <pre className="mb-4 overflow-x-auto rounded-lg border border-[#222] bg-[#141414] p-4 text-[0.85rem] leading-relaxed">
          <code>{`{
  "kind": 11111,
  "pubkey": "<hex public key>",
  "tags": [
    ["record",    "TYPE", "name", "rdata", "", "", "", "", "", "", "ttl"],
    ["delegation", "DOMAIN", "NPUB", "VALID_FROM", "VALID_UNTIL", "RENEW_BY"],
    ["registrar",  "ZONE", "PUBKEY_HEX"],
    ["cashu",      "TOKEN", "MINT_URL", "AMOUNT"],
    ["zap",        "ZAP_RECEIPT_EVENT_ID", "AMOUNT"]
  ],
  "content": "",
  "created_at": <unix timestamp>
}`}</code>
        </pre>

        <h3 className="mb-3 text-lg font-semibold">Tag Types</h3>
        <div className="mb-4 overflow-x-auto">
          <table className="w-full text-sm">
            <thead>
              <tr>
                <th className="px-3 py-2.5 text-left text-xs font-semibold uppercase tracking-wider text-[#666]">
                  Tag
                </th>
                <th className="px-3 py-2.5 text-left text-xs font-semibold uppercase tracking-wider text-[#666]">
                  Format
                </th>
                <th className="px-3 py-2.5 text-left text-xs font-semibold uppercase tracking-wider text-[#666]">
                  Description
                </th>
              </tr>
            </thead>
            <tbody>
              <tr className="border-b border-[#222]">
                <td className="px-3 py-2.5 font-semibold text-[#e0e0e0] whitespace-nowrap">
                  record
                </td>
                <td className="px-3 py-2.5 font-mono text-xs">
                  [&quot;record&quot;, TYPE, NAME, RDATA, &quot;&quot;, &quot;&quot;,
                  &quot;&quot;, &quot;&quot;, &quot;&quot;, &quot;&quot;, TTL]
                </td>
                <td className="px-3 py-2.5 text-[#bbb]">
                  DNS record entry. 11-element fixed array for forward
                  compatibility.
                </td>
              </tr>
              <tr className="border-b border-[#222]">
                <td className="px-3 py-2.5 font-semibold text-[#e0e0e0] whitespace-nowrap">
                  delegation
                </td>
                <td className="px-3 py-2.5 font-mono text-xs">
                  [&quot;delegation&quot;, DOMAIN, NPUB, VALID_FROM, VALID_UNTIL,
                  RENEW_BY]
                </td>
                <td className="px-3 py-2.5 text-[#bbb]">
                  Grants a pubkey control over a human-readable name within a
                  zone.
                </td>
              </tr>
              <tr className="border-b border-[#222]">
                <td className="px-3 py-2.5 font-semibold text-[#e0e0e0] whitespace-nowrap">
                  registrar
                </td>
                <td className="px-3 py-2.5 font-mono text-xs">
                  [&quot;registrar&quot;, ZONE, PUBKEY_HEX]
                </td>
                <td className="px-3 py-2.5 text-[#bbb]">
                  Identifies the registrar authority for a zone. Only this key
                  can issue delegations.
                </td>
              </tr>
              <tr className="border-b border-[#222]">
                <td className="px-3 py-2.5 font-semibold text-[#e0e0e0] whitespace-nowrap">
                  cashu
                </td>
                <td className="px-3 py-2.5 font-mono text-xs">
                  [&quot;cashu&quot;, TOKEN, MINT_URL, AMOUNT]
                </td>
                <td className="px-3 py-2.5 text-[#bbb]">
                  Anti-spam payment via Cashu ecash tokens (250 sats per new
                  record).
                </td>
              </tr>
              <tr className="border-b border-[#222]">
                <td className="px-3 py-2.5 font-semibold text-[#e0e0e0] whitespace-nowrap">
                  zap
                </td>
                <td className="px-3 py-2.5 font-mono text-xs">
                  [&quot;zap&quot;, ZAP_RECEIPT_EVENT_ID, AMOUNT]
                </td>
                <td className="px-3 py-2.5 text-[#bbb]">
                  Payment proof via NIP-57 zap receipts.
                </td>
              </tr>
            </tbody>
          </table>
        </div>

        <h3 className="mb-3 text-lg font-semibold">
          Record Tag Fields (11-element)
        </h3>
        <div className="mb-4 overflow-x-auto">
          <table className="w-full text-sm">
            <thead>
              <tr>
                <th className="px-3 py-2.5 text-left text-xs font-semibold uppercase tracking-wider text-[#666]">
                  Index
                </th>
                <th className="px-3 py-2.5 text-left text-xs font-semibold uppercase tracking-wider text-[#666]">
                  Field
                </th>
                <th className="px-3 py-2.5 text-left text-xs font-semibold uppercase tracking-wider text-[#666]">
                  Description
                </th>
              </tr>
            </thead>
            <tbody>
              {[
                ["0", "record", 'Tag identifier (literal "record")'],
                ["1", "type", "DNS record type (A, AAAA, CNAME, TXT, MX)"],
                [
                  "2",
                  "name",
                  'Subdomain name ("@" for root, or e.g. "www")',
                ],
                [
                  "3",
                  "rdata",
                  "Record data (IP address, hostname, text content)",
                ],
                ["4-9", "unused", "Empty strings (legacy padding)"],
                ["10", "ttl", "TTL in seconds (as string)"],
              ].map(([idx, field, desc]) => (
                <tr key={idx} className="border-b border-[#222]">
                  <td className="px-3 py-2.5 font-semibold text-[#e0e0e0]">
                    {idx}
                  </td>
                  <td className="px-3 py-2.5 font-semibold text-[#e0e0e0]">
                    {field}
                  </td>
                  <td className="px-3 py-2.5 text-[#bbb]">{desc}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>

        <h3 className="mb-3 text-lg font-semibold">FQDN Construction</h3>
        <pre className="mb-4 overflow-x-auto rounded-lg border border-[#222] bg-[#141414] p-4 text-[0.85rem] leading-relaxed">
          <code>{`{name}.{npub}.{zone}

Examples (full npub truncated for readability):
  name="@"    → npub1ykal2...pa3dl.nodns.shop
  name="www"  → www.npub1ykal2...pa3dl.nodns.shop
  name="blog" → blog.npub1ykal2...pa3dl.nodns.shop

Delegated names:
  Delegation assigns: alice.nodns.shop → npub1abc...xyz
  User publishes:     name="alice" → alice.nodns.shop`}</code>
        </pre>

        <h3 className="mb-3 text-lg font-semibold">
          Delegation &amp; Custom Names
        </h3>
        <p className="mb-4 text-[#bbb]">
          A zone registrar (identified by a <code className="font-mono text-[#ff6b35]">registrar</code> tag)
          can delegate a human-readable name to a specific Nostr pubkey. For
          example, the <code className="font-mono text-[#ff6b35]">nodns.shop</code> zone registrar can
          assign <code className="font-mono text-[#ff6b35]">alice.nodns.shop</code> to{" "}
          <code className="font-mono text-[#ff6b35]">npub1abc...xyz</code> for a fixed time period.
          The delegation is signed by the registrar&apos;s private key and published as
          a Nostr event.
        </p>
        <p className="mb-6 text-[#bbb]">
          Once delegated, the user publishes standard <code className="font-mono text-[#ff6b35]">record</code>{" "}
          tags with <code className="font-mono text-[#ff6b35]">name</code> set to their delegated name.
          The bot verifies the delegation exists and is valid before pushing DNS
          records. Delegations are <strong>irrevocable</strong> within their
          validity period &mdash; the Nostr event is the authoritative proof.
        </p>

        <h3 className="mb-3 text-lg font-semibold">Allowed Record Types</h3>
        <div className="grid grid-cols-[repeat(auto-fit,minmax(280px,1fr))] gap-4">
          {[
            {
              type: "A",
              desc: "IPv4 address mapping. Points a domain to an IPv4 address.",
            },
            {
              type: "AAAA",
              desc: "IPv6 address mapping. Points a domain to an IPv6 address.",
            },
            {
              type: "CNAME",
              desc: "Canonical name. Aliases one domain to another.",
            },
            {
              type: "TXT",
              desc: "Text records. Used for verification, SPF, DKIM, etc.",
            },
            {
              type: "MX",
              desc: "Mail exchange. Specifies mail server for the domain.",
            },
          ].map((rec) => (
            <div
              key={rec.type}
              className="rounded-[10px] border border-[#222] bg-[#141414] p-6"
            >
              <h3 className="mb-3 text-lg font-semibold">{rec.type}</h3>
              <p className="text-[#bbb]">{rec.desc}</p>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}
