"use client";

import {
  Suspense,
  useCallback,
  useEffect,
  useMemo,
  useState,
} from "react";
import { useSearchParams } from "next/navigation";
import Link from "next/link";
import { SiteHeader } from "@/components/site-header";
import { SiteFooter } from "@/components/site-footer";
import { ErrorBoundary } from "@/components/error-boundary";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
  DialogClose,
} from "@/components/ui/dialog";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useIdentity } from "@/contexts/IdentityContext";
import { useWallet } from "@/contexts/WalletContext";
import { MINT_URL } from "@/lib/wallet";
import { toFqdn } from "@/lib/pricing";
import {
  fetchTripartiteRecords,
  fetchPricing,
  compareTripartite,
  type TripartiteRecords,
} from "@/lib/sources";
import {
  publishDnsEvent,
  publishDeleteEvent,
  keyPairFromNsec,
  subscribeToDnsEvents,
} from "@/lib/nostr";
import { validateRecord } from "@/lib/validation";
import { getEncodedToken } from "coco-cashu-core";
import type { ZonePricing, KeyPair } from "@/lib/types";
import { SourceIndicator } from "@/components/source-indicator";
import {
  ArrowLeftIcon,
  PlusIcon,
  PencilIcon,
  TrashIcon,
  CheckIcon,
  XIcon,
  LoaderIcon,
  GlobeIcon,
  RefreshCwIcon,
  ShieldIcon,
} from "lucide-react";

type RecordStatus = "idle" | "saving" | "deleting";

interface DnsRecordRow {
  id: string;
  type: string;
  name: string;
  value: string;
  ttl: number;
  created_at: number;
  isNew?: boolean;
  sources: string[];
}

const RECORD_TYPES = ["A", "AAAA", "CNAME", "TXT", "MX", "NS"] as const;

function makeRecordId(r: {
  type: string;
  name: string;
  value: string;
}): string {
  return `${r.type}:${r.name}:${r.value}`;
}

function statusDot(status: string) {
  if (status === "ok") return "🟢";
  if (status === "error") return "🔴";
  if (status === "loading") return "🟡";
  return "⚫";
}

