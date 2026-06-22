import {
  generateSecretKey,
  getPublicKey,
  finalizeEvent,
  type EventTemplate,
  type NostrEvent,
} from "nostr-tools/pure";
import { npubEncode, nsecEncode, decode as nip19Decode } from "nostr-tools/nip19";
import { SimplePool } from "nostr-tools/pool";
import { RELAYS } from "./constants";

export function generateKeypair() {
  const sk = generateSecretKey();
  const pk = getPublicKey(sk);
  return {
    secretKey: sk,
    pubkey: pk,
    nsec: nsecEncode(sk),
    npub: npubEncode(pk),
  };
}

export function decodeNsec(nsec: string) {
  const decoded = nip19Decode(nsec);
  if (decoded.type !== "nsec") throw new Error("Not a valid nsec");
  const sk = decoded.data as Uint8Array;
  const pk = getPublicKey(sk);
  return {
    secretKey: sk,
    pubkey: pk,
    nsec,
    npub: npubEncode(pk),
  };
}

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

export function buildRecordTag(
  recordType: string,
  name: string,
  rdata: string,
  ttl: number = 3600,
): string[] {
  return ["record", recordType.toUpperCase(), name, String(ttl), rdata];
}

export function buildCashuTag(
  token: string,
  mintUrl: string,
  amount: number,
): string[] {
  return ["cashu", token, mintUrl, String(amount)];
}

export async function signAndPublish(
  secretKey: Uint8Array | null,
  tags: string[][],
  content: string = "",
): Promise<NostrEvent> {
  const template: EventTemplate = {
    kind: NOSTR_KIND,
    content,
    tags,
    created_at: Math.floor(Date.now() / 1000),
  };

  let event: NostrEvent;
  if (secretKey) {
    event = finalizeEvent(template, secretKey);
  } else if (isExtensionAvailable()) {
    event = await window.nostr!.signEvent(template);
  } else {
    throw new Error("No signing method available");
  }

  const pool = new SimplePool();
  await Promise.allSettled(RELAYS.map((url) => pool.publish([url], event)));
  pool.close(RELAYS);

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
