"use client";

import { useEffect, useState } from "react";

interface CertDisplayProps {
  certificatePem: string;
  acmeEnvironment: string;
}

interface CertInfo {
  issuer: string;
  subject: string;
  serialNumber: string;
  notBefore: Date;
  notAfter: Date;
  sans: string[];
  signatureAlgorithm: string;
  publicKeyAlgorithm: string;
  publicKeySize: string;
  thumbprintSha256: string;
  validityStatus: "valid" | "expired" | "not_yet_valid";
  daysRemaining: number;
}

interface Asn1Node {
  tag: number;
  content: Uint8Array;
  children: Asn1Node[];
  offset: number;
}

function pemToDer(pem: string): Uint8Array {
  const b64 = pem
    .replace(/-----BEGIN [^-]+-----/g, "")
    .replace(/-----END [^-]+-----/g, "")
    .replace(/\s+/g, "");
  const bin = atob(b64);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  return bytes;
}

function parseAsn1(data: Uint8Array, offset: number): Asn1Node {
  const tag = data[offset++];

  let length: number;
  const lenByte = data[offset++];
  if (lenByte < 0x80) {
    length = lenByte;
  } else {
    const numBytes = lenByte & 0x7f;
    length = 0;
    for (let i = 0; i < numBytes; i++) {
      length = (length << 8) | data[offset++];
    }
  }

  const contentStart = offset;
  const content = data.slice(contentStart, contentStart + length);
  const children: Asn1Node[] = [];
  const isConstructed = (tag & 0x20) !== 0;

  if (isConstructed) {
    let childOffset = contentStart;
    const end = contentStart + length;
    while (childOffset < end) {
      const child = parseAsn1(data, childOffset);
      children.push(child);
      childOffset = child.offset;
    }
  }

  return { tag, content, children, offset: contentStart + length };
}

function decodeOid(data: Uint8Array): string {
  if (data.length === 0) return "";
  const components: number[] = [Math.floor(data[0] / 40), data[0] % 40];
  let val = 0;
  for (let i = 1; i < data.length; i++) {
    val = (val << 7) | (data[i] & 0x7f);
    if (!(data[i] & 0x80)) {
      components.push(val);
      val = 0;
    }
  }
  return components.join(".");
}

function parseTime(node: Asn1Node): Date {
  const str = new TextDecoder().decode(node.content);
  if (node.tag === 0x17) {
    const year = parseInt(str.substring(0, 2));
    const fullYear = year >= 50 ? 1900 + year : 2000 + year;
    return new Date(
      `${fullYear}-${str.substring(2, 4)}-${str.substring(4, 6)}T${str.substring(6, 8)}:${str.substring(8, 10)}:${str.substring(10, 12)}Z`,
    );
  }
  return new Date(
    `${str.substring(0, 4)}-${str.substring(4, 6)}-${str.substring(6, 8)}T${str.substring(8, 10)}:${str.substring(10, 12)}:${str.substring(12, 14)}Z`,
  );
}

const OID_NAMES: Record<string, string> = {
  "2.5.4.3": "CN",
  "2.5.4.6": "C",
  "2.5.4.7": "L",
  "2.5.4.8": "ST",
  "2.5.4.10": "O",
  "2.5.4.11": "OU",
};

function parseDn(seq: Asn1Node): string {
  const parts: string[] = [];
  for (const rdn of seq.children) {
    for (const attr of rdn.children) {
      if (attr.children.length >= 2) {
        const oid = decodeOid(attr.children[0].content);
        const name = OID_NAMES[oid] || oid;
        const value = new TextDecoder().decode(attr.children[1].content);
        parts.push(`${name}=${value}`);
      }
    }
  }
  return parts.join(", ");
}

function parseSans(tbs: Asn1Node): string[] {
  const sans: string[] = [];
  for (let i = 0; i < tbs.children.length; i++) {
    const child = tbs.children[i];
    if (child.tag !== 0xa3) continue;
    const extensionsSeq = child.children[0];
    for (const ext of extensionsSeq.children) {
      if (ext.children.length < 2) continue;
      const oid = decodeOid(ext.children[0].content);
      if (oid !== "2.5.29.17") continue;
      const octetString = ext.children.find((c) => c.tag === 0x04);
      if (!octetString) continue;
      const sanSeq = parseAsn1(
        new Uint8Array(octetString.content.buffer, octetString.content.byteOffset, octetString.content.length),
        0,
      );
      for (const entry of sanSeq.children) {
        if (entry.tag === 0x82) {
          sans.push(new TextDecoder().decode(entry.content));
        }
      }
    }
  }
  return sans;
}

