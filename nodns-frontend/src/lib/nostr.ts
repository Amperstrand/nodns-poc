import { SimplePool } from 'nostr-tools/pool';
import { generateSecretKey, getPublicKey, finalizeEvent } from 'nostr-tools/pure';
import { npubEncode, nsecEncode, decode as nip19Decode } from 'nostr-tools/nip19';
import { bytesToHex } from 'nostr-tools/utils';
import { RELAYS } from './constants';
import type { NostrEvent, KeyPair } from './types';

const pool = new SimplePool();

export function generateEphemeralKeyPair(): KeyPair {
  const sk = generateSecretKey();
  const pk = getPublicKey(sk);
  const npub = npubEncode(pk);
  const nsec = nsecEncode(sk);
  return { secretKey: sk, publicKey: pk, npub, nsec };
}

export function keyPairFromNsec(nsec: string): KeyPair {
  const decoded = nip19Decode(nsec);
  if (decoded.type !== 'nsec') {
    throw new Error('Invalid nsec key');
  }
  const sk = decoded.data as Uint8Array;
  const pk = getPublicKey(sk);
  const npub = npubEncode(pk);
  return { secretKey: sk, publicKey: pk, npub, nsec };
}

export function secretKeyToHex(sk: Uint8Array): string {
  return bytesToHex(sk);
}

export function buildRecordTag(type: string, name: string, rdata: string, ttl: number): string[] {
  return ['record', type, name, rdata, '', '', '', '', '', '', String(ttl)];
}

export function buildCashuTag(token: string, mintUrl: string, amount: string): string[] {
  return ['cashu', token, mintUrl, amount];
}

export async function publishDnsEvent(
  records: { type: string; name: string; value: string; ttl: number }[],
  secretKey: Uint8Array,
  cashuToken?: string,
): Promise<NostrEvent> {
  const tags = records.map((r) =>
    buildRecordTag(r.type, r.name || '@', r.value, r.ttl),
  );

  if (cashuToken) {
    tags.push(buildCashuTag(cashuToken, 'https://testnut.cashu.space', '250'));
  }

  const template = {
    kind: 11111,
    created_at: Math.floor(Date.now() / 1000),
    tags,
    content: '',
  };

  const event = finalizeEvent(template, secretKey);

  const pubs = pool.publish(RELAYS, event);
  await Promise.any(pubs);

  return event as NostrEvent;
}

export async function publishDeleteEvent(
  deletes: { type: string; name: string }[],
  secretKey: Uint8Array,
): Promise<NostrEvent> {
  const tags = deletes.map((d) => ['delete', d.type, d.name || '@']);

  const template = {
    kind: 11111,
    created_at: Math.floor(Date.now() / 1000),
    tags,
    content: '',
  };

  const event = finalizeEvent(template, secretKey);

  const pubs = pool.publish(RELAYS, event);
  await Promise.any(pubs);

  return event as NostrEvent;
}

export function subscribeToDnsEvents(
  onEvent: (event: NostrEvent, relay: string) => void,
): () => void {
  const sub = pool.subscribeMany(
    RELAYS,
    { kinds: [11111], limit: 20 },
    {
      onevent(event) {
        onEvent(event as unknown as NostrEvent, '');
      },
    },
  );

  return () => {
    sub.close();
  };
}
