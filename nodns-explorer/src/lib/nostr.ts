import type { NostrEvent } from "nostr-tools/pure";
import { npubEncode } from "nostr-tools/nip19";
import { SimplePool } from "nostr-tools/pool";
import { RELAYS, RECORD_KIND, ZONE_HANDLER_KIND } from "./constants";

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

export function pubkeyToNpub(pubkey: string): string {
  try {
    return npubEncode(pubkey);
  } catch {
    return pubkey;
  }
}