const SIG_ALG_NAMES: Record<string, string> = {
  "1.2.840.113549.1.1.5": "SHA-1 with RSA",
  "1.2.840.113549.1.1.11": "SHA-256 with RSA",
  "1.2.840.113549.1.1.12": "SHA-384 with RSA",
  "1.2.840.10045.4.3.2": "ECDSA with SHA-256",
  "1.2.840.10045.4.3.3": "ECDSA with SHA-384",
};

function getPublicKeyInfo(spki: Asn1Node): { algorithm: string; size: string } {
  const algSeq = spki.children[0];
  const algOid = decodeOid(algSeq.children[0].content);

  if (algOid === "1.2.840.113549.1.1.1") {
    const bitString = spki.children[1];
    const keyData = bitString.content.slice(1);
    const rsaSeq = parseAsn1(keyData, 0);
    const modulusContent = rsaSeq.children[0].content;
    const modulusBytes = modulusContent[0] === 0 ? modulusContent.slice(1) : modulusContent;
    const bitLength = modulusBytes.length * 8;
    const leadingZeros = modulusBytes[0] === 0 ? 0 : Math.clz32(modulusBytes[0]) - 24;
    return { algorithm: "RSA", size: (bitLength - leadingZeros).toString() };
  }

  if (algOid === "1.2.840.10045.2.1") {
    const curveOid = algSeq.children.length > 1 ? decodeOid(algSeq.children[1].content) : "";
    const curveMap: Record<string, string> = {
      "1.2.840.10045.3.1.7": "P-256",
      "1.3.132.0.34": "P-384",
      "1.3.132.0.35": "P-521",
    };
    return { algorithm: "ECDSA", size: curveMap[curveOid] || "unknown" };
  }

  return { algorithm: "Unknown", size: "" };
}

function formatSerialNumber(bytes: Uint8Array): string {
  const hex: string[] = [];
  for (const b of bytes) hex.push(b.toString(16).padStart(2, "0").toUpperCase());
  return hex.join(":");
}

function formatDate(date: Date): string {
  return date.toLocaleDateString("en-US", {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    timeZoneName: "short",
  });
}

async function parseCertificate(pem: string): Promise<CertInfo> {
  const der = pemToDer(pem);
  const cert = parseAsn1(der, 0);

  const tbs = cert.children[0];
  const sigAlgSeq = cert.children[1];

  const serialBytes = tbs.children[1].content;
  const serialHex = formatSerialNumber(serialBytes);

  const issuer = parseDn(tbs.children[3]);
  const validity = tbs.children[4];
  const notBefore = parseTime(validity.children[0]);
  const notAfter = parseTime(validity.children[1]);
  const subject = parseDn(tbs.children[5]);
  const spki = tbs.children[6];

  const sans = parseSans(tbs);

  const sigAlgOid = decodeOid(sigAlgSeq.children[0].content);
  const signatureAlgorithm = SIG_ALG_NAMES[sigAlgOid] || sigAlgOid;

  const { algorithm: publicKeyAlgorithm, size: publicKeySize } = getPublicKeyInfo(spki);

  const digest = await crypto.subtle.digest("SHA-256", der.buffer as ArrayBuffer);
  const hashBytes = new Uint8Array(digest);
  const thumbprintParts: string[] = [];
  for (const b of hashBytes) thumbprintParts.push(b.toString(16).padStart(2, "0").toUpperCase());
  const thumbprintSha256 = thumbprintParts.join(":");

  const now = new Date();
  let validityStatus: CertInfo["validityStatus"] = "valid";
  if (now < notBefore) validityStatus = "not_yet_valid";
  if (now > notAfter) validityStatus = "expired";

  const daysRemaining = Math.floor(
    (notAfter.getTime() - now.getTime()) / (1000 * 60 * 60 * 24),
  );

  return {
    issuer,
    subject,
    serialNumber: serialHex,
    notBefore,
    notAfter,
    sans,
    signatureAlgorithm,
    publicKeyAlgorithm,
    publicKeySize,
    thumbprintSha256,
    validityStatus,
    daysRemaining,
  };
}

