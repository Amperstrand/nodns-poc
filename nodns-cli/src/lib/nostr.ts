import {
  generateSecretKey,
  getPublicKey,
  finalizeEvent,
  type EventTemplate,
  type NostrEvent,
} from "nostr-tools/pure";
import { npubEncode, nsecEncode, decode as nip19Decode } from "nostr-tools/nip19";
import { SimplePool } from "nostr-tools/pool";
import { hexToBytes, bytesToHex } from "nostr-tools/utils";
import { RECORD_KIND } from "./constants.js";
import type { Keypair } from "./types.js";

export { bytesToHex };

export function generateKeypair(): Keypair {
  const sk = generateSecretKey();
  const pk = getPublicKey(sk);
  return {
    secretKey: sk,
    pubkey: pk,
    nsec: nsecEncode(sk),
    npub: npubEncode(pk),
  };
}

export function decodeNsec(nsec: string): Keypair {
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

export function decodeSec(sec: string): Keypair {
  if (sec.startsWith("nsec1")) {
    return decodeNsec(sec);
  }
  const sk = hexToBytes(sec);
  const pk = getPublicKey(sk);
  return {
    secretKey: sk,
    pubkey: pk,
    nsec: nsecEncode(sk),
    npub: npubEncode(pk),
  };
}

export function buildRecordTag(
  recordType: string,
  name: string,
  rdata: string,
  ttl: number = 3600,
): string[] {
  return ["record", recordType.toUpperCase(), name, String(ttl), rdata];
}

export function buildDeleteTag(
  recordType: string,
  name: string,
): string[] {
  return ["record", recordType.toUpperCase(), name, "3600", ""];
}

export function buildCashuTag(
  token: string,
  mintUrl: string,
  amount: number,
): string[] {
  return ["cashu", token, mintUrl, String(amount)];
}

export async function signAndPublish(
  secretKey: Uint8Array,
  relays: string[],
  tags: string[][],
  content: string = "",
  dryRun: boolean = false,
): Promise<NostrEvent> {
  const template: EventTemplate = {
    kind: RECORD_KIND,
    content,
    tags,
    created_at: Math.floor(Date.now() / 1000),
  };

  const event = finalizeEvent(template, secretKey);

  const json = JSON.stringify(event, null, 2);
  console.log(json);

  if (dryRun) {
    console.error("(dry run — not publishing)");
    return event;
  }

  const pool = new SimplePool();
  const results = await Promise.allSettled(
    relays.map(async (url) => {
      await pool.publish([url], event);
      return url;
    }),
  );
  pool.close(relays);

  let succeeded = 0;
  let failed = 0;
  for (const r of results) {
    if (r.status === "fulfilled") {
      console.error(`\u2713 ${r.value} — ${event.id}`);
      succeeded++;
    } else {
      console.error(`\u2717 publish failed: ${r.reason}`);
      failed++;
    }
  }

  if (succeeded === 0 && failed > 0) {
    throw new Error(`Failed to publish to all ${failed} relay(s)`);
  }

  return event;
}

export { fetchEvents } from "@nodns/resolver";
