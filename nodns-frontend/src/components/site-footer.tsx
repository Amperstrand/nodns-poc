export function SiteFooter() {
  return (
    <footer className="border-t border-border px-6 py-8 text-center text-base text-foreground/70">
      <p>NoDNS &mdash; DNS from Nostr. Open protocol, no central authority.</p>
      <p className="mt-2 flex items-center justify-center gap-1">
        <a
          href="https://nodns.shop"
          className="text-primary hover:underline px-2 py-1 rounded-md hover:bg-secondary/50 transition-colors"
        >
          nodns.shop
        </a>
        <span className="text-border">&middot;</span>
        <a
          href="https://github.com/Amperstrand/nodns-poc"
          className="text-primary hover:underline px-2 py-1 rounded-md hover:bg-secondary/50 transition-colors"
          target="_blank"
          rel="noopener noreferrer"
        >
          GitHub
        </a>
      </p>
    </footer>
  );
}
