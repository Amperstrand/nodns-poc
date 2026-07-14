import { SimplePool } from 'nostr-tools/pool';
import { generateSecretKey, getPublicKey, finalizeEvent } from 'nostr-tools/pure';
import { npubEncode, nsecEncode, decode as nip19Decode } from 'nostr-tools/nip19';
import { bytesToHex } from 'nostr-tools/utils';
import { RELAYS, PUBLISH_RELAYS, DEFAULT_POW_DIFFICULTY } from './constants';
import type { NostrEvent, KeyPair } from './types';
import {
  queryRecordsByPubkey as sdkQueryRecordsByPubkey,
  queryRecordsByDomain as sdkQueryRecordsByDomain,
  queryAllRecentRecords as sdkQueryAllRecentRecords,
} from '@nodns/resolver';

export { DEFAULT_POW_DIFFICULTY };

export type MiningPhase = 'mining' | 'publishing';
export type MiningProgressCallback = (phase: MiningPhase) => void;

type EventTemplateData = {
  kind: number;
  tags: string[][];
  content: string;
  created_at: number;
};

type MinedEvent = EventTemplateData & { pubkey: string; id: string };

function minePowAsync(
  template: EventTemplateData,
  difficulty: number,
  pubkey: string,
): Promise<MinedEvent> {
  return new Promise((resolve, reject) => {
    const worker = new Worker(new URL('./pow-worker.ts', import.meta.url));
    const cleanup = () => worker.terminate();
    worker.onmessage = (e: MessageEvent) => {
      const data = e.data as { type: string; mined?: MinedEvent; error?: string };
      cleanup();
      if (data.type === 'done' && data.mined) {
        resolve(data.mined);
      } else {
        reject(new Error(data.error ?? 'PoW mining failed'));
      }
    };
    worker.onerror = (err) => {
      cleanup();
      reject(new Error(err.message ?? 'PoW worker error'));
    };
    worker.postMessage({ template, difficulty, pubkey });
  });
}

export { parseRecordsFromEvent } from '@nodns/resolver';
export type { NostrDnsRecord } from '@nodns/resolver';

import { buildRecordTag, buildCashuTag } from '@nodns/resolver';
export { buildRecordTag, buildCashuTag };

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

export async function publishDnsEvent(
  records: { type: string; name: string; value: string; ttl: number }[],
  secretKey: Uint8Array,
  cashuToken?: string,
  mintUrl?: string,
  satAmount?: number,
  powDifficulty: number = DEFAULT_POW_DIFFICULTY,
  onMiningProgress?: MiningProgressCallback,
): Promise<NostrEvent> {
  const tags = records.map((r) =>
    buildRecordTag(r.type, r.name || '@', r.value, r.ttl),
  );

  if (cashuToken && mintUrl) {
    tags.push(buildCashuTag(cashuToken, mintUrl, satAmount ?? 0));
  }

  const template: EventTemplateData = {
    kind: 11111,
    created_at: Math.floor(Date.now() / 1000),
    tags,
    content: '',
  };

  let finalTemplate: EventTemplateData = template;
  if (powDifficulty > 0) {
    onMiningProgress?.('mining');
    const pubkey = getPublicKey(secretKey);
    const mined = await minePowAsync(template, powDifficulty, pubkey);
    finalTemplate = { kind: mined.kind, tags: mined.tags, content: mined.content, created_at: mined.created_at };
    onMiningProgress?.('publishing');
  }

  const event = finalizeEvent(finalTemplate, secretKey);

  const pubs = pool.publish(PUBLISH_RELAYS, event);
  await Promise.any(pubs);

  return event as NostrEvent;
}

export async function publishDeleteEvent(
  deletes: { type: string; name: string }[],
  secretKey: Uint8Array,
  powDifficulty: number = DEFAULT_POW_DIFFICULTY,
  onMiningProgress?: MiningProgressCallback,
): Promise<NostrEvent> {
  const tags = deletes.map((d) => ['delete', d.type, d.name || '@']);

  const template: EventTemplateData = {
    kind: 11111,
    created_at: Math.floor(Date.now() / 1000),
    tags,
    content: '',
  };

  let finalTemplate: EventTemplateData = template;
  if (powDifficulty > 0) {
    onMiningProgress?.('mining');
    const pubkey = getPublicKey(secretKey);
    const mined = await minePowAsync(template, powDifficulty, pubkey);
    finalTemplate = { kind: mined.kind, tags: mined.tags, content: mined.content, created_at: mined.created_at };
    onMiningProgress?.('publishing');
  }

  const event = finalizeEvent(finalTemplate, secretKey);

  const pubs = pool.publish(PUBLISH_RELAYS, event);
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

export async function queryRecordsByPubkey(
  pubkeyHex: string,
  zone: string,
  opts?: { limit?: number },
) {
  return sdkQueryRecordsByPubkey(pubkeyHex, zone, RELAYS, opts);
}

export async function queryRecordsByDomain(
  fqdn: string,
  zone: string,
  opts?: { limit?: number },
) {
  return sdkQueryRecordsByDomain(fqdn, zone, RELAYS, opts);
}

export async function queryAllRecentRecords(
  zone: string,
  opts?: { limit?: number },
) {
  return sdkQueryAllRecentRecords(zone, RELAYS, opts);
}
