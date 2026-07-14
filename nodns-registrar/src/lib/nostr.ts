import {
  getPublicKey,
  finalizeEvent,
  type EventTemplate,
  type NostrEvent,
} from "nostr-tools/pure";
import { SimplePool } from "nostr-tools/pool";
import { minePow } from "nostr-tools/nip13";
import { RELAYS, PUBLISH_RELAYS, DEFAULT_POW_DIFFICULTY } from "./constants";

export {
  generateKeypair,
  decodeNsec,
  buildRecordTag,
  buildCashuTag,
} from "@nodns/resolver";

export function isExtensionAvailable(): boolean {
  return typeof window !== "undefined" && !!window.nostr;
}

declare global {
  interface Window {
    nostr?: {
      getPublicKey(): Promise<string>;
      signEvent(event: EventTemplate): Promise<NostrEvent>;
      nip04?: {
        encrypt(pubkey: string, plaintext: string): Promise<string>;
        decrypt(pubkey: string, ciphertext: string): Promise<string>;
      };
      nip44?: {
        encrypt(pubkey: string, plaintext: string): Promise<string>;
        decrypt(pubkey: string, ciphertext: string): Promise<string>;
      };
    };
  }
}

const NOSTR_KIND = 11111;

export async function signAndPublish(
  secretKey: Uint8Array | null,
  tags: string[][],
  content: string = "",
  powDifficulty: number = DEFAULT_POW_DIFFICULTY,
): Promise<NostrEvent> {
  let template: EventTemplate = {
    kind: NOSTR_KIND,
    content,
    tags,
    created_at: Math.floor(Date.now() / 1000),
  };

  if (powDifficulty > 0) {
    let pubkey: string;
    if (secretKey) {
      pubkey = getPublicKey(secretKey);
    } else if (isExtensionAvailable()) {
      pubkey = await window.nostr!.getPublicKey();
    } else {
      throw new Error("No signing method available");
    }
    const pubkeyTagged = { ...template, pubkey };
    template = minePow(pubkeyTagged, powDifficulty);
  }

  let event: NostrEvent;
  if (secretKey) {
    event = finalizeEvent(template, secretKey);
  } else if (isExtensionAvailable()) {
    event = await window.nostr!.signEvent(template);
  } else {
    throw new Error("No signing method available");
  }

  const pool = new SimplePool();
  await Promise.allSettled(PUBLISH_RELAYS.map((url) => pool.publish([url], event)));
  pool.close(PUBLISH_RELAYS);

  return event;
}

export const publishAndBroadcast = signAndPublish;

export function subscribeToRecords(
  pubkey: string,
  onEvent: (event: NostrEvent) => void,
): () => void {
  const pool = new SimplePool();
  const filters = [{ kinds: [NOSTR_KIND], authors: [pubkey] }] as never;
  const sub = pool.subscribeMany(RELAYS, filters, { onevent: onEvent });
  return () => {
    sub.close();
    pool.close(RELAYS);
  };
}
