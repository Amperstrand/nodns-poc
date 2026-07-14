import { useState, useEffect, useCallback, useMemo, useRef } from "react";
import { useIdentity } from "@/contexts/IdentityContext";
import { useWallet } from "@/contexts/WalletContext";
import { fetchRecords, fetchPricing } from "@/lib/api";
import {
  buildRecordTag,
  buildCashuTag,
  signAndPublish,
  subscribeToRecords,
} from "@/lib/nostr";
import { validateRecord } from "@nodns/resolver";
import { calculatePrice, formatSats } from "@/lib/pricing";
import { DNS_TYPES, DEFAULT_MINT_URL, DEFAULT_ZONE } from "@/lib/constants";
import type { DnsRecord, PricingInfo } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  CardDescription,
} from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";

const VALUE_PLACEHOLDERS: Record<string, string> = {
  A: "203.0.113.1",
  AAAA: "2001:db8::1",
  CNAME: "example.com.",
  TXT: "v=spf1 include:_spf.example.com ~all",
  MX: "10 mail.example.com.",
};

const TTL_OPTIONS = [
  { label: "Auto", value: 3600 },
  { label: "5 min", value: 300 },
  { label: "30 min", value: 1800 },
  { label: "1 hour", value: 3600 },
  { label: "12 hours", value: 43200 },
  { label: "24 hours", value: 86400 },
];

const TYPE_BADGE_CLASSES: Record<string, string> = {
  A: "bg-blue-500/15 text-blue-400 border-blue-500/30",
  AAAA: "bg-purple-500/15 text-purple-400 border-purple-500/30",
  CNAME: "bg-cyan-500/15 text-cyan-400 border-cyan-500/30",
  TXT: "bg-orange-500/15 text-orange-400 border-orange-500/30",
  MX: "bg-green-500/15 text-green-400 border-green-500/30",
};

const compactSelectClass =
  "flex h-8 w-full rounded-md border border-input bg-background px-2 py-1 text-xs ring-offset-background focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring cursor-pointer";

function useHashSearchParams(): URLSearchParams {
  const [params, setParams] = useState<URLSearchParams>(() => {
    const hash = window.location.hash;
    const qIndex = hash.indexOf("?");
    return new URLSearchParams(qIndex >= 0 ? hash.slice(qIndex + 1) : "");
  });

  useEffect(() => {
    const update = () => {
      const hash = window.location.hash;
      const qIndex = hash.indexOf("?");
      setParams(new URLSearchParams(qIndex >= 0 ? hash.slice(qIndex + 1) : ""));
    };
    window.addEventListener("hashchange", update);
    return () => window.removeEventListener("hashchange", update);
  }, []);

  return params;
}

