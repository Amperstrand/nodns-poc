export interface CsrResult {
  csrDerBase64: string;
  csrPem: string;
}

export async function generateCsr(
  keyPair: CryptoKeyPair,
  domain: string,
): Promise<CsrResult> {
  const OID_CN = encodeOid([2, 5, 4, 3]);
  const OID_EXTENSION_REQUEST = encodeOid([1, 2, 840, 113549, 1, 9, 14]);
  const OID_SAN = encodeOid([2, 5, 29, 17]);
  const OID_ECDSA_SHA256 = encodeOid([1, 2, 840, 10045, 4, 3, 2]);

  const version = derTag(0x02, new Uint8Array([0x00]));
  const subject = derSeq(
    derSet(derSeq(OID_CN, derTag(0x0c, new TextEncoder().encode(domain)))),
  );
  const spki = new Uint8Array(
    await crypto.subtle.exportKey("spki", keyPair.publicKey),
  );

  const sanExt = derSeq(
    OID_SAN,
    derTag(0x04, derSeq(derTag(0x82, new TextEncoder().encode(domain)))),
  );
  const attributes = derTag(
    0xa0,
    derSet(derSeq(OID_EXTENSION_REQUEST, derSet(derSeq(sanExt)))),
  );

  const csrInfo = derSeq(version, subject, spki, attributes);

  const rawSig = new Uint8Array(
    await crypto.subtle.sign(
      { name: "ECDSA", hash: "SHA-256" },
      keyPair.privateKey,
      csrInfo.buffer as ArrayBuffer,
    ),
  );
  const sigDer = rawToDerSignature(rawSig);

  const csr = derSeq(
    csrInfo,
    derSeq(OID_ECDSA_SHA256),
    derTag(0x03, concat(new Uint8Array([0x00]), sigDer)),
  );

  const csrDerBase64 = toBase64(csr);
  const lines = csrDerBase64.match(/.{1,64}/g) ?? [];
  const csrPem = `-----BEGIN CERTIFICATE REQUEST-----\n${lines.join("\n")}\n-----END CERTIFICATE REQUEST-----`;

  return { csrDerBase64, csrPem };
}

function encodeOid(components: number[]): Uint8Array {
  const bytes: number[] = [components[0] * 40 + components[1]];
  for (let i = 2; i < components.length; i++) {
    let val = components[i];
    if (val < 0x80) {
      bytes.push(val);
    } else {
      const encoded: number[] = [];
      encoded.push(val & 0x7f);
      val >>= 7;
      while (val > 0) {
        encoded.push(0x80 | (val & 0x7f));
        val >>= 7;
      }
      encoded.reverse();
      bytes.push(...encoded);
    }
  }
  return derTag(0x06, new Uint8Array(bytes));
}

function derTag(tag: number, content: Uint8Array): Uint8Array {
  const len = encodeLength(content.length);
  const out = new Uint8Array(1 + len.length + content.length);
  out[0] = tag;
  out.set(len, 1);
  out.set(content, 1 + len.length);
  return out;
}

function encodeLength(len: number): Uint8Array {
  if (len < 0x80) return new Uint8Array([len]);
  if (len < 0x100) return new Uint8Array([0x81, len]);
  return new Uint8Array([0x82, (len >> 8) & 0xff, len & 0xff]);
}

function derSeq(...items: Uint8Array[]): Uint8Array {
  return derTag(0x30, concat(...items));
}

function derSet(...items: Uint8Array[]): Uint8Array {
  return derTag(0x31, concat(...items));
}

function concat(...arrays: Uint8Array[]): Uint8Array {
  const total = arrays.reduce((s, a) => s + a.length, 0);
  const out = new Uint8Array(total);
  let off = 0;
  for (const a of arrays) {
    out.set(a, off);
    off += a.length;
  }
  return out;
}

function rawToDerSignature(raw: Uint8Array): Uint8Array {
  const half = raw.length >> 1;
  const r = trimUint(raw.slice(0, half));
  const s = trimUint(raw.slice(half));
  return derSeq(derInt(r), derInt(s));
}

function trimUint(bytes: Uint8Array): Uint8Array {
  let start = 0;
  while (start < bytes.length - 1 && bytes[start] === 0) start++;
  return bytes.slice(start);
}

function derInt(value: Uint8Array): Uint8Array {
  if (value[0] & 0x80) {
    const padded = new Uint8Array(value.length + 1);
    padded.set(value, 1);
    return derTag(0x02, padded);
  }
  return derTag(0x02, value);
}

function toBase64(data: Uint8Array): string {
  let bin = "";
  for (let i = 0; i < data.length; i++) bin += String.fromCharCode(data[i]);
  return btoa(bin);
}
