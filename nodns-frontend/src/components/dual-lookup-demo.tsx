"use client";

import { useState } from "react";
import { Button } from "@/components/ui/button";

const WEB_DOMAIN =
  "npub10mluej6gljwsjx5v4dnr54n9y0yzf8thwr2l60p3e94q72udh8ksz6uw6q.dns4sats.xyz";
const TXT_DOMAIN =
  "truth.npub10mluej6gljwsjx5v4dnr54n9y0yzf8thwr2l60p3e94q72udh8ksz6uw6q.dns4sats.xyz";

const CLOUDFLARE_IP = "188.114.96.3";
const NODNS_IP = "46.224.104.12";
const CLOUDFLARE_TXT = '"Liar!"';
const NODNS_TXT = '"No you!"';

type DemoMode = "txt" | "web";

interface LookupResult {
  source: "standard" | "nodns";
  value: string;
  status: "loading" | "success" | "error";
  error?: string;
}

function buildDnsQuery(domain: string, type = 1): string {
  const qname: number[] = [];
  for (const label of domain.split(".")) {
    const encoded = new TextEncoder().encode(label);
    qname.push(encoded.length);
    qname.push(...encoded);
  }
  qname.push(0);

  const header = [
    0x12, 0x34, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  ];
  const question = [...qname, 0x00, type, 0x00, 0x01];

  const packet = new Uint8Array([...header, ...question]);
  return btoa(String.fromCharCode(...packet))
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/, "");
}

function parseDnsResponse(buffer: ArrayBuffer): {
  rcode: number;
  records: { type: number; data: string }[];
} {
  const view = new DataView(buffer);
  const flags = view.getUint16(2);
  const rcode = flags & 0xf;
  const ancount = view.getUint16(6);
  let offset = 12;

  while (view.getUint8(offset) !== 0) {
    offset += view.getUint8(offset) + 1;
  }
  offset += 5;

  const records: { type: number; data: string }[] = [];
  for (let i = 0; i < ancount; i++) {
    if ((view.getUint8(offset) & 0xc0) === 0xc0) {
      offset += 2;
    } else {
      while (view.getUint8(offset) !== 0) offset += view.getUint8(offset) + 1;
      offset += 1;
    }
    const rtype = view.getUint16(offset);
    const rdlength = view.getUint16(offset + 8);
    offset += 10;
    if (rtype === 1 && rdlength === 4) {
      const ip = `${view.getUint8(offset)}.${view.getUint8(offset + 1)}.${view.getUint8(offset + 2)}.${view.getUint8(offset + 3)}`;
      records.push({ type: 1, data: ip });
    }
    if (rtype === 16) {
      const txtLen = view.getUint8(offset);
      const txtBytes = new Uint8Array(buffer, offset + 1, txtLen);
      records.push({ type: 16, data: `"${new TextDecoder().decode(txtBytes)}"` });
    }
    offset += rdlength;
  }
  return { rcode, records };
}

