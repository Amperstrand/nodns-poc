/**
 * Deterministic TLS private key derivation from Nostr nsec.
 *
 * Derivation: HMAC-SHA512(key="nodns-tls-v1", data=nsec || 0x00 || subdomain)
 * First 32 bytes of output → P-256 private key
 *
 * This matches the Rust implementation in tls_derivation.rs exactly.
 */

import { hmac } from "@noble/hashes/hmac.js";
import { sha512 } from "@noble/hashes/sha2.js";
import { bytesToHex } from "@noble/hashes/utils.js";
import { p256 } from "@noble/curves/nist.js";

export function deriveTlsKeyBytes(
  nsecBytes: Uint8Array,
  subdomain: string,
): Uint8Array {
  if (nsecBytes.length !== 32) {
    throw new Error("nsec must be exactly 32 bytes");
  }

  const subdomainBytes = new TextEncoder().encode(subdomain);
  const data = new Uint8Array(32 + 1 + subdomainBytes.length);
  data.set(nsecBytes, 0);
  data[32] = 0x00;
  data.set(subdomainBytes, 33);

  const result = hmac(
    sha512,
    new TextEncoder().encode("nodns-tls-v1"),
    data,
  );
  return result.slice(0, 32); // First 32 bytes = P-256 private key
}

function bytesToBase64(bytes: Uint8Array): string {
  let binary = "";
  for (let i = 0; i < bytes.length; i++) {
    binary += String.fromCharCode(bytes[i]);
  }
  return btoa(binary);
}

/**
 * Generate a CryptoKeyPair from the derived TLS key bytes for CSR generation.
 * Uses @noble/curves/p256 for point multiplication to get public key from private key,
 * then imports both into Web Crypto API as a JWK.
 */
export async function generateTlsKeyPair(
  nsecBytes: Uint8Array,
  subdomain: string,
): Promise<{ keyPair: CryptoKeyPair; privateKeyPem: string }> {
  const keyBytes = deriveTlsKeyBytes(nsecBytes, subdomain);

  // Get P-256 public key point from private key bytes using noble/curves
  const pubPoint = p256.getPublicKey(keyBytes, false); // Uncompressed point (65 bytes)
  // pubPoint[0] = 0x04 (uncompressed marker)
  // pubPoint[1..33] = x coordinate
  // pubPoint[33..65] = y coordinate
  const xBytes = pubPoint.slice(1, 33);
  const yBytes = pubPoint.slice(33, 65);

  // Construct JWK for P-256 private key
  const jwk: JsonWebKey = {
    kty: "EC",
    crv: "P-256",
    d: bytesToBase64(keyBytes),
    x: bytesToBase64(xBytes),
    y: bytesToBase64(yBytes),
  };

  // Import as private key with signing capability
  const privateKey = await crypto.subtle.importKey(
    "jwk",
    jwk,
    { name: "ECDSA", namedCurve: "P-256" },
    true,
    ["sign"],
  );

  // Import public key (needed for CryptoKeyPair)
  const publicJwk: JsonWebKey = {
    kty: "EC",
    crv: "P-256",
    x: bytesToBase64(xBytes),
    y: bytesToBase64(yBytes),
  };

  const publicKey = await crypto.subtle.importKey(
    "jwk",
    publicJwk,
    { name: "ECDSA", namedCurve: "P-256" },
    true,
    ["verify"],
  );

  // Export PEM for download
  const privateKeyDer = await crypto.subtle.exportKey("pkcs8", privateKey);
  const privateKeyPem = arrayBufferToPem(privateKeyDer, "PRIVATE KEY");

  return {
    keyPair: { privateKey, publicKey },
    privateKeyPem,
  };
}

function arrayBufferToPem(buffer: ArrayBuffer, label: string): string {
  const bytes = new Uint8Array(buffer);
  const base64 = bytesToBase64(bytes);
  const lines = base64.match(/.{1,64}/g) || [];
  return `-----BEGIN ${label}-----\n${lines.join("\n")}\n-----END ${label}-----`;
}

export { bytesToHex };