function TtlSelect({
  value,
  onChange,
  disabled,
}: {
  value: number;
  onChange: (v: number) => void;
  disabled?: boolean;
}) {
  const [open, setOpen] = useState(false);
  const current = TTL_OPTIONS.find((o) => o.value === value);
  const label = current ? `${current.label} (${current.value})` : `${value}s`;
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const handleClick = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", handleClick);
    document.addEventListener("keydown", handleKey);
    return () => {
      document.removeEventListener("mousedown", handleClick);
      document.removeEventListener("keydown", handleKey);
    };
  }, [open]);

  return (
    <div className="relative inline-block" ref={ref}>
      <button
        type="button"
        onClick={() => setOpen(!open)}
        disabled={disabled}
        className="flex h-8 w-full items-center justify-between gap-1 rounded-md border border-input bg-background px-2 text-xs text-foreground hover:bg-secondary/50 disabled:opacity-50 cursor-pointer whitespace-nowrap min-w-[120px]"
      >
        <span className="truncate">{label}</span>
        <svg
          width="10"
          height="10"
          viewBox="0 0 10 10"
          fill="none"
          className="shrink-0 opacity-60"
        >
          <path
            d="M2.5 3.75L5 6.25L7.5 3.75"
            stroke="currentColor"
            strokeWidth="1.5"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      </button>
      {open && (
        <div className="absolute right-0 top-full z-30 mt-1 min-w-[150px] rounded-md border border-border bg-card py-1 shadow-xl">
          {TTL_OPTIONS.map((opt) => (
            <button
              key={opt.value}
              type="button"
              onClick={() => {
                onChange(opt.value);
                setOpen(false);
              }}
              className={`flex w-full items-center justify-between px-3 py-1.5 text-xs hover:bg-secondary cursor-pointer ${
                opt.value === value
                  ? "text-primary font-medium"
                  : "text-foreground"
              }`}
            >
              <span>{opt.label}</span>
              <span className="text-muted-foreground ml-2">
                ({opt.value})
              </span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

export function Domain() {
  const searchParams = useHashSearchParams();
  const queryName = searchParams.get("name") || "";
  const zone = searchParams.get("zone") || DEFAULT_ZONE;

  const { session, npub, secretKey, loading: identityLoading } = useIdentity();
  const { balance, ready: walletReady, sendTokens } = useWallet();

  const isNpubDomain = !queryName || (!!npub && queryName === npub);
  const effectiveName = isNpubDomain ? "" : queryName;
  const fqdn = isNpubDomain
    ? npub
      ? `${npub}.${zone}`
      : `npub.${zone}`
    : `${queryName}.${zone}`;

  const [records, setRecords] = useState<DnsRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [pricing, setPricing] = useState<PricingInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const [formType, setFormType] = useState<string>("A");
  const [formName, setFormName] = useState<string>(effectiveName);
  const [formValue, setFormValue] = useState<string>("");
  const [formTtl, setFormTtl] = useState<number>(3600);

  const [editingId, setEditingId] = useState<string | null>(null);
  const [editValue, setEditValue] = useState("");
  const [editTtl, setEditTtl] = useState(3600);

  const [deleteTarget, setDeleteTarget] = useState<DnsRecord | null>(null);
  const [searchQuery, setSearchQuery] = useState("");

  useEffect(() => {
    setFormName(effectiveName);
  }, [effectiveName]);

  const loadRecords = useCallback(async () => {
    if (!npub) return;
    try {
      const allRecords = await fetchRecords(npub);
      const filtered = allRecords.filter((r) => {
        const rName = r.name || "";
        if (isNpubDomain) return rName === "" || rName === npub;
        return rName === queryName;
      });
      setRecords(filtered);
    } catch {
      setError("Failed to load records");
    } finally {
      setLoading(false);
    }
  }, [npub, queryName, isNpubDomain]);

  useEffect(() => {
    loadRecords();
    fetchPricing(zone)
      .then(setPricing)
      .catch(() => {});
  }, [loadRecords, zone]);

  useEffect(() => {
    if (!session?.pubkey) return;
    let t: ReturnType<typeof setTimeout>;
    const unsub = subscribeToRecords(session.pubkey, () => {
      clearTimeout(t);
      t = setTimeout(() => loadRecords(), 500);
    });
    return () => {
      clearTimeout(t);
      unsub();
    };
  }, [session?.pubkey, loadRecords]);

  useEffect(() => {
    if (!success) return;
    const t = setTimeout(() => setSuccess(null), 5000);
    return () => clearTimeout(t);
  }, [success]);

  const createCost = useMemo(() => {
    if (isNpubDomain) return 0;
    if (pricing) return pricing.create_price;
    return calculatePrice(queryName.length, false);
  }, [isNpubDomain, pricing, queryName]);

  const insufficientFunds =
    createCost > 0 && walletReady && balance < createCost;

  const filteredRecords = useMemo(() => {
    if (!searchQuery.trim()) return records;
    const q = searchQuery.toLowerCase();
    return records.filter(
      (r) =>
        r.record_type.toLowerCase().includes(q) ||
        (r.name || "").toLowerCase().includes(q) ||
        r.rdata.toLowerCase().includes(q),
    );
  }, [records, searchQuery]);

  async function handlePublish(tags: string[][], cost: number) {
    if (!session?.pubkey) throw new Error("Not authenticated");

    const allTags = [...tags];

    if (cost > 0) {
      if (balance < cost) {
        throw new Error(
          `Insufficient balance: need ${cost} sats, have ${balance} sats`,
        );
      }
      const token = await sendTokens(cost);
      allTags.push(buildCashuTag(token, DEFAULT_MINT_URL, cost));
    }

    await signAndPublish(secretKey, allTags);
  }

  async function handleAddRecord() {
    setError(null);
    setSuccess(null);

    const nameToUse = formName || effectiveName;
    const validationError = validateRecord(formType, nameToUse, formValue);
    if (validationError) {
      setError(validationError);
      return;
    }

    setBusy(true);
    try {
      const recordTag = buildRecordTag(formType, nameToUse, formValue, formTtl);
      await handlePublish([recordTag], createCost);
      setSuccess(`${formType} record published for ${fqdn}`);
      setFormValue("");
      await loadRecords();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to publish record");
    } finally {
      setBusy(false);
    }
  }

  function startEdit(record: DnsRecord) {
    const rid =
      record.id ||
      `${record.record_type}-${record.name}-${record.rdata}-${record.ttl}`;
    setEditingId(rid);
    setEditValue(record.rdata);
    setEditTtl(record.ttl);
  }

  async function handleSaveEdit(record: DnsRecord) {
    setError(null);
    const validationError = validateRecord(
      record.record_type,
      record.name,
      editValue,
    );
    if (validationError) {
      setError(validationError);
      return;
    }

    setBusy(true);
    try {
      const recordTag = buildRecordTag(
        record.record_type,
        record.name,
        editValue,
        editTtl,
      );
      const cost = pricing?.update_price ?? 0;
      await handlePublish([recordTag], cost);
      setSuccess("Record updated");
      setEditingId(null);
      await loadRecords();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to update record");
    } finally {
      setBusy(false);
    }
  }

  async function handleDeleteRecord(record: DnsRecord) {
    setError(null);
    setBusy(true);
    try {
      const recordTag = buildRecordTag(
        record.record_type,
        record.name,
        "",
        record.ttl,
      );
      const cost = pricing?.delete_price ?? 0;
      await handlePublish([recordTag], cost);
      setSuccess(`${record.record_type} record deleted`);
      setDeleteTarget(null);
      await loadRecords();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to delete record");
    } finally {
      setBusy(false);
    }
  }

  if (identityLoading) {
    return (
      <Card>
        <CardContent className="py-16 text-center text-muted-foreground">
          Loading identity...
        </CardContent>
      </Card>
    );
  }

  if (!session) {
    return (
      <Card>
        <CardContent className="py-16 text-center text-muted-foreground">
          Connect your Nostr identity to manage DNS records.
        </CardContent>
      </Card>
    );
  }

  return (
    <div className="space-y-6">
      <div className="space-y-2">
        <div className="flex items-center gap-3 flex-wrap">
          <h1 className="text-2xl md:text-3xl font-bold tracking-tight font-mono break-all">
            {fqdn}
          </h1>
          <Badge className="border-primary/40 text-primary">
            {isNpubDomain ? "npub-derived" : "custom"}
          </Badge>
          {createCost === 0 ? (
            <Badge className="border-green-500/40 text-green-400">free</Badge>
          ) : (
            <Badge className="border-primary/40 text-primary">
              {formatSats(createCost)} / record
            </Badge>
          )}
        </div>
        <p className="text-sm text-muted-foreground">
          Manage DNS records for this subdomain. Changes propagate globally
          within seconds.
        </p>
      </div>

      {error && (
        <Card className="border-destructive/40 bg-destructive/5">
          <CardContent className="py-3 text-sm text-destructive flex items-center justify-between gap-4">
            <span className="break-all">{error}</span>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setError(null)}
              className="shrink-0"
            >
              Dismiss
            </Button>
          </CardContent>
        </Card>
      )}

      {success && (
        <Card className="border-green-500/40 bg-green-500/5">
          <CardContent className="py-3 text-sm text-green-400 flex items-center justify-between gap-4">
            <span className="break-all">{success}</span>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setSuccess(null)}
              className="shrink-0"
            >
              Dismiss
            </Button>
          </CardContent>
        </Card>
      )}

      <Card>
        <CardHeader className="flex-row items-center justify-between space-y-0">
          <div className="space-y-1">
            <CardTitle>DNS Records</CardTitle>
            <CardDescription>
              {loading
                ? "Loading..."
                : `${filteredRecords.length}${searchQuery ? ` of ${records.length}` : ""} record${filteredRecords.length !== 1 ? "s" : ""} for ${fqdn}`}
            </CardDescription>
          </div>
          <div className="relative w-full max-w-[220px]">
            <svg
              className="absolute left-2.5 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground pointer-events-none"
              viewBox="0 0 16 16"
              fill="none"
            >
              <circle
                cx="7"
                cy="7"
                r="5"
                stroke="currentColor"
                strokeWidth="1.5"
              />
              <path
                d="M11 11L14 14"
                stroke="currentColor"
                strokeWidth="1.5"
                strokeLinecap="round"
              />
            </svg>
            <Input
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="Search records..."
              aria-label="Search records"
              className="h-8 pl-8 text-xs"
            />
          </div>
        </CardHeader>
        <CardContent>
          <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-border">
                  <th className="pb-2 pr-3 text-left text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                    Type
                  </th>
                  <th className="pb-2 pr-3 text-left text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                    Name
                  </th>
                  <th className="pb-2 pr-3 text-left text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                    Content
                  </th>
                  <th className="pb-2 pr-3 text-left text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                    TTL
                  </th>
                  <th className="pb-2 text-right text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                    Actions
                  </th>
                </tr>
              </thead>
              <tbody>
                {loading ? (
                  [0, 1, 2].map((n) => (
                    <tr key={n} className="border-b border-border/30">
                      <td className="py-3 pr-3"><div className="h-5 w-12 animate-pulse rounded bg-secondary" /></td>
                      <td className="py-3 pr-3"><div className="h-5 w-20 animate-pulse rounded bg-secondary" /></td>
                      <td className="py-3 pr-3"><div className="h-5 w-40 animate-pulse rounded bg-secondary" /></td>
                      <td className="py-3 pr-3"><div className="h-5 w-16 animate-pulse rounded bg-secondary" /></td>
                      <td className="py-3" />
                    </tr>
                  ))
                ) : filteredRecords.length === 0 ? (
                  <tr>
                    <td colSpan={5} className="py-16 text-center">
                      <div className="mx-auto mb-3 flex h-12 w-12 items-center justify-center rounded-full bg-secondary text-muted-foreground text-xl font-bold">+</div>
                      <p className="text-sm font-medium text-foreground">
                        {records.length === 0 ? "No DNS records yet" : "No matching records"}
                      </p>
                      <p className="mt-1 text-xs text-muted-foreground">
                        {records.length === 0 ? "Add your first record using the form below." : "Try a different search term."}
                      </p>
                    </td>
                  </tr>
                ) : (
                  filteredRecords.map((record, i) => {
                    const rid =
                      record.id ||
                      `${record.record_type}-${record.name}-${record.rdata}-${record.ttl}-${i}`;
                    const isEditing = editingId === rid;
                    return (
                      <tr
                        key={rid}
                        className="border-b border-border/50 last:border-0 group hover:bg-secondary/30 transition-colors"
                      >
                        <td className="py-2.5 pr-3">
                          <Badge
                            className={`font-mono border ${TYPE_BADGE_CLASSES[record.record_type] || "border-border"}`}
                          >
                            {record.record_type}
                          </Badge>
                        </td>
                        <td className="py-2.5 pr-3 font-mono text-xs text-muted-foreground whitespace-nowrap">
                          {record.name || "@ (apex)"}
                        </td>
                        <td className="py-2.5 pr-3 font-mono text-xs break-all max-w-md">
                          {isEditing ? (
                            <Input
                              value={editValue}
                              onChange={(e) => setEditValue(e.target.value)}
                              className="h-8 font-mono text-xs"
                            />
                          ) : (
                            record.rdata
                          )}
                        </td>
                        <td className="py-2.5 pr-3 whitespace-nowrap">
                          {isEditing ? (
                            <TtlSelect
                              value={editTtl}
                              onChange={setEditTtl}
                              disabled={busy}
                            />
                          ) : (
                            <span className="font-mono text-xs text-muted-foreground">
                              {TTL_OPTIONS.find((o) => o.value === record.ttl)?.label ?? `${record.ttl}s`}
                            </span>
                          )}
                        </td>
                        <td className="py-2.5 text-right whitespace-nowrap">
                          {isEditing ? (
                            <div className="flex gap-1 justify-end">
                              <Button
                                size="sm"
                                onClick={() => handleSaveEdit(record)}
                                disabled={busy}
                              >
                                Save
                              </Button>
                              <Button
                                size="sm"
                                variant="outline"
                                onClick={() => setEditingId(null)}
                                disabled={busy}
                              >
                                Cancel
                              </Button>
                            </div>
                          ) : (
                            <div className="flex gap-1 justify-end">
                              <Button
                                size="sm"
                                variant="ghost"
                                onClick={() => startEdit(record)}
                                disabled={busy}
                              >
                                Edit
                              </Button>
                              <Button
                                size="sm"
                                variant="ghost"
                                className="text-destructive hover:text-destructive"
                                onClick={() => setDeleteTarget(record)}
                                disabled={busy}
                              >
                                Delete
                              </Button>
                            </div>
                          )}
                        </td>
                      </tr>
                    );
                  })
                )}
                <tr className="border-t-2 border-border/60 bg-muted/30">
                  <td className="py-3 pr-3">
                    <select
                      value={formType}
                      onChange={(e) => setFormType(e.target.value)}
                      className={compactSelectClass}
                      aria-label="Record type"
                    >
                      {DNS_TYPES.map((t) => (
                        <option key={t} value={t}>
                          {t}
                        </option>
                      ))}
                    </select>
                  </td>
                  <td className="py-2.5 pr-3">
                    <Input
                      value={formName}
                      onChange={(e) => setFormName(e.target.value)}
                      placeholder={isNpubDomain ? "(apex)" : queryName}
                      className="h-8 font-mono text-xs min-w-[100px]"
                    />
                  </td>
                  <td className="py-2.5 pr-3">
                    <Input
                      value={formValue}
                      onChange={(e) => setFormValue(e.target.value)}
                      placeholder={VALUE_PLACEHOLDERS[formType] || ""}
                      title={
                        formType === "MX"
                          ? "Format: priority followed by mail host (e.g. 10 mail.example.com.)"
                          : undefined
                      }
                      className="h-8 font-mono text-xs"
                    />
                  </td>
                  <td className="py-2.5 pr-3">
                    <TtlSelect
                      value={formTtl}
                      onChange={setFormTtl}
                      disabled={busy}
                    />
                  </td>
                  <td className="py-2.5 text-right">
                    <Button
                      size="sm"
                      onClick={handleAddRecord}
                      disabled={busy || !formValue.trim() || insufficientFunds}
                    >
                      {busy
                        ? "Publishing..."
                        : createCost > 0
                          ? `Add Record (${formatSats(createCost)})`
                          : "Add Record"}
                    </Button>
                  </td>
                </tr>
              </tbody>
            </table>
          {insufficientFunds && (
            <div className="mt-3 flex items-center gap-3 text-xs">
              <span className="text-destructive">
                Insufficient balance: need {createCost} sats, have {balance}{" "}
                sats.
              </span>
              <a href="#/wallet">
                <Button variant="outline" size="sm">
                  Top up wallet
                </Button>
              </a>
            </div>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardContent className="py-4 flex items-center justify-between">
          <div>
            <p className="text-xs text-muted-foreground">Wallet Balance</p>
            <p className="text-lg font-semibold">{balance} sats</p>
          </div>
          {!walletReady && (
            <Badge className="text-muted-foreground">Initializing...</Badge>
          )}
        </CardContent>
      </Card>

      {deleteTarget && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm p-4"
          onClick={() => !busy && setDeleteTarget(null)}
        >
          <Card
            className="w-full max-w-md"
            onClick={(e) => e.stopPropagation()}
          >
            <CardHeader>
              <CardTitle>Delete Record</CardTitle>
              <CardDescription>
                This publishes a deletion event. The record will be removed from
                DNS globally within seconds.
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="rounded-md border border-border bg-secondary/50 p-3 space-y-2 text-sm">
                <div className="flex justify-between gap-4">
                  <span className="text-muted-foreground shrink-0">Type</span>
                  <Badge
                    className={`font-mono border ${TYPE_BADGE_CLASSES[deleteTarget.record_type] || "border-border"}`}
                  >
                    {deleteTarget.record_type}
                  </Badge>
                </div>
                <div className="flex justify-between gap-4">
                  <span className="text-muted-foreground shrink-0">Name</span>
                  <span className="font-mono text-right">
                    {deleteTarget.name || "@ (apex)"}
                  </span>
                </div>
                <div className="flex justify-between gap-4">
                  <span className="text-muted-foreground shrink-0">Value</span>
                  <span className="font-mono text-right break-all">
                    {deleteTarget.rdata}
                  </span>
                </div>
              </div>
              <div className="flex gap-2 justify-end">
                <Button
                  variant="outline"
                  onClick={() => setDeleteTarget(null)}
                  disabled={busy}
                >
                  Cancel
                </Button>
                <Button
                  variant="destructive"
                  onClick={() => handleDeleteRecord(deleteTarget)}
                  disabled={busy}
                >
                  {busy ? "Deleting..." : "Delete Record"}
                </Button>
              </div>
            </CardContent>
          </Card>
        </div>
      )}
    </div>
  );
}