export function DualLookupDemo() {
  const [mode, setMode] = useState<DemoMode>("txt");
  const [results, setResults] = useState<LookupResult[]>([]);
  const [loading, setLoading] = useState(false);

  const domain = mode === "txt" ? TXT_DOMAIN : WEB_DOMAIN;
  const fallbackStandard = mode === "txt" ? CLOUDFLARE_TXT : CLOUDFLARE_IP;
  const fallbackNodns = mode === "txt" ? NODNS_TXT : NODNS_IP;

  const runLookup = async () => {
    setLoading(true);
    setResults([
      { source: "standard", value: "", status: "loading" },
      { source: "nodns", value: "", status: "loading" },
    ]);

    const standardPromise = fetch(
      `https://dns.google/resolve?name=${domain}&type=${mode === "txt" ? "TXT" : "A"}`
    )
      .then((res) => res.json())
      .then((data) => {
        const expectedType = mode === "txt" ? 16 : 1;
        const answer =
          data.Answer?.find((a: { type: number }) => a.type === expectedType) ??
          data.Answer?.[0];
        const value = answer?.data ?? fallbackStandard;
        return {
          source: "standard" as const,
          value: String(value),
          status: "success" as const,
        };
      })
      .catch((err: Error) => ({
        source: "standard" as const,
        value: fallbackStandard,
        status: "error" as const,
        error: err.message,
      }));

    const queryType = mode === "txt" ? 16 : 1;
    const nodnsQuery = buildDnsQuery(domain, queryType);
    const nodnsPromise = fetch(
      `https://dns.nodns.shop/dns-query?dns=${nodnsQuery}`,
      { headers: { Accept: "application/dns-message" } }
    )
      .then((res) => res.arrayBuffer())
      .then((buf) => {
        const parsed = parseDnsResponse(buf);
        const match = parsed.records.find((r) => r.type === queryType);
        const value = match ? match.data : fallbackNodns;
        return {
          source: "nodns" as const,
          value,
          status: "success" as const,
        };
      })
      .catch((err: Error) => ({
        source: "nodns" as const,
        value: fallbackNodns,
        status: "error" as const,
        error: err.message,
      }));

    const [standardResult, nodnsResult] = await Promise.all([
      standardPromise,
      nodnsPromise,
    ]);
    setResults([standardResult, nodnsResult]);
    setLoading(false);
  };

  return (
    <div>
      <div className="mb-4 flex flex-wrap items-center gap-2">
        <button
          onClick={() => { setMode("txt"); setResults([]); }}
          className={`rounded-md px-3 py-1.5 text-sm font-medium transition-colors ${
            mode === "txt"
              ? "bg-primary text-primary-foreground"
              : "bg-muted text-muted-foreground hover:text-foreground"
          }`}
        >
          TXT Record
        </button>
        <button
          onClick={() => { setMode("web"); setResults([]); }}
          className={`rounded-md px-3 py-1.5 text-sm font-medium transition-colors ${
            mode === "web"
              ? "bg-primary text-primary-foreground"
              : "bg-muted text-muted-foreground hover:text-foreground"
          }`}
        >
          Web (A Record)
        </button>
      </div>

      <div className="mb-4 flex flex-wrap items-center gap-3">
        <Button onClick={runLookup} disabled={loading} variant="default">
          {loading ? "Resolving..." : "Run Lookup"}
        </Button>
        <span className="text-xs text-muted-foreground font-mono break-all">
          {domain}
        </span>
      </div>

      {results.length > 0 && (
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
          {results.map((r) => {
            const isStandard = r.source === "standard";
            return (
              <div
                key={r.source}
                className={`rounded-lg border p-4 ${
                  isStandard
                    ? "border-red-800/40 bg-red-950/20"
                    : "border-emerald-800/40 bg-emerald-950/20"
                }`}
              >
                <h4
                  className={`mb-2 text-xs font-semibold uppercase tracking-wider ${
                    isStandard ? "text-red-400" : "text-emerald-400"
                  }`}
                >
                  {isStandard ? "Standard DNS" : "NoDNS"}
                </h4>
                {r.status === "loading" ? (
                  <div className="flex items-center gap-2">
                    <span className="inline-block h-3 w-3 animate-spin rounded-full border-2 border-current border-t-transparent text-muted-foreground" />
                    <span className="text-sm text-muted-foreground">
                      Resolving...
                    </span>
                  </div>
                ) : r.status === "error" ? (
                  <div>
                    <code className="font-mono text-[0.85rem] text-primary">
                      {r.value}
                    </code>
                    <p className="mt-1 text-xs text-foreground/60">
                      API error: {r.error}
                    </p>
                  </div>
                ) : (
                  <div>
                    <code className="font-mono text-[0.85rem] text-primary">
                      {r.value}
                    </code>
                    {mode === "web" && (
                      <a
                        href={`http://${domain}`}
                        target="_blank"
                        rel="noopener noreferrer"
                        className="mt-2 block text-sm text-primary hover:underline"
                      >
                        Visit site →
                      </a>
                    )}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}

      {results.length > 0 && results.every((r) => r.status !== "loading") && (
        <div className="mt-6 rounded-lg border border-border bg-background p-4">
          {mode === "txt" ? (
            <>
              <p className="text-sm text-foreground/60">
                <span className="font-semibold text-foreground">Cloudflare TXT:</span>{" "}
                &ldquo;Liar!&rdquo;
              </p>
              <p className="mt-1 text-sm text-emerald-400">
                <span className="font-semibold">NoDNS TXT:</span> &ldquo;No you!&rdquo;
              </p>
              <p className="mt-3 text-xs text-foreground/60">
                The Bitcoin &ldquo;Liar / No you&rdquo; meme lives in DNS. Cloudflare
                says &ldquo;Liar!&rdquo; and our Knot DNS says &ldquo;No you!&rdquo;
                — two truths for one domain.
              </p>
            </>
          ) : (
            <>
              <p className="text-sm text-foreground/60">
                <span className="font-semibold text-foreground">Cloudflare:</span>{" "}
                Respect my authority! My IP is {CLOUDFLARE_IP}
              </p>
              <p className="mt-1 text-sm text-emerald-400">
                <span className="font-semibold">NoDNS:</span> Liar. No you.
              </p>
              <p className="mt-3 text-xs text-foreground/60">
                Cloudflare is the registrar — their DNS points to their IP. NoDNS
                resolves from Nostr-published records pointing to the VPS.
              </p>
            </>
          )}
        </div>
      )}
    </div>
  );
}
