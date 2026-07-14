import { useState, useCallback, type FormEvent } from "react";
import { useIdentity } from "@/contexts/IdentityContext";
import { useWallet } from "@/contexts/WalletContext";
import { useZones } from "@/contexts/ZoneContext";
import { checkAvailability } from "@/lib/api";
import {
  buildRecordTag,
  buildCashuTag,
  publishAndBroadcast,
} from "@/lib/nostr";
import { formatSats } from "@/lib/pricing";
import { validateDomainName } from "@nodns/resolver";
import { DEFAULT_ZONE, DEFAULT_MINT_URL } from "@/lib/constants";
import type { AvailabilityResult } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  CardDescription,
} from "@/components/ui/card";
import { LoginModal } from "@/components/login-modal";

const FEATURES = [
  {
    title: "Nostr-native identity",
    description:
      "Your Nostr key is your identity. No accounts, no passwords, no email verification. Sign a single event and your name is live.",
  },
  {
    title: "Cashu payments",
    description:
      "Anti-spam pricing via Cashu ecash tokens. Short names cost more, long names cost less. npub-derived names are always free.",
  },
  {
    title: "DNSSEC-signed",
    description:
      "Every record is signed with ECDSAP256SHA256 and resolvable globally via standard DNS. No browser extension or special resolver required.",
  },
];

const STEPS = [
  {
    title: "Publish event",
    description:
      "Sign a kind 11111 Nostr event with your record data. Add a Cashu token if the name requires payment.",
  },
  {
    title: "Bot validates",
    description:
      "The nodns-bot verifies your signature, checks authority, validates the Cashu payment, and enforces policy rules.",
  },
  {
    title: "DNS live",
    description:
      "A TSIG-signed DDNS update hits Knot DNS. Your record is DNSSEC-signed and globally resolvable within seconds.",
  },
];

