import type { NostrEvent } from "nostr-tools/pure";
import { SimplePool } from "nostr-tools/pool";
import { RELAYS, RECORD_KIND, ZONE_HANDLER_KIND } from "./constants";

export { pubkeyToNpub } from "@nodns/resolver";

export function subscribeToEvents(
  onEvent: (event: NostrEvent) => void,
  onEOSE?: () => void,
): () => void {
  const pool = new SimplePool();
  const filters = [{ kinds: [RECORD_KIND, ZONE_HANDLER_KIND], limit: 100 }] as never;
  const sub = pool.subscribeMany(
    RELAYS,
    filters,
    {
      onevent,
      oneose: () => {
        onEOSE?.();
      },
    },
  );

  function onevent(event: NostrEvent) {
    onEvent(event);
  }

  return () => {
    sub.close();
    pool.close(RELAYS);
  };
}
