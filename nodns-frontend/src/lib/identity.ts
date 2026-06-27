import { generateSecretKey, getPublicKey } from 'nostr-tools/pure';
import { bytesToHex, hexToBytes } from 'nostr-tools/utils';
import { npubEncode, nsecEncode, decode as nip19Decode } from 'nostr-tools/nip19';

const STORAGE_KEY = 'nodns-identity';

function toUint8Array(data: Uint8Array | string): Uint8Array {
  if (data instanceof Uint8Array) return data;
  if (typeof data === 'string') return hexToBytes(data);
  return new Uint8Array(data);
}

export interface Identity {
  sk: Uint8Array;
  pk: string;
  npub: string;
  nsec: string;
}

export function getOrCreateIdentity(): Identity {
  const stored = localStorage.getItem(STORAGE_KEY);
  if (stored) {
    const decoded = nip19Decode(stored);
    if (decoded.type === 'nsec') {
      const sk = toUint8Array(decoded.data);
      const pk = getPublicKey(sk);
      return {
        sk,
        pk,
        npub: npubEncode(pk),
        nsec: stored,
      };
    }
  }

  // Generate new
  const sk = generateSecretKey();
  const pk = getPublicKey(sk);
  const nsec = nsecEncode(sk);
  localStorage.setItem(STORAGE_KEY, nsec);

  return { sk, pk, npub: npubEncode(pk), nsec };
}

export function importIdentity(nsec: string): Identity {
  const trimmed = nsec.trim();
  if (!trimmed.startsWith('nsec1')) {
    throw new Error('Invalid key: must start with nsec1');
  }

  const decoded = nip19Decode(trimmed);
  if (decoded.type !== 'nsec') {
    throw new Error('Invalid key: expected nsec format');
  }

  const sk = toUint8Array(decoded.data);
  const pk = getPublicKey(sk);
  const encoded = nsecEncode(sk);

  localStorage.setItem(STORAGE_KEY, encoded);

  return { sk, pk, npub: npubEncode(pk), nsec: encoded };
}

export function resetIdentity(): Identity {
  const sk = generateSecretKey();
  const pk = getPublicKey(sk);
  const nsec = nsecEncode(sk);

  localStorage.setItem(STORAGE_KEY, nsec);

  return { sk, pk, npub: npubEncode(pk), nsec };
}

export function nsecToSeed(nsec: string): Uint8Array {
  const decoded = nip19Decode(nsec);
  if (decoded.type !== 'nsec') throw new Error('Expected nsec');
  const sk = toUint8Array(decoded.data);
  const seed = new Uint8Array(64);
  seed.set(sk, 0);
  seed.set(sk, 32);
  return seed;
}

export function hexPk(npub: string): string {
  const decoded = nip19Decode(npub);
  if (decoded.type !== 'npub') throw new Error('Expected npub');
  return bytesToHex(toUint8Array(decoded.data));
}
