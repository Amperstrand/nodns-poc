export function SiteFooter() {
  return (
    <footer className="border-t border-[#222] px-6 py-8 text-center text-sm text-[#666]">
      <p>NoDNS &mdash; DNS from Nostr. Open protocol, no central authority.</p>
      <p className="mt-2">
        <a
          href="https://relay.ngit.dev"
          target="_blank"
          rel="noopener noreferrer"
          className="text-[#ff6b35] hover:underline"
        >
          Protocol Spec
        </a>
        {" "}&middot;{" "}
        <a
          href="https://github.com/nbd-wtf/nostr-tools"
          target="_blank"
          rel="noopener noreferrer"
          className="text-[#ff6b35] hover:underline"
        >
          nostr-tools
        </a>
      </p>
    </footer>
  );
}
