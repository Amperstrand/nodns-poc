import { generateSecretKey, getPublicKey } from "nostr-tools/pure";
import { bytesToHex, hexToBytes } from "nostr-tools/utils";
import { npubEncode, nsecEncode, decode as nip19Decode } from "nostr-tools/nip19";
import type { SavedAccount } from "./types";

const ACCOUNTS_KEY = "nodns_registrar_accounts";
const LAST_ACCOUNT_KEY = "nodns_registrar_last_account";
const EPHEMERAL_KEY = "nodns_registrar_ephemeral";
const WALLET_SEED_KEY = "nodns_registrar_wallet_seed";

function toUint8Array(data: Uint8Array | string): Uint8Array {
  if (data instanceof Uint8Array) return data;
  if (typeof data === "string") return hexToBytes(data);
  return new Uint8Array(data);
}

export function getAccounts(): SavedAccount[] {
  if (typeof window === "undefined") return [];
  try {
    const raw = localStorage.getItem(ACCOUNTS_KEY);
    if (!raw) return [];
    const arr = JSON.parse(raw);
    return Array.isArray(arr) ? arr : [];
  } catch {
    return [];
  }
}

export function saveAccount(pubkey: string, nsec: string): void {
  const accounts = getAccounts();
  const npub = npubEncode(pubkey);
  const idx = accounts.findIndex((a) => a.pubkey === pubkey);
  if (idx >= 0) {
    accounts[idx] = { pubkey, nsec, npub, addedAt: Date.now() };
  } else {
    accounts.push({ pubkey, nsec, npub, addedAt: Date.now() });
  }
  localStorage.setItem(ACCOUNTS_KEY, JSON.stringify(accounts));
}

export function removeAccount(pubkey: string): void {
  const accounts = getAccounts().filter((a) => a.pubkey !== pubkey);
  localStorage.setItem(ACCOUNTS_KEY, JSON.stringify(accounts));
}

export function getLastAccount(): string | null {
  return localStorage.getItem(LAST_ACCOUNT_KEY);
}

export function setLastAccount(pubkey: string): void {
  localStorage.setItem(LAST_ACCOUNT_KEY, pubkey);
}

export function clearLastAccount(): void {
  localStorage.removeItem(LAST_ACCOUNT_KEY);
}

export function getEphemeralNsec(): string | null {
  return localStorage.getItem(EPHEMERAL_KEY);
}

export function setEphemeralNsec(nsec: string): void {
  localStorage.setItem(EPHEMERAL_KEY, nsec);
}

export function generateEphemeral() {
  const sk = generateSecretKey();
  const pk = getPublicKey(sk);
  const nsec = nsecEncode(sk);
  setEphemeralNsec(nsec);
  return { secretKey: sk, pubkey: pk, nsec, npub: npubEncode(pk) };
}

export function nsecToSeed(nsec: string): Uint8Array {
  const decoded = nip19Decode(nsec);
  if (decoded.type !== "nsec") throw new Error("Expected nsec");
  const sk = toUint8Array(decoded.data);
  const seed = new Uint8Array(64);
  seed.set(sk, 0);
  seed.set(sk, 32);
  return seed;
}

export function getWalletSeed(): Uint8Array {
  const stored = localStorage.getItem(WALLET_SEED_KEY);
  if (stored) {
    const bytes = hexToBytes(stored);
    if (bytes.length === 64) return bytes;
  }
  const seed = new Uint8Array(64);
  crypto.getRandomValues(seed);
  localStorage.setItem(WALLET_SEED_KEY, bytesToHex(seed));
  return seed;
}

export { bytesToHex };
