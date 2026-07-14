import { describe, it, expect } from 'vitest';
import {
  generateKeypair,
  decodeNsec,
  buildRecordTag,
  buildDeleteTag,
  buildCashuTag,
  pubkeyToNpub,
} from './nostr.js';

describe('generateKeypair', () => {
  it('returns a valid keypair with all fields', () => {
    const kp = generateKeypair();
    expect(kp.secretKey).toBeInstanceOf(Uint8Array);
    expect(kp.secretKey.length).toBe(32);
    expect(kp.pubkey).toMatch(/^[0-9a-f]{64}$/);
    expect(kp.nsec).toMatch(/^nsec1/);
    expect(kp.npub).toMatch(/^npub1/);
  });

  it('generates unique keypairs', () => {
    const a = generateKeypair();
    const b = generateKeypair();
    expect(a.secretKey).not.toEqual(b.secretKey);
    expect(a.pubkey).not.toBe(b.pubkey);
  });
});

describe('decodeNsec', () => {
  it('round-trips generateKeypair output', () => {
    const kp = generateKeypair();
    const decoded = decodeNsec(kp.nsec);
    expect(decoded.pubkey).toBe(kp.pubkey);
    expect(decoded.nsec).toBe(kp.nsec);
    expect(decoded.npub).toBe(kp.npub);
  });

  it('throws on invalid input', () => {
    expect(() => decodeNsec('garbage')).toThrow();
    expect(() => decodeNsec('not-a-key')).toThrow();
  });
});

describe('buildRecordTag', () => {
  it('builds correct 5-element tag', () => {
    const tag = buildRecordTag('A', 'alice', '1.2.3.4', 3600);
    expect(tag).toEqual(['record', 'A', 'alice', '3600', '1.2.3.4']);
  });

  it('uppercases record type', () => {
    const tag = buildRecordTag('txt', 'test', 'hello', 300);
    expect(tag[1]).toBe('TXT');
  });

  it('uses default TTL of 3600', () => {
    const tag = buildRecordTag('A', '', '1.2.3.4');
    expect(tag[3]).toBe('3600');
  });
});

describe('buildDeleteTag', () => {
  it('builds correct delete tag', () => {
    const tag = buildDeleteTag('A', 'alice');
    expect(tag).toEqual(['record', 'A', 'alice', '3600', '']);
  });

  it('uppercases record type', () => {
    const tag = buildDeleteTag('cname', 'test');
    expect(tag[1]).toBe('CNAME');
  });
});

describe('buildCashuTag', () => {
  it('builds correct cashu tag', () => {
    const tag = buildCashuTag('cashuAtoken', 'https://testnut.cashu.space', 100);
    expect(tag).toEqual(['cashu', 'cashuAtoken', 'https://testnut.cashu.space', '100']);
  });

  it('converts amount to string', () => {
    const tag = buildCashuTag('tok', 'mint', 42);
    expect(tag[3]).toBe('42');
    expect(typeof tag[3]).toBe('string');
  });
});

describe('pubkeyToNpub', () => {
  it('encodes a hex pubkey', () => {
    const hex = 'a'.repeat(64);
    const npub = pubkeyToNpub(hex);
    expect(npub).toMatch(/^npub1/);
  });

  it('returns input on failure', () => {
    expect(pubkeyToNpub('invalid')).toBe('invalid');
  });
});
