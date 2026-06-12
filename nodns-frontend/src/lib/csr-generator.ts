import * as x509 from "@peculiar/x509";

export interface CsrResult {
  csrDerBase64: string;
  csrPem: string;
}

export async function generateCsr(
  keyPair: CryptoKeyPair,
  domain: string,
): Promise<CsrResult> {
  const csr = await x509.Pkcs10CertificateRequestGenerator.create({
    name: `CN=${domain}`,
    keys: keyPair,
    signingAlgorithm: { name: "ECDSA", hash: "SHA-256" },
    extensions: [
      new x509.SubjectAlternativeNameExtension([
        { type: "dns", value: domain },
      ]),
    ],
  });

  const csrPem = csr.toString("pem");
  const csrDerBase64 = btoa(
    String.fromCharCode(...new Uint8Array(csr.rawData)),
  );

  return { csrDerBase64, csrPem };
}
