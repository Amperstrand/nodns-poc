export function FAQ() {
  return (
    <section id="faq" className="px-6 py-16">
      <div className="mx-auto max-w-[960px]">
        <h2 className="mb-6 text-[1.75rem] font-bold tracking-tight">
          Common Mistakes &amp; FAQ
        </h2>

        {/* Critical: nested domain warning */}
        <div className="mb-5 rounded-[10px] border border-[#222] bg-[#141414] p-6">
          <h3 className="mb-3 text-lg font-semibold text-[#e74c3c]">
            ⚠ Why is my record showing up as a long nested domain?
          </h3>
          <p className="mb-3 text-[#bbb]">
            If your record looks like this in the browser:
          </p>
          <pre className="mb-3 overflow-x-auto rounded-lg border border-[#222] bg-[#0a0a0a] p-4 text-[0.85rem] leading-relaxed">
              <code>
                blog.alice.nodns.shop.npub13udukn...pgx.nodns.shop
              </code>
          </pre>
          <p className="mb-3 text-[#bbb]">
            <strong>
              You put a full domain path in the <code className="font-mono text-[#ff6b35]">name</code>{" "}
              field instead of a simple subdomain label.
            </strong>
          </p>
          <p className="mb-3 text-[#bbb]">
            The <code className="font-mono text-[#ff6b35]">name</code> field in a record tag should
            only be the subdomain part. The bot automatically appends{" "}
            <code className="font-mono text-[#ff6b35]">.&#123;your_npub&#125;.&#123;zone&#125;</code> to
            construct the full domain. Think of it like this:
          </p>
          <div className="mb-4 overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr>
                  <th className="px-3 py-2.5 text-left text-xs font-semibold uppercase tracking-wider text-[#666]">
                    You type in <code>name</code>
                  </th>
                  <th className="px-3 py-2.5 text-left text-xs font-semibold uppercase tracking-wider text-[#666]">
                    Bot builds
                  </th>
                  <th className="px-3 py-2.5 text-left text-xs font-semibold uppercase tracking-wider text-[#666]">
                    Correct?
                  </th>
                </tr>
              </thead>
              <tbody>
                <tr className="border-b border-[#222]">
                  <td className="px-3 py-2.5 font-mono text-xs">@</td>
                  <td className="px-3 py-2.5 font-mono text-xs">
                    npub1abc...xyz.nodns.shop
                  </td>
                  <td className="px-3 py-2.5 text-[#2ecc71]">
                    ✓ Root domain
                  </td>
                </tr>
                <tr className="border-b border-[#222]">
                  <td className="px-3 py-2.5 font-mono text-xs">www</td>
                  <td className="px-3 py-2.5 font-mono text-xs">
                    www.npub1abc...xyz.nodns.shop
                  </td>
                  <td className="px-3 py-2.5 text-[#2ecc71]">✓ Subdomain</td>
                </tr>
                <tr className="border-b border-[#222]">
                  <td className="px-3 py-2.5 font-mono text-xs">blog</td>
                  <td className="px-3 py-2.5 font-mono text-xs">
                    blog.npub1abc...xyz.nodns.shop
                  </td>
                  <td className="px-3 py-2.5 text-[#2ecc71]">✓ Subdomain</td>
                </tr>
                <tr className="border-b border-[#222]">
                  <td className="px-3 py-2.5 font-mono text-xs text-[#e74c3c]">
                    blog.alice.nodns.shop
                  </td>
                  <td className="px-3 py-2.5 font-mono text-xs text-[#e74c3c]">
                    blog.alice.nodns.shop.npub1abc...xyz.nodns.shop
                  </td>
                  <td className="px-3 py-2.5 text-[#e74c3c]">✗ Wrong!</td>
                </tr>
              </tbody>
            </table>
          </div>
          <p className="mb-3 text-[#bbb]">
            <strong>What happened:</strong> A user wanted to publish a subdomain
            record for their delegated name. They put a full domain path like{" "}
            <code className="font-mono text-[#ff6b35]">blog.alice.nodns.shop</code> in the name
            field, probably intending to create records under a delegated name.
            But the bot treated the entire string as a subdomain label and
            appended their own npub and zone on top.
          </p>
          <p className="text-[#bbb]">
            <strong>How to fix it:</strong> Use just{" "}
            <code className="font-mono text-[#ff6b35]">blog</code>,{" "}
            <code className="font-mono text-[#ff6b35]">api</code>, or{" "}
            <code className="font-mono text-[#ff6b35]">www</code> as the name. If you have a
            delegated name (e.g.{" "}
            <code className="font-mono text-[#ff6b35]">alice.nodns.shop</code>), just use the name
            part that was delegated to you.
          </p>
        </div>

        {/* What should I put in the name field? */}
        <div className="mb-5 rounded-[10px] border border-[#222] bg-[#141414] p-6">
          <h3 className="mb-3 text-lg font-semibold">
            What should I put in the <code>name</code> field?
          </h3>
          <p className="mb-2 text-[#bbb]">
            The <code className="font-mono text-[#ff6b35]">name</code> field is the subdomain label{" "}
            <em>only</em>. The full domain is constructed automatically as:
          </p>
          <pre className="mb-3 overflow-x-auto rounded-lg border border-[#222] bg-[#0a0a0a] p-4 text-[0.85rem] leading-relaxed">
            <code>FQDN = {"{name}.{your_npub}.{zone}"}</code>
          </pre>
          <ul className="mb-3 list-disc pl-5 text-[#bbb]">
            <li>
              <code className="font-mono text-[#ff6b35]">@</code> or empty
              string &mdash; your root domain (
              <code className="font-mono text-xs">npub1abc...xyz.nodns.shop</code>
              )
            </li>
            <li>
              <code className="font-mono text-[#ff6b35]">www</code> &mdash;
              creates{" "}
              <code className="font-mono text-xs">
                www.npub1abc...xyz.nodns.shop
              </code>
            </li>
            <li>
              <code className="font-mono text-[#ff6b35]">blog</code> &mdash;
              creates{" "}
              <code className="font-mono text-xs">
                blog.npub1abc...xyz.nodns.shop
              </code>
            </li>
            <li>Any single label without dots &mdash; treated as a subdomain</li>
          </ul>
          <p className="text-[#bbb]">
            <strong>
              Never put a full domain, another npub, or a zone name in the name
              field.
            </strong>{" "}
            The bot handles zone and npub construction for you.
          </p>
        </div>

        {/* How do I get a human-readable domain? */}
        <div className="mb-5 rounded-[10px] border border-[#222] bg-[#141414] p-6">
          <h3 className="mb-3 text-lg font-semibold">
            How do I get a human-readable domain like{" "}
            <code>alice.nodns.shop</code>?
          </h3>
          <p className="text-[#bbb]">
            Human-readable names require{" "}
            <strong>cryptographic delegation</strong> from a zone registrar. A
            registrar publishes a <code className="font-mono text-[#ff6b35]">delegation</code> tag
            assigning a name (like{" "}
            <code className="font-mono text-[#ff6b35]">alice.nodns.shop</code>) to your npub for a
            fixed period. Once delegated, you publish records with the delegated
            name in the <code className="font-mono text-[#ff6b35]">name</code> field, and the bot
            verifies the delegation before creating DNS records. This is not yet
            available for public signup &mdash; watch the roadmap for updates.
          </p>
        </div>

        {/* Can I publish records for a different zone? */}
        <div className="mb-5 rounded-[10px] border border-[#222] bg-[#141414] p-6">
          <h3 className="mb-3 text-lg font-semibold">
            Can I publish records for a different zone?
          </h3>
          <p className="text-[#bbb]">
            No. You can only publish records for zones that recognize your
            events. Each zone has its own nodns-bot instance subscribed to its
            registrar&apos;s events. Publishing to the{" "}
            <code className="font-mono text-[#ff6b35]">nodns.shop</code> bot only creates records
            under <code className="font-mono text-[#ff6b35]">*.nodns.shop</code>. To publish under a
            different zone, that zone needs its own bot infrastructure. In the
            future, ccTLD operators could run their own nodns-bot to enable
            Nostr-native DNS for entire country-code TLDs.
          </p>
        </div>

        {/* How long until my records are live? */}
        <div className="mb-5 rounded-[10px] border border-[#222] bg-[#141414] p-6">
          <h3 className="mb-3 text-lg font-semibold">
            How long until my records are live?
          </h3>
          <p className="text-[#bbb]">
            Typically 3&ndash;5 seconds from publishing your Nostr event. The
            bot subscribes to relays in real-time, validates the event, and
            pushes changes via DDNS immediately. You can verify with{" "}
            <code className="font-mono text-[0.85rem] text-[#ff6b35]">
              dig @ns1.nodns.shop {"{your_npub}"}.nodns.shop A
            </code>
            .
          </p>
        </div>
      </div>
    </section>
  );
}