export function Landing() {
  const { session, secretKey } = useIdentity();
  const { balance, sendTokens } = useWallet();
  const { zones, selectedZone, selectZone, loading: zonesLoading, error: zonesError } = useZones();

  const activeZone = selectedZone?.zone ?? DEFAULT_ZONE;
  const showZoneWarning =
    selectedZone?.status === "testing" || selectedZone?.testnet === true;

  const [query, setQuery] = useState("");
  const [searching, setSearching] = useState(false);
  const [result, setResult] = useState<AvailabilityResult | null>(null);
  const [searchError, setSearchError] = useState<string | null>(null);
  const [loginOpen, setLoginOpen] = useState(false);
  const [registering, setRegistering] = useState(false);
  const [registerError, setRegisterError] = useState<string | null>(null);
  const [registeredFqdn, setRegisteredFqdn] = useState<string | null>(null);

  const handleSearch = useCallback(
    async (e?: FormEvent) => {
      e?.preventDefault();
      const zoneEscaped = activeZone.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
      const name = query
        .trim()
        .toLowerCase()
        .replace(new RegExp(`\\.${zoneEscaped}$`, "i"), "");
      if (!name) return;

      const vErr = validateDomainName(name);
      if (vErr) {
        setSearchError(vErr);
        setResult(null);
        return;
      }

      setSearching(true);
      setSearchError(null);
      setResult(null);
      setRegisteredFqdn(null);
      setRegisterError(null);

      try {
        const res = await checkAvailability(name, activeZone);
        setResult(res);
      } catch (err) {
        setSearchError(
          err instanceof Error ? err.message : "Search failed",
        );
      } finally {
        setSearching(false);
      }
    },
    [query, activeZone],
  );

  const handleRegister = useCallback(async () => {
    if (!session || !result) return;

    setRegistering(true);
    setRegisterError(null);

    try {
      const tags: string[][] = [
        buildRecordTag("TXT", result.name, "registered", 3600),
      ];

      if (result.price > 0) {
        const token = await sendTokens(result.price);
        tags.push(buildCashuTag(token, DEFAULT_MINT_URL, result.price));
      }

      await publishAndBroadcast(secretKey, tags);
      setRegisteredFqdn(result.fqdn);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      if (msg.includes("proofs") || msg.includes("Not enough") || msg.includes("insufficient")) {
        setRegisterError(`Insufficient balance. You need ${result.price} sats — top up your wallet first.`);
      } else if (msg.includes("mint") || msg.includes("swap")) {
        setRegisterError("Wallet error. Try again or top up your wallet.");
      } else {
        setRegisterError(msg);
      }
    } finally {
      setRegistering(false);
    }
  }, [session, secretKey, result, sendTokens]);

  const resetSearch = useCallback(() => {
    setRegisteredFqdn(null);
    setResult(null);
    setQuery("");
    setSearchError(null);
    setRegisterError(null);
  }, []);

  return (
    <div className="space-y-28 pb-20">
      <section className="relative">
        <div
          className="pointer-events-none absolute inset-x-0 -top-8 -z-10 h-[420px]"
          style={{
            background:
              "radial-gradient(ellipse 55% 50% at 50% 0%, rgba(255,107,53,0.10), transparent 70%)",
          }}
        />
        <div className="mx-auto max-w-3xl pt-10 text-center">
          <div className="mb-6 inline-flex items-center gap-2 rounded-full border border-border bg-card/80 px-3 py-1 backdrop-blur">
            <span className="h-1.5 w-1.5 rounded-full bg-primary animate-live-pulse" />
            <span className="text-xs font-medium tracking-wide text-muted-foreground">
              Decentralized DNS
            </span>
          </div>

          <h1 className="text-4xl font-bold tracking-tight sm:text-5xl md:text-6xl">
            Find your{" "}
            <span className="text-primary">.{activeZone}</span>{" "}
            name
          </h1>

          <p className="mx-auto mt-5 max-w-lg text-base text-muted-foreground sm:text-lg">
            No registrar. No account. No email verification. Just a Nostr
            signature and your name is live globally.
          </p>

          {zonesLoading && (
            <div className="mx-auto mt-6 max-w-2xl text-center text-sm text-muted-foreground">
              Discovering available zones…
            </div>
          )}

          {zonesError && (
            <div className="mx-auto mt-6 max-w-2xl rounded-md border border-amber-500/40 bg-amber-500/10 px-4 py-3 text-left text-sm text-amber-400">
              Zone discovery failed: {zonesError}. Using default zone{" "}
              <span className="font-mono">.{DEFAULT_ZONE}</span>.
            </div>
          )}

          {!zonesLoading && !zonesError && zones.length === 0 && (
            <div className="mx-auto mt-6 max-w-2xl rounded-md border border-amber-500/40 bg-amber-500/10 px-4 py-3 text-left text-sm text-amber-400">
              No zones discovered via Nostr. Using default zone{" "}
              <span className="font-mono">.{DEFAULT_ZONE}</span>.
            </div>
          )}

          {zones.length >= 2 && (
            <div className="mx-auto mt-6 max-w-2xl">
              <label
                htmlFor="zone-selector"
                className="mb-1.5 block text-left text-xs font-medium text-muted-foreground"
              >
                Zone
              </label>
              <select
                id="zone-selector"
                value={selectedZone?.zone ?? ""}
                onChange={(e) => selectZone(e.target.value)}
                className="w-full rounded-lg border border-border bg-card px-3 py-2.5 text-sm text-foreground focus:border-primary/50 focus:outline-none focus:ring-2 focus:ring-ring"
              >
                {zones.map((z) => (
                  <option key={z.zone} value={z.zone}>
                    {z.zone}
                    {z.verified ? " ✓ verified" : " ⚠ unverified"}
                    {z.testnet ? " (testnet)" : ""}
                    {z.status === "testing" ? " (testing)" : ""}
                  </option>
                ))}
              </select>
            </div>
          )}

          {showZoneWarning && (
            <div className="mx-auto mt-4 max-w-2xl rounded-md border border-amber-500/40 bg-amber-500/10 px-4 py-3 text-left text-sm text-amber-400">
              <p className="font-semibold">
                ⚠ TESTING MODE — This is a best-effort pilot. Records may be
                temporary. Not yet fully implemented.
              </p>
              {selectedZone?.statusReason && (
                <p className="mt-1 text-xs text-amber-400/80">
                  {selectedZone.statusReason}
                </p>
              )}
            </div>
          )}

          <form onSubmit={handleSearch} className="mx-auto mt-10 max-w-2xl">
            <div className="flex flex-col gap-2 sm:flex-row sm:items-stretch">
              <div className="flex flex-1 items-center overflow-hidden rounded-lg border border-border bg-card transition-colors focus-within:border-primary/50 focus-within:ring-2 focus-within:ring-ring">
                <Input
                  type="text"
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  placeholder="alice"
                  className="h-14 border-0 bg-transparent px-4 text-base shadow-none focus-visible:ring-0"
                  autoComplete="off"
                  autoCapitalize="off"
                  spellCheck={false}
                />
                <span className="whitespace-nowrap border-l border-border px-4 py-2 text-sm text-muted-foreground">
                  .{activeZone}
                </span>
              </div>
              <Button
                type="submit"
                size="lg"
                disabled={searching || !query.trim()}
                className="h-14 animate-cta-glow px-8 text-base sm:px-10"
              >
                {searching ? "Searching..." : "Search"}
              </Button>
            </div>
          </form>

          {searchError && (
            <div className="mx-auto mt-4 max-w-2xl rounded-md border border-destructive/50 bg-destructive/10 px-4 py-3 text-left text-sm text-destructive">
              {searchError}
            </div>
          )}

          {result && !registeredFqdn && (
            <Card className="mx-auto mt-6 max-w-2xl text-left">
              <CardContent className="flex flex-col gap-4 p-5 sm:flex-row sm:items-center sm:justify-between">
                <div className="flex items-center gap-3">
                  <div
                    className={`flex h-11 w-11 shrink-0 items-center justify-center rounded-full text-lg font-bold ${
                      result.available
                        ? "bg-green-500/10 text-green-400"
                        : "bg-destructive/10 text-destructive"
                    }`}
                  >
                    {result.available ? "+" : "!"}
                  </div>
                  <div>
                    <p className="font-mono text-sm font-medium">
                      {result.fqdn}
                    </p>
                    <p
                      className={`text-sm font-medium ${
                        result.available
                          ? "text-green-400"
                          : "text-destructive"
                      }`}
                    >
                      {result.available ? "Available!" : "Taken"}
                    </p>
                  </div>
                </div>

                <div className="flex shrink-0 items-center gap-3">
                  {result.available && (
                    <span className="hidden text-sm text-muted-foreground sm:block">
                      {formatSats(result.price)}
                    </span>
                  )}
                  {result.available ? (
                    session ? (
                      <>
                        <Button
                          onClick={handleRegister}
                          disabled={registering || (result.price > 0 && balance < result.price)}
                        >
                          {registering
                            ? "Registering..."
                            : result.price > 0
                              ? `Register for ${result.price} sats`
                              : "Register free"}
                        </Button>
                        {result.price > 0 && balance < result.price && (
                          <a href="#/wallet">
                            <Button variant="outline" size="sm">
                              Top up wallet
                            </Button>
                          </a>
                        )}
                      </>
                    ) : (
                      <Button onClick={() => setLoginOpen(true)}>
                        Sign in to register
                      </Button>
                    )
                  ) : (
                    <span className="text-sm text-muted-foreground">
                      Try another name
                    </span>
                  )}
                </div>
              </CardContent>

              {session &&
                result.available &&
                result.price > 0 && (
                  <div className="border-t border-border px-5 py-2.5 text-xs text-muted-foreground">
                    Wallet balance: {balance} sats
                    {balance < result.price && (
                      <span className="ml-2 text-destructive">
                        Insufficient funds
                      </span>
                    )}
                  </div>
                )}

              {registerError && (
                <div className="border-t border-destructive/30 bg-destructive/5 px-5 py-3 text-sm text-destructive">
                  {registerError}
                </div>
              )}
            </Card>
          )}

          {registeredFqdn && (
            <Card className="mx-auto mt-6 max-w-2xl border-green-500/30 text-left">
              <CardContent className="p-6 space-y-4">
                <div className="flex items-start gap-4">
                  <div className="flex h-11 w-11 shrink-0 items-center justify-center rounded-full bg-green-500/10 text-lg font-bold text-green-400">
                    {"\u2713"}
                  </div>
                  <div className="flex-1 space-y-1">
                    <p className="font-medium text-green-400">
                      Registration broadcast!
                    </p>
                    <p className="font-mono text-sm break-all">
                      {registeredFqdn}
                    </p>
                    <p className="text-xs text-muted-foreground">
                      Your Nostr event is published. The bot will process it and your record will be live globally within seconds.
                    </p>
                  </div>
                </div>
                <div className="flex flex-wrap gap-2 pt-2 border-t border-border">
                  <a href="#/dashboard">
                    <Button size="sm">
                      View Dashboard
                    </Button>
                  </a>
                  <a
                    href={`https://dns.google/query?name=${encodeURIComponent(registeredFqdn)}&type=TXT`}
                    target="_blank"
                    rel="noopener noreferrer"
                  >
                    <Button variant="outline" size="sm">
                      Verify in DNS
                    </Button>
                  </a>
                  <Button variant="ghost" size="sm" onClick={resetSearch}>
                    Search again
                  </Button>
                </div>
              </CardContent>
            </Card>
          )}
        </div>
      </section>

      <section>
        <div className="mb-10 text-center">
          <h2 className="text-2xl font-bold tracking-tight sm:text-3xl">
            Why NoDNS
          </h2>
          <p className="mx-auto mt-2 max-w-md text-sm text-muted-foreground">
            A fundamentally different approach to DNS. No centralized
            authority, no custody, no rent-seeking.
          </p>
        </div>
        <div className="grid gap-4 sm:grid-cols-3">
          {FEATURES.map((f) => (
            <Card
              key={f.title}
              className="transition-colors hover:border-primary/40"
            >
              <CardHeader>
                <CardTitle className="text-base">{f.title}</CardTitle>
              </CardHeader>
              <CardContent>
                <CardDescription className="leading-relaxed">
                  {f.description}
                </CardDescription>
              </CardContent>
            </Card>
          ))}
        </div>
      </section>

      <section>
        <div className="mb-10 text-center">
          <h2 className="text-2xl font-bold tracking-tight sm:text-3xl">
            How it works
          </h2>
          <p className="mx-auto mt-2 max-w-md text-sm text-muted-foreground">
            From keystroke to globally resolvable DNS record in under five
            seconds.
          </p>
        </div>
        <div className="grid gap-4 sm:grid-cols-3">
          {STEPS.map((step, i) => (
            <Card key={step.title} className="relative h-full">
              <CardContent className="p-6">
                <div className="mb-4 flex h-10 w-10 items-center justify-center rounded-full bg-primary text-sm font-bold text-primary-foreground">
                  {i + 1}
                </div>
                <h3 className="mb-2 font-semibold">{step.title}</h3>
                <p className="text-sm leading-relaxed text-muted-foreground">
                  {step.description}
                </p>
              </CardContent>
            </Card>
          ))}
        </div>
      </section>

      <LoginModal open={loginOpen} onClose={() => setLoginOpen(false)} />
    </div>
  );
}