export function CertDisplay({ certificatePem, acmeEnvironment }: CertDisplayProps) {
  const [certInfo, setCertInfo] = useState<CertInfo | null>(null);
  const [parseError, setParseError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    parseCertificate(certificatePem)
      .then((info) => {
        if (!cancelled) setCertInfo(info);
      })
      .catch((err) => {
        if (!cancelled) {
          setParseError(
            err instanceof Error ? err.message : "Failed to parse certificate",
          );
        }
      });

    return () => { cancelled = true; };
  }, [certificatePem]);

  if (parseError) {
    return (
      <div className="rounded-lg border border-red-500/25 bg-red-500/8 px-4 py-3 text-sm text-destructive">
        Failed to parse certificate: {parseError}
      </div>
    );
  }

  if (!certInfo) {
    return (
      <div className="rounded-lg border border-border bg-background px-4 py-3 text-sm text-muted-foreground">
        Parsing certificate...
      </div>
    );
  }

  const validityIcon =
    certInfo.validityStatus === "valid"
      ? "✅"
      : certInfo.validityStatus === "expired"
        ? "⚠️"
        : "❌";

  const validityText =
    certInfo.validityStatus === "valid"
      ? `Valid — ${certInfo.daysRemaining} days remaining`
      : certInfo.validityStatus === "expired"
        ? "Expired"
        : "Not yet valid";

  const validityClass =
    certInfo.validityStatus === "valid" ? "text-chart-2" : "text-destructive";

  const isStaging = acmeEnvironment === "staging" || acmeEnvironment === "letsencrypt-staging";

  return (
    <div className="rounded-lg border border-border bg-background p-4">
      <div className="mb-3 flex items-center gap-2">
        <span className="text-sm font-semibold text-foreground">
          Certificate Details
        </span>
        {isStaging && (
          <span className="rounded-full bg-primary/15 px-2 py-0.5 text-[0.6rem] font-bold uppercase tracking-wider text-primary">
            Staging
          </span>
        )}
      </div>

      <div className="space-y-2 font-mono text-xs">
        <CertField label="Issuer" value={certInfo.issuer} />
        <CertField label="Subject" value={certInfo.subject} />
        <CertField
          label="Serial"
          value={certInfo.serialNumber}
        />
        <CertField
          label="Validity"
          value={`${formatDate(certInfo.notBefore)} → ${formatDate(certInfo.notAfter)}`}
        />
        <div className="flex items-start gap-3">
          <span className="shrink-0 text-muted-foreground">Status</span>
          <span className={validityClass}>
            {validityIcon} {validityText}
          </span>
        </div>
        {certInfo.sans.length > 0 && (
          <div className="flex items-start gap-3">
             <span className="shrink-0 text-muted-foreground">SANs</span>
             <div className="flex flex-wrap gap-1">
               {certInfo.sans.map((san, i) => (
                 <span
                   key={i}
                   className="rounded bg-chart-2/10 px-1.5 py-0.5 text-chart-2"
                >
                  {san}
                </span>
              ))}
            </div>
          </div>
        )}
        <CertField
          label="Signature"
          value={certInfo.signatureAlgorithm}
        />
        <CertField
          label="Public Key"
          value={`${certInfo.publicKeyAlgorithm}${certInfo.publicKeySize ? ` (${certInfo.publicKeySize})` : ""}`}
        />
        <div className="flex items-start gap-3">
           <span className="shrink-0 text-muted-foreground">SHA-256</span>
           <span className="break-all text-foreground">
            {certInfo.thumbprintSha256}
          </span>
        </div>
      </div>
    </div>
  );
}

function CertField({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-start gap-3">
      <span className="shrink-0 text-muted-foreground">{label}</span>
      <span className="break-all text-foreground">{value}</span>
    </div>
  );
}
