export function SiteFooter() {
  return (
    <footer className="border-t border-border px-6 py-8 text-center text-sm text-muted-foreground">
      <p>NoDNS &mdash; DNS from Nostr. Open protocol, no central authority.</p>
      <p className="mt-2">
        <a
          href="https://nodns.shop"
          className="text-primary hover:underline"
        >
          nodns.shop
        </a>
        <span className="mx-2">&middot;</span>
        <a
          href="https://github.com/Amperstrand/nodns-poc"
          className="text-primary hover:underline"
          target="_blank"
          rel="noopener noreferrer"
        >
          GitHub
        </a>
      </p>
    </footer>
  );
}
