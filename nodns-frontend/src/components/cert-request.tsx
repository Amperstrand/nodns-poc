"use client";

import { useState, useEffect, useCallback, useRef } from "react";import {
  requestCertificate,
  getCertificateOrder,
  type AcmeOrderStatus,
} from "@/lib/api";
import { generateTlsKeyPair } from "@/lib/tls-derivation";
import { generateCsr } from "@/lib/csr-generator";
import { CertDisplay } from "@/components/cert-display";
import { AcmeLogDisplay } from "@/components/acme-log-display";

function downloadFile(content: string, filename: string) {
  const blob = new Blob([content], { type: "text/plain" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

function CopyButton({ text, label }: { text: string; label: string }) {
  const [copied, setCopied] = useState(false);
  const timeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    return () => {
      if (timeoutRef.current) clearTimeout(timeoutRef.current);
    };
  }, []);

  const handleCopy = useCallback(() => {
    navigator.clipboard.writeText(text);
    setCopied(true);
    if (timeoutRef.current) clearTimeout(timeoutRef.current);
    timeoutRef.current = setTimeout(() => setCopied(false), 2000);
  }, [text]);

  return (
    <button
      onClick={handleCopy}
      className="rounded bg-[#222] px-2 py-0.5 text-[0.7rem] text-[#666] transition-colors hover:text-[#e0e0e0]"
    >
      {copied ? "Copied!" : label}
    </button>
  );
}

interface CertRequestProps {
  domain: string;
  disabled: boolean;
  nsecBytes: Uint8Array | null;
  npub: string;
}

export function CertRequest({ domain, disabled, nsecBytes, npub }: CertRequestProps) {
  const [orderId, setOrderId] = useState<string | null>(null);
  const [orderStatus, setOrderStatus] = useState<AcmeOrderStatus | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showKey, setShowKey] = useState(false);
  const [derivedPrivateKeyPem, setDerivedPrivateKeyPem] = useState<string | null>(null);
  const [derivationError, setDerivationError] = useState<string | null>(null);
  const [acmeCa, setAcmeCa] = useState<"zerossl" | "letsencrypt-staging" | "letsencrypt-production">("letsencrypt-staging");

  // Track if we used client-side key derivation for this order
  const usedClientDerivation = useRef(false);

  // Extract subdomain from domain (e.g., "blog.npub123.nodns.shop" → "blog")
  // Or full npub.nodns.shop → use npub as subdomain for derivation
  const subdomain = domain.replace(/\.nodns\.shop$/, "");

  // Polling
  useEffect(() => {
    if (!orderId) return;
    if (
      orderStatus?.status === "issued" ||
      orderStatus?.status === "failed"
    )
      return;

    const interval = setInterval(async () => {
      try {
        const order = await getCertificateOrder(orderId, npub);
        setOrderStatus(order);
      } catch {
        // Keep polling on transient errors
      }
    }, 3000);

    return () => clearInterval(interval);
  }, [orderId, orderStatus?.status]);

  const handleRequest = useCallback(async () => {
    if (disabled || loading) return;
    setLoading(true);
    setError(null);
    setDerivationError(null);
    setOrderStatus(null);
    setOrderId(null);
    setShowKey(false);
    setDerivedPrivateKeyPem(null);
    usedClientDerivation.current = false;

    let csrDerBase64: string | undefined;

    // If nsecBytes available, derive key and generate CSR client-side
    if (nsecBytes) {
      try {
        const { keyPair, privateKeyPem } = await generateTlsKeyPair(
          nsecBytes,
          subdomain,
        );
        setDerivedPrivateKeyPem(privateKeyPem);
        usedClientDerivation.current = true;

        const csrResult = await generateCsr(keyPair, domain);
        csrDerBase64 = csrResult.csrDerBase64;
      } catch (err) {
        const msg = err instanceof Error ? err.message : "Key derivation failed";
        setDerivationError(msg);
        // Fall back to server-generated key
      }
    }

    try {
      const res = await requestCertificate(domain, csrDerBase64, undefined, acmeCa, npub);
      setOrderId(res.order_id);
      setOrderStatus({
        order_id: res.order_id,
        status: res.status as AcmeOrderStatus["status"],
        domain,
        certificate_pem: null,
        private_key_pem: null,
        error: null,
        acme_environment: "",
        logs: [],
      });
    } catch (err) {
      const msg = err instanceof Error ? err.message : "Request failed";
      setError(msg);
    }
    setLoading(false);
  }, [domain, subdomain, disabled, loading, nsecBytes, acmeCa]);

  const handleReset = useCallback(() => {
    setError(null);
    setOrderId(null);
    setOrderStatus(null);
    setShowKey(false);
    setDerivedPrivateKeyPem(null);
    setDerivationError(null);
    usedClientDerivation.current = false;
  }, []);

  const status = orderStatus?.status;
  const environment = orderStatus?.acme_environment || "";
  const isInProgress =
    status === "pending" ||
    status === "challenge_published" ||
    status === "verifying";
  const logs = orderStatus?.logs || [];

  // Determine which private key PEM to use for download
  const effectivePrivateKeyPem =
    derivedPrivateKeyPem || orderStatus?.private_key_pem || null;

  return (
    <div className="mt-6">
      <div className="mb-6 border-t border-[#222]" />

      <div className="rounded-[10px] border border-[#222] bg-[#141414] p-5">
        <div className="mb-3 flex items-center gap-3">
          <h3 className="text-lg font-semibold">HTTPS Certificate</h3>
          {environment && (
            <span
              className={`rounded-full px-2.5 py-0.5 text-[0.6rem] font-bold uppercase tracking-wider ${
                environment === "zerossl"
                  ? "bg-[rgba(52,152,219,0.15)] text-[#3498db]"
                  : environment === "production" || environment === "letsencrypt-production"
                    ? "bg-[rgba(46,204,113,0.15)] text-[#2ecc71]"
                    : "bg-[rgba(255,107,53,0.15)] text-[#ff6b35]"
              }`}
            >
              {environment === "zerossl"
                ? "ZeroSSL"
                : environment === "production" || environment === "letsencrypt-production"
                  ? "LE Production"
                  : "LE Staging"}
            </span>
          )}
        </div>

        {/* Request button */}
        {!orderId && !error && (
          <>
            <div className="mb-3 flex items-center gap-4 text-xs">
              <label className="flex cursor-pointer items-center gap-1.5">
                <input
                  type="radio"
                  name="acme-ca"
                  value="zerossl"
                  checked={acmeCa === "zerossl"}
                  onChange={() => setAcmeCa("zerossl")}
                  className="accent-[#3498db]"
                />
                <span className="text-[#3498db] font-semibold">ZeroSSL</span>
              </label>
              <label className="flex cursor-pointer items-center gap-1.5">
                <input
                  type="radio"
                  name="acme-ca"
                  value="letsencrypt-staging"
                  checked={acmeCa === "letsencrypt-staging"}
                  onChange={() => setAcmeCa("letsencrypt-staging")}
                  className="accent-[#ff6b35]"
                />
                <span className="text-[#ff6b35] font-semibold">LE Staging</span>
              </label>
              <label className="flex cursor-pointer items-center gap-1.5">
                <input
                  type="radio"
                  name="acme-ca"
                  value="letsencrypt-production"
                  checked={acmeCa === "letsencrypt-production"}
                  onChange={() => setAcmeCa("letsencrypt-production")}
                  className="accent-[#2ecc71]"
                />
                <span className="text-[#2ecc71] font-semibold">LE Production</span>
              </label>
            </div>
            {acmeCa === "letsencrypt-production" && (
              <p className="mb-3 text-xs text-[#e8a49c]">
                ⚠️ Production certificates count against Let's Encrypt rate limits (5 per week per domain). Use Staging for testing.
              </p>
            )}
            <button
              onClick={handleRequest}
              disabled={disabled || loading}
              className="rounded-lg bg-[#222] px-4 py-2.5 text-sm font-semibold text-[#e0e0e0] transition-colors hover:bg-[#333] disabled:cursor-not-allowed disabled:opacity-50"
            >
              {loading ? "Requesting..." : "Get HTTPS Certificate"}
            </button>
            <p className="mt-2 text-xs text-[#666]">
              {nsecBytes ? (
                <>
                  🔒 Your private key is derived from your nsec and never leaves
                  your browser. The bot receives only a Certificate Signing
                  Request (CSR).
                </>
              ) : (
                <>
                  🔒 Your private key is generated by the bot and returned to
                  you. The bot does not retain it after delivery.
                </>
              )}
            </p>
            {disabled && (
              <p className="mt-1 text-xs text-[#666]">
                Publish at least one DNS record first.
              </p>
            )}
            {derivationError && (
              <p className="mt-1 text-xs text-[#ff6b35]">
                ⚠️ Key derivation failed ({derivationError}). Falling back to
                server-generated key.
              </p>
            )}
          </>
        )}

        {/* Error */}
        {error && (
          <div className="space-y-3">
            <div className="rounded-lg border border-[rgba(231,76,60,0.25)] bg-[rgba(231,76,60,0.08)] px-4 py-3 text-sm text-[#e74c3c]">
              ❌ {error}
            </div>
            <button
              onClick={handleReset}
              className="rounded-lg bg-[#222] px-3 py-2 text-xs font-semibold text-[#e0e0e0] hover:bg-[#333]"
            >
              Try Again
            </button>
          </div>
        )}

        {/* In-progress: ACME logs */}
        {orderStatus && isInProgress && (
          <div className="space-y-3">
            <div className="flex items-center gap-3 rounded-lg border border-[#222] bg-[#0a0a0a] px-4 py-3">
              <span className="inline-block h-3 w-3 animate-pulse rounded-full bg-[#ff6b35]" />
              <span className="text-sm text-[#ff6b35]">
                {status === "pending" && "Creating certificate order..."}
                {status === "challenge_published" &&
                  "DNS challenge published, waiting for verification..."}
                {status === "verifying" &&
                  "CA is verifying your domain..."}
              </span>
            </div>
            <AcmeLogDisplay logs={logs} isComplete={false} />
            <p className="text-xs text-[#666]">
              Domain: <code className="text-[#bbb]">{orderStatus.domain}</code>
            </p>
          </div>
        )}

        {/* Issued */}
        {orderStatus && status === "issued" && (
          <div className="space-y-4">
            <div className="rounded-lg border border-[rgba(46,204,113,0.25)] bg-[rgba(46,204,113,0.08)] px-4 py-3 text-center">
              <p className="text-sm font-semibold text-[#2ecc71]">
                ✅ Certificate ready!
              </p>
              <p className="mt-1 text-xs text-[#666]">
                Certificate valid for 90 days
              </p>
            </div>

            {/* ACME log (completed) */}
            {logs.length > 0 && (
              <AcmeLogDisplay logs={logs} isComplete={true} />
            )}

            {/* Cert details */}
            {orderStatus.certificate_pem && (
              <CertDisplay
                certificatePem={orderStatus.certificate_pem}
                acmeEnvironment={environment}
              />
            )}

            {/* Download buttons */}
            {orderStatus.certificate_pem && (
              <div className="flex flex-wrap gap-2">
                <button
                  onClick={() =>
                    downloadFile(
                      orderStatus.certificate_pem!,
                      `${orderStatus.domain}-cert.pem`,
                    )
                  }
                  className="rounded-lg bg-[#222] px-3 py-2 text-xs font-semibold text-[#e0e0e0] transition-colors hover:bg-[#333]"
                >
                  ⬇ Certificate (.pem)
                </button>
                {effectivePrivateKeyPem && (
                  <button
                    onClick={() =>
                      downloadFile(
                        effectivePrivateKeyPem,
                        `${orderStatus.domain}-key.pem`,
                      )
                    }
                    className="rounded-lg bg-[#222] px-3 py-2 text-xs font-semibold text-[#e0e0e0] transition-colors hover:bg-[#333]"
                  >
                    ⬇ Private Key (.pem)
                  </button>
                )}
                {orderStatus.certificate_pem && effectivePrivateKeyPem && (
                  <button
                    onClick={() =>
                      downloadFile(
                        orderStatus.certificate_pem +
                          "\n" +
                          effectivePrivateKeyPem,
                        `${orderStatus.domain}-combined.pem`,
                      )
                    }
                    className="rounded-lg bg-[#222] px-3 py-2 text-xs font-semibold text-[#e0e0e0] transition-colors hover:bg-[#333]"
                  >
                    ⬇ Combined (.pem)
                  </button>
                )}
              </div>
            )}

            {/* Certificate PEM */}
            {orderStatus.certificate_pem && (
              <div className="rounded-lg border border-[#222] bg-[#0a0a0a] p-3">
                <div className="mb-2 flex items-center justify-between">
                  <div className="text-[0.7rem] uppercase tracking-wider text-[#666]">
                    Certificate PEM
                  </div>
                  <div className="flex gap-1">
                    <CopyButton
                      text={orderStatus.certificate_pem}
                      label="Copy"
                    />
                  </div>
                </div>
                <pre className="max-h-[120px] overflow-y-auto break-all font-mono text-xs text-[#bbb]">
                  {orderStatus.certificate_pem.slice(0, 300)}
                  ...
                </pre>
              </div>
            )}

            {/* Private Key */}
            {effectivePrivateKeyPem && (
              <div className="rounded-lg border border-[rgba(231,76,60,0.15)] bg-[#0a0a0a] p-3">
                <div className="mb-2 flex items-center justify-between">
                  <div className="text-[0.7rem] uppercase tracking-wider text-[#666]">
                    Private Key
                  </div>
                  <div className="flex gap-1">
                    <CopyButton text={effectivePrivateKeyPem} label="Copy" />
                  </div>
                </div>
                {showKey ? (
                  <pre className="max-h-[120px] overflow-y-auto break-all font-mono text-xs text-[#bbb]">
                    {effectivePrivateKeyPem.slice(0, 300)}
                    ...
                  </pre>
                ) : (
                  <button
                    onClick={() => setShowKey(true)}
                    className="text-xs text-[#ff6b35] hover:underline"
                  >
                    Click to reveal private key
                  </button>
                )}
              </div>
            )}

            {/* Security note */}
            <div className="rounded-lg border border-[rgba(231,76,60,0.15)] bg-[rgba(231,76,60,0.05)] px-4 py-3 text-xs text-[#e8a49c]">
              {usedClientDerivation.current ? (
                <>
                  🔒 Your private key was derived from your nsec and{" "}
                  <strong>never left your browser</strong>. The certificate
                  authority never saw it. The NoDNS bot never saw it.
                </>
              ) : (
                <>
                  Install these on your web server. The private key is secret —
                  never share it publicly.
                </>
              )}
            </div>

            {/* Try Again */}
            <button
              onClick={handleReset}
              className="rounded-lg bg-[#222] px-3 py-2 text-xs font-semibold text-[#e0e0e0] hover:bg-[#333]"
            >
              Request New Certificate
            </button>
          </div>
        )}

        {/* Failed */}
        {orderStatus && status === "failed" && (
          <div className="space-y-3">
            <div className="rounded-lg border border-[rgba(231,76,60,0.25)] bg-[rgba(231,76,60,0.08)] px-4 py-3 text-sm text-[#e74c3c]">
              ❌ Certificate failed: {orderStatus.error || "Unknown error"}
            </div>
            {logs.length > 0 && (
              <AcmeLogDisplay logs={logs} isComplete={true} />
            )}
            <button
              onClick={handleReset}
              className="rounded-lg bg-[#222] px-3 py-2 text-xs font-semibold text-[#e0e0e0] hover:bg-[#333]"
            >
              Try Again
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