function DomainDetailContent() {
  const searchParams = useSearchParams();
  const nameParam = searchParams.get("name") || "";
  const fqdn = nameParam ? toFqdn(nameParam) : "";

  const { initialized, nsec } = useIdentity();
  const { manager, balance, status: walletStatus } = useWallet();

  const [records, setRecords] = useState<DnsRecordRow[]>([]);
  const [pageStatus, setPageStatus] = useState<"loading" | "ready" | "error">(
    "loading"
  );
  const [errorMsg, setErrorMsg] = useState("");
  const [pricing, setPricing] = useState<ZonePricing | null>(null);
  const [tripartite, setTripartite] = useState<TripartiteRecords | null>(null);

  // Editing state
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editValue, setEditValue] = useState("");
  const [editTtl, setEditTtl] = useState(3600);
  const [validationError, setValidationError] = useState<string | null>(null);

  // Add record state
  const [addingRecord, setAddingRecord] = useState(false);
  const [newType, setNewType] = useState<string>("A");
  const [newName, setNewName] = useState("@");
  const [newValue, setNewValue] = useState("");
  const [newTtl, setNewTtl] = useState(3600);
  const [addError, setAddError] = useState<string | null>(null);

  // Operation state
  const [opStatus, setOpStatus] = useState<RecordStatus>("idle");
  const [opError, setOpError] = useState<string | null>(null);

  // Delete confirmation
  const [deleteTarget, setDeleteTarget] = useState<DnsRecordRow | null>(null);

  const loadRecords = useCallback(async () => {
    if (!initialized || !nameParam) return;

    try {
      const domain = toFqdn(nameParam);
      const results = await fetchTripartiteRecords({ domain });
      setTripartite(results);

      const recordMap = new Map<string, DnsRecordRow>();

      for (const r of results.api.records) {
        const id = makeRecordId({ type: r.type, name: r.name || "@", value: r.rdata });
        const existing = recordMap.get(id);
        if (existing) {
          if (!existing.sources.includes("api")) existing.sources.push("api");
          existing.created_at = Math.max(existing.created_at, r.created_at);
        } else {
          recordMap.set(id, {
            id,
            type: r.type,
            name: r.name || "@",
            value: r.rdata,
            ttl: r.ttl,
            created_at: r.created_at,
            sources: ["api"],
          });
        }
      }

      for (const r of results.nostr.records) {
        const id = makeRecordId({ type: r.type, name: r.name, value: r.value });
        const existing = recordMap.get(id);
        if (existing) {
          if (!existing.sources.includes("nostr")) existing.sources.push("nostr");
          existing.created_at = Math.max(existing.created_at, r.created_at);
        } else {
          recordMap.set(id, {
            id,
            type: r.type,
            name: r.name,
            value: r.value,
            ttl: r.ttl,
            created_at: r.created_at,
            sources: ["nostr"],
          });
        }
      }

      for (const r of results.dns.records) {
        const id = makeRecordId({ type: r.type, name: r.name.split(".")[0] || "@", value: r.data });
        const existing = recordMap.get(id);
        if (existing) {
          if (!existing.sources.includes("dns")) existing.sources.push("dns");
        } else {
          recordMap.set(id, {
            id,
            type: r.type,
            name: r.name.split(".")[0] || "@",
            value: r.data,
            ttl: r.ttl,
            created_at: 0,
            sources: ["dns"],
          });
        }
      }

      setRecords(
        Array.from(recordMap.values()).sort((a, b) => b.created_at - a.created_at)
      );
    } catch (err) {
      setErrorMsg(
        err instanceof Error ? err.message : "Failed to load records"
      );
    } finally {
      setPageStatus("ready");
    }
  }, [initialized, nameParam]);

  // Fetch pricing
  useEffect(() => {
    fetchPricing()
      .then(setPricing)
      .catch(() => {});
  }, []);

  useEffect(() => {
    if (!initialized) return;

    const id = requestAnimationFrame(() => loadRecords());

    const unsub = subscribeToDnsEvents(() => {
      loadRecords();
    });

    return () => {
      cancelAnimationFrame(id);
      unsub();
    };
  }, [initialized, loadRecords]);

  // --- Computed ---
  const getStatusBadge = () => {
    if (records.length === 0)
      return (
        <Badge className="border border-yellow-800 bg-yellow-950/60 text-yellow-400">
          No Records
        </Badge>
      );
    return (
      <Badge className="border border-emerald-800 bg-emerald-950/60 text-emerald-400">
        Active
      </Badge>
    );
  };

  const lastSeen = useMemo(
    () => records.reduce((max, r) => Math.max(max, r.created_at), 0),
    [records]
  );

  const getExpiryDate = (): string => {
    if (lastSeen === 0) return "—";
    const expiry = new Date((lastSeen + 365 * 86400) * 1000);
    return expiry.toLocaleDateString("en-US", {
      year: "numeric",
      month: "short",
      day: "numeric",
    });
  };

  // --- Handlers ---

  const handleSaveNew = useCallback(async () => {
    if (!manager || !nsec || !nameParam) return;

    setAddError(null);

    // Validate
    const err = validateRecord(newType, newName === "@" ? "" : newName, newValue);
    if (err) {
      setAddError(err);
      return;
    }

    setOpStatus("saving");
    setOpError(null);

    try {
      const keyPair: KeyPair = keyPairFromNsec(nsec);
      const price = pricing?.create_price ?? 0;

      let cashuToken: string | undefined;
      if (price > 0 && balance >= price) {
        const tokenObj = await manager.wallet.send(MINT_URL, price);
        cashuToken = getEncodedToken(tokenObj);
      }

      await publishDnsEvent(
        [
          {
            type: newType,
            name: newName === "@" ? "" : newName,
            value: newValue,
            ttl: newTtl,
          },
        ],
        keyPair.secretKey,
        cashuToken,
        MINT_URL,
        price
      );

      // Reset add form
      setAddingRecord(false);
      setNewType("A");
      setNewName("@");
      setNewValue("");
      setNewTtl(3600);

      // Refresh
      await loadRecords();
    } catch (err) {
      setOpError(
        err instanceof Error ? err.message : "Failed to save record"
      );
    } finally {
      setOpStatus("idle");
    }
  }, [
    manager,
    nsec,
    nameParam,
    newType,
    newName,
    newValue,
    newTtl,
    pricing,
    balance,
    loadRecords,
  ]);

  const handleEditSave = useCallback(async () => {
    if (!manager || !nsec || !nameParam || !editingId) return;

    const record = records.find((r) => r.id === editingId);
    if (!record) return;

    // Validate
    const err = validateRecord(
      record.type,
      record.name === "@" ? "" : record.name,
      editValue
    );
    if (err) {
      setValidationError(err);
      return;
    }

    setOpStatus("saving");
    setOpError(null);

    try {
      const keyPair: KeyPair = keyPairFromNsec(nsec);
      const price = pricing?.update_price ?? 0;

      let cashuToken: string | undefined;
      if (price > 0 && balance >= price) {
        const tokenObj = await manager.wallet.send(MINT_URL, price);
        cashuToken = getEncodedToken(tokenObj);
      }

      await publishDnsEvent(
        [
          {
            type: record.type,
            name: record.name === "@" ? "" : record.name,
            value: editValue,
            ttl: editTtl,
          },
        ],
        keyPair.secretKey,
        cashuToken,
        MINT_URL,
        price
      );

      setEditingId(null);
      await loadRecords();
    } catch (err) {
      setOpError(
        err instanceof Error ? err.message : "Failed to update record"
      );
    } finally {
      setOpStatus("idle");
    }
  }, [
    manager,
    nsec,
    nameParam,
    editingId,
    records,
    editValue,
    editTtl,
    pricing,
    balance,
    loadRecords,
  ]);

  const handleDelete = useCallback(
    async (record: DnsRecordRow) => {
      if (!nsec) return;

      setOpStatus("deleting");
      setOpError(null);
      setDeleteTarget(null);

      try {
        const keyPair: KeyPair = keyPairFromNsec(nsec);

        await publishDeleteEvent(
          [{ type: record.type, name: record.name === "@" ? "" : record.name }],
          keyPair.secretKey
        );

        await loadRecords();
      } catch (err) {
        setOpError(
          err instanceof Error ? err.message : "Failed to delete record"
        );
      } finally {
        setOpStatus("idle");
      }
    },
    [nsec, loadRecords]
  );

  const startEdit = (record: DnsRecordRow) => {
    setEditingId(record.id);
    setEditValue(record.value);
    setEditTtl(record.ttl);
    setValidationError(null);
  };

  const cancelEdit = () => {
    setEditingId(null);
    setValidationError(null);
  };

  // --- No domain param ---
  if (!nameParam) {
    return (
      <div className="mx-auto max-w-[560px] py-20 text-center">
        <h1 className="text-2xl font-bold mb-3">No domain selected</h1>
        <p className="text-muted-foreground mb-6">
          Go to your dashboard to manage domains.
        </p>
        <Link href="/dashboard">
          <Button variant="outline">
            <ArrowLeftIcon className="size-4" />
            Back to Dashboard
          </Button>
        </Link>
      </div>
    );
  }

  return (
    <div className="mx-auto max-w-[960px] py-8 md:py-12">
      {/* Back nav */}
      <Link
        href="/dashboard"
        className="inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground transition-colors mb-6"
      >
        <ArrowLeftIcon className="size-3.5" />
        Back to Domains
      </Link>

      {/* Domain header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4 mb-8">
        <div className="flex items-center gap-3">
          <div className="flex items-center justify-center size-10 rounded-lg bg-primary/10 shrink-0">
            <GlobeIcon className="size-5 text-primary" />
          </div>
          <div>
            <h1 className="text-xl md:text-2xl font-bold font-mono break-all">
              {fqdn}
            </h1>
            <div className="flex items-center gap-3 mt-1">
              {getStatusBadge()}
              {lastSeen > 0 && (
                <span className="text-xs text-muted-foreground">
                  Expires {getExpiryDate()}
                </span>
              )}
            </div>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={loadRecords}
            disabled={pageStatus === "loading"}
          >
            <RefreshCwIcon
              className={`size-3.5 ${pageStatus === "loading" ? "animate-spin" : ""}`}
            />
            Refresh
          </Button>
          {/* Renew placeholder - only show for active domains */}
          {records.length > 0 && (
            <Button variant="outline" size="sm" disabled>
              <ShieldIcon className="size-3.5" />
              Renew
            </Button>
          )}
        </div>
      </div>

      {/* Source status bar */}
      <div className="flex items-center gap-4 mb-6 px-4 py-3 rounded-lg bg-card ring-1 ring-foreground/10">
        <span className="text-xs text-muted-foreground uppercase tracking-wider font-medium">Sources</span>
        {tripartite ? (
          <>
            <SourceIndicator source="api" status={tripartite.api.status} fqdn={fqdn} />
            <SourceIndicator source="nostr" status={tripartite.nostr.status} fqdn={fqdn} />
            <SourceIndicator source="dns" status={tripartite.dns.status} fqdn={fqdn} />
          </>
        ) : (
          <span className="text-xs text-muted-foreground animate-pulse">Loading sources...</span>
        )}
      </div>

      {/* Error banner */}
      {errorMsg && (
        <div className="rounded-lg border border-red-800 bg-red-950/40 px-4 py-3 text-sm text-red-400 mb-6">
          {errorMsg}
          <button
            onClick={loadRecords}
            className="ml-2 underline hover:no-underline"
          >
            Retry
          </button>
        </div>
      )}

      {/* Operation error */}
      {opError && (
        <div className="rounded-lg border border-red-800 bg-red-950/40 px-4 py-3 text-sm text-red-400 mb-6">
          {opError}
        </div>
      )}

      {/* DNS Records section */}
      <div className="rounded-xl bg-card ring-1 ring-foreground/10 overflow-hidden">
        {/* Section header */}
        <div className="flex items-center justify-between px-5 py-4 border-b border-border">
          <div>
            <h2 className="text-base font-semibold">DNS Records</h2>
            <p className="text-xs text-muted-foreground mt-0.5">
              {records.length} record{records.length !== 1 ? "s" : ""}
              {pricing && (
                <span className="ml-1">
                  (Create: {pricing.create_price} sats / Update:{" "}
                  {pricing.update_price} sats)
                </span>
              )}
            </p>
          </div>
          <Button
            size="sm"
            onClick={() => {
              setAddingRecord(true);
              setAddError(null);
            }}
            disabled={addingRecord}
          >
            <PlusIcon className="size-3.5" />
            Add Record
          </Button>
        </div>

        {/* Table header (desktop) */}
        <div className="hidden md:grid grid-cols-[80px_100px_1fr_80px_70px_80px] gap-3 px-5 py-2.5 border-b border-border text-xs text-muted-foreground font-medium uppercase tracking-wider">
          <span>Type</span>
          <span>Name</span>
          <span>Value</span>
          <span className="text-center">TTL</span>
          <span className="text-center">Src</span>
          <span className="text-right">Actions</span>
        </div>

        {/* Loading skeleton */}
        {pageStatus === "loading" && records.length === 0 && (
          <div>
            {[1, 2, 3].map((i) => (
              <div
                key={i}
                className="grid grid-cols-1 md:grid-cols-[80px_100px_1fr_80px_70px_80px] gap-2 md:gap-3 px-5 py-3.5 border-b border-border last:border-b-0"
              >
                <div className="h-4 w-12 bg-muted rounded animate-pulse" />
                <div className="h-4 w-10 bg-muted rounded animate-pulse" />
                <div className="h-4 w-40 bg-muted rounded animate-pulse" />
                <div className="h-4 w-10 bg-muted rounded animate-pulse" />
                <div className="h-4 w-12 bg-muted rounded animate-pulse" />
                <div className="h-4 w-14 bg-muted rounded animate-pulse" />
              </div>
            ))}
          </div>
        )}

        {/* Empty state */}
        {pageStatus === "ready" && records.length === 0 && !addingRecord && (
          <div className="px-5 py-12 text-center">
            <p className="text-muted-foreground text-sm mb-4">
              No DNS records yet. Add one to get started.
            </p>
            <Button
              size="sm"
              onClick={() => {
                setAddingRecord(true);
                setAddError(null);
              }}
            >
              <PlusIcon className="size-3.5" />
              Add First Record
            </Button>
          </div>
        )}

        {/* Records */}
        {records.map((record) => (
          <div
            key={record.id}
            className="grid grid-cols-1 md:grid-cols-[80px_100px_1fr_80px_70px_80px] gap-2 md:gap-3 px-5 py-3.5 border-b border-border last:border-b-0 hover:bg-muted/30 transition-colors"
          >
            {/* Type */}
            <div>
              <span className="md:hidden text-xs text-muted-foreground mr-1">
                Type:
              </span>
              <span className="inline-flex items-center rounded-md bg-secondary px-2 py-0.5 text-xs font-mono font-medium">
                {record.type}
              </span>
            </div>

            {/* Name */}
            <div className="font-mono text-sm truncate">
              <span className="md:hidden text-xs text-muted-foreground mr-1">
                Name:
              </span>
              {record.name}
            </div>

            {/* Value - editable */}
            <div>
              <span className="md:hidden text-xs text-muted-foreground mr-1">
                Value:
              </span>
              {editingId === record.id ? (
                <div className="flex items-center gap-2">
                  <Input
                    value={editValue}
                    onChange={(e) => {
                      setEditValue(e.target.value);
                      setValidationError(null);
                    }}
                    className="h-7 text-sm font-mono"
                    placeholder="Record value"
                  />
                  {validationError && (
                    <span className="text-xs text-red-400">
                      {validationError}
                    </span>
                  )}
                </div>
              ) : (
                <span className="text-sm font-mono truncate block max-w-full">
                  {record.value}
                </span>
              )}
            </div>

            {/* TTL */}
            <div className="text-sm text-muted-foreground text-center">
              <span className="md:hidden text-xs mr-1">TTL:</span>
              {editingId === record.id ? (
                <Input
                  type="number"
                  value={editTtl}
                  onChange={(e) => setEditTtl(Number(e.target.value))}
                  className="h-7 w-16 text-sm text-center"
                />
              ) : (
                record.ttl
              )}
            </div>

            {/* Sources */}
            <div className="flex items-center justify-center gap-0.5">
              <span className="md:hidden text-xs text-muted-foreground mr-1">Sources:</span>
              {record.sources.includes("api") && (
                <SourceIndicator compact source="api" status="ok" fqdn={fqdn} />
              )}
              {record.sources.includes("nostr") && (
                <SourceIndicator compact source="nostr" status="ok" fqdn={fqdn} />
              )}
              {record.sources.includes("dns") && (
                <SourceIndicator compact source="dns" status="ok" fqdn={fqdn} />
              )}
            </div>

            {/* Actions */}
            <div className="flex items-center justify-end gap-1">
              {editingId === record.id ? (
                <>
                  <Button
                    variant="ghost"
                    size="icon-xs"
                    onClick={handleEditSave}
                    disabled={opStatus === "saving"}
                  >
                    {opStatus === "saving" ? (
                      <LoaderIcon className="size-3.5 animate-spin" />
                    ) : (
                      <CheckIcon className="size-3.5 text-emerald-400" />
                    )}
                  </Button>
                  <Button
                    variant="ghost"
                    size="icon-xs"
                    onClick={cancelEdit}
                    disabled={opStatus === "saving"}
                  >
                    <XIcon className="size-3.5 text-muted-foreground" />
                  </Button>
                </>
              ) : (
                <>
                  <Button
                    variant="ghost"
                    size="icon-xs"
                    onClick={() => startEdit(record)}
                    disabled={!!editingId || opStatus !== "idle"}
                  >
                    <PencilIcon className="size-3.5 text-muted-foreground" />
                  </Button>
                  <Button
                    variant="ghost"
                    size="icon-xs"
                    onClick={() => setDeleteTarget(record)}
                    disabled={!!editingId || opStatus !== "idle"}
                  >
                    <TrashIcon className="size-3.5 text-red-400" />
                  </Button>
                </>
              )}
            </div>
          </div>
        ))}

        {/* Add record row */}
        {addingRecord && (
          <div className="grid grid-cols-1 md:grid-cols-[80px_100px_1fr_80px_70px_auto] gap-2 md:gap-3 px-5 py-3.5 border-t-2 border-primary/30 bg-primary/5">
            {/* Type selector */}
            <div>
              <span className="md:hidden text-xs text-muted-foreground mr-1 mb-1 block">
                Type:
              </span>
              <Select value={newType} onValueChange={(v) => { if (v) setNewType(v); }}>
                <SelectTrigger size="sm" className="w-full md:w-[72px]">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {RECORD_TYPES.map((t) => (
                    <SelectItem key={t} value={t}>
                      {t}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>

            {/* Name */}
            <div>
              <span className="md:hidden text-xs text-muted-foreground mr-1 mb-1 block">
                Name:
              </span>
              <Input
                value={newName}
                onChange={(e) => {
                  setNewName(e.target.value);
                  setAddError(null);
                }}
                className="h-7 text-sm font-mono"
                placeholder="@"
              />
            </div>

            {/* Value */}
            <div>
              <span className="md:hidden text-xs text-muted-foreground mr-1 mb-1 block">
                Value:
              </span>
              <Input
                value={newValue}
                onChange={(e) => {
                  setNewValue(e.target.value);
                  setAddError(null);
                }}
                className="h-7 text-sm font-mono"
                placeholder="Record value..."
              />
            </div>

            {/* TTL */}
            <div>
              <span className="md:hidden text-xs text-muted-foreground mr-1 mb-1 block">
                TTL:
              </span>
              <Input
                type="number"
                value={newTtl}
                onChange={(e) => setNewTtl(Number(e.target.value))}
                className="h-7 w-16 text-sm text-center"
              />
            </div>

            {/* Save / Cancel */}
            <div className="flex items-center gap-1">
              <Button
                size="xs"
                onClick={handleSaveNew}
                disabled={opStatus === "saving" || !newValue}
              >
                {opStatus === "saving" ? (
                  <LoaderIcon className="size-3 animate-spin" />
                ) : (
                  <CheckIcon className="size-3" />
                )}
                Save
              </Button>
              <Button
                variant="ghost"
                size="icon-xs"
                onClick={() => {
                  setAddingRecord(false);
                  setAddError(null);
                }}
                disabled={opStatus === "saving"}
              >
                <XIcon className="size-3.5" />
              </Button>
            </div>

            {/* Validation error */}
            {addError && (
              <div className="col-span-full text-xs text-red-400 mt-1">
                {addError}
              </div>
            )}

            {/* Insufficient balance warning */}
            {pricing &&
              pricing.create_price > 0 &&
              balance < pricing.create_price && (
                <div className="col-span-full text-xs text-yellow-400 mt-1">
                  Insufficient balance ({balance} sats). Need{" "}
                  {pricing.create_price} sats to create a record.
                  <Link href="/wallet" className="underline ml-1">
                    Add funds
                  </Link>
                </div>
              )}
          </div>
        )}
      </div>

      {/* Verification section */}
      {tripartite && (
        <div className="rounded-xl border border-border bg-card p-4 mt-6">
          <h3 className="text-sm font-semibold mb-3">Verification</h3>
          {(() => {
            const cmp = compareTripartite(tripartite);
            return (
              <div>
                <div className="flex items-center gap-3 text-sm mb-2">
                  <span className="text-xs text-muted-foreground">
                    API: {cmp.apiCount} records
                  </span>
                  <span className="text-border">|</span>
                  <span className="text-xs text-muted-foreground">
                    Nostr: {cmp.nostrCount} records
                  </span>
                  <span className="text-border">|</span>
                  <span className="text-xs text-muted-foreground">
                    DNS: {cmp.dnsCount} records
                  </span>
                </div>
                {cmp.match ? (
                  <div className="flex items-center gap-2">
                    <span className="text-emerald-400">✓</span>
                    <span className="text-sm text-emerald-400">All sources agree</span>
                  </div>
                ) : (
                  <div className="space-y-1">
                    <div className="flex items-center gap-2">
                      <span className="text-yellow-400">⚠</span>
                      <span className="text-sm text-yellow-400">Sources differ</span>
                    </div>
                    {cmp.onlyInApi.length > 0 && (
                      <p className="text-xs text-muted-foreground">Only in API: {cmp.onlyInApi.join(", ")}</p>
                    )}
                    {cmp.onlyInNostr.length > 0 && (
                      <p className="text-xs text-muted-foreground">Only in Nostr: {cmp.onlyInNostr.join(", ")}</p>
                    )}
                    {cmp.onlyInDns.length > 0 && (
                      <p className="text-xs text-muted-foreground">Only in DNS: {cmp.onlyInDns.join(", ")}</p>
                    )}
                  </div>
                )}
              </div>
            );
          })()}
        </div>
      )}

      {/* Wallet info bar */}
      <div className="flex items-center gap-4 mt-6 px-4 py-3 rounded-lg bg-card ring-1 ring-foreground/10">
        <span className="text-xs text-muted-foreground">Wallet balance</span>
        <span
          className={`text-xs font-mono ${walletStatus === "ready" ? "text-emerald-400" : "text-yellow-400"}`}
        >
          {balance} sats
        </span>
        <div className="h-3 w-px bg-border" />
        <span className="text-xs text-muted-foreground">
          {opStatus === "idle"
            ? "Ready"
            : opStatus === "saving"
              ? "Publishing event..."
              : "Deleting..."}
        </span>
      </div>

      {/* Delete confirmation dialog */}
      <Dialog
        open={!!deleteTarget}
        onOpenChange={(open) => {
          if (!open) setDeleteTarget(null);
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete DNS Record</DialogTitle>
            <DialogDescription>
              Are you sure you want to delete this{" "}
              <span className="font-mono text-foreground">
                {deleteTarget?.type}
              </span>{" "}
              record
              {deleteTarget?.name && (
                <>
                  {" "}
                  for{" "}
                  <span className="font-mono text-foreground">
                    {deleteTarget.name}
                  </span>
                </>
              )}
              ? This action publishes a delete event to Nostr.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <DialogClose render={<Button variant="outline" />}>
              Cancel
            </DialogClose>
            <Button
              variant="destructive"
              onClick={() => {
                if (deleteTarget) handleDelete(deleteTarget);
              }}
              disabled={opStatus === "deleting"}
            >
              {opStatus === "deleting" ? (
                <LoaderIcon className="size-3.5 animate-spin" />
              ) : (
                <TrashIcon className="size-3.5" />
              )}
              Delete
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

export default function DomainPage() {
  return (
    <ErrorBoundary>
      <SiteHeader />
      <main className="px-6 pb-16">
        <Suspense
          fallback={
            <div className="mx-auto max-w-[960px] py-20 text-center text-muted-foreground animate-pulse">
              Loading domain...
            </div>
          }
        >
          <DomainDetailContent />
        </Suspense>
      </main>
      <SiteFooter />
    </ErrorBoundary>
  );
}
