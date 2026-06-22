"use client";

import { Suspense, useState, useEffect, useCallback, useMemo } from "react";
import Link from "next/link";
import { useSearchParams } from "next/navigation";
import { useIdentity } from "@/contexts/IdentityContext";
import { useWallet } from "@/contexts/WalletContext";
import { fetchRecords, fetchPricing } from "@/lib/api";
import {
  buildRecordTag,
  buildCashuTag,
  signAndPublish,
  subscribeToRecords,
} from "@/lib/nostr";
import { validateRecord } from "@/lib/validation";
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

const selectClass =
  "flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring cursor-pointer";

function DomainDetailContent() {
  const searchParams = useSearchParams();
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
        <CardHeader>
          <CardTitle>Add Record</CardTitle>
          <CardDescription>
            Publish a new DNS record via a signed Nostr event.
            {createCost > 0 && ` This operation costs ${formatSats(createCost)}.`}
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="grid grid-cols-1 md:grid-cols-[100px_1fr] gap-4">
            <div className="space-y-1.5">
              <label className="text-xs font-medium text-muted-foreground">
                Type
              </label>
              <select
                value={formType}
                onChange={(e) => setFormType(e.target.value)}
                className={selectClass}
              >
                {DNS_TYPES.map((t) => (
                  <option key={t} value={t}>
                    {t}
                  </option>
                ))}
              </select>
            </div>
            <div className="space-y-1.5">
              <label className="text-xs font-medium text-muted-foreground">
                Name
              </label>
              <Input
                value={formName}
                onChange={(e) => setFormName(e.target.value)}
                placeholder={
                  isNpubDomain ? "Empty = your npub apex" : queryName
                }
                className="font-mono"
              />
            </div>
          </div>

          <div className="space-y-1.5">
            <label className="text-xs font-medium text-muted-foreground">
              Value
            </label>
            <Input
              value={formValue}
              onChange={(e) => setFormValue(e.target.value)}
              placeholder={VALUE_PLACEHOLDERS[formType] || ""}
              className="font-mono"
            />
            {formType === "MX" && (
              <p className="text-xs text-muted-foreground">
                Format: priority followed by mail host (e.g.{" "}
                <code className="font-mono">10 mail.example.com.</code>)
              </p>
            )}
          </div>

          <div className="grid grid-cols-1 md:grid-cols-[140px_1fr] gap-4">
            <div className="space-y-1.5">
              <label className="text-xs font-medium text-muted-foreground">
                TTL (seconds)
              </label>
              <Input
                type="number"
                value={formTtl}
                onChange={(e) => setFormTtl(parseInt(e.target.value) || 3600)}
                min={60}
                max={86400}
              />
            </div>
            <div className="flex items-end">
              <Button
                onClick={handleAddRecord}
                disabled={busy || !formValue.trim() || insufficientFunds}
                className="w-full md:w-auto"
              >
                {busy
                  ? "Publishing..."
                  : createCost > 0
                    ? `Add Record (${formatSats(createCost)})`
                    : "Add Record"}
              </Button>
            </div>
          </div>

          {insufficientFunds && (
            <div className="flex items-center gap-3">
              <p className="text-xs text-destructive">
                Need {createCost} sats, have {balance} sats.
              </p>
              <Link href="/wallet">
                <Button variant="outline" size="sm">
                  Top up wallet
                </Button>
              </Link>
            </div>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Records</CardTitle>
          <CardDescription>
            {records.length} record{records.length !== 1 ? "s" : ""} for {fqdn}
          </CardDescription>
        </CardHeader>
        <CardContent>
          {loading ? (
            <div className="py-12 text-center text-muted-foreground">
              Loading records...
            </div>
          ) : records.length === 0 ? (
            <div className="py-12 text-center text-muted-foreground">
              No records yet. Add your first record above.
            </div>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b border-border text-left text-muted-foreground">
                    <th className="pb-2 pr-4 font-medium">Type</th>
                    <th className="pb-2 pr-4 font-medium">Name</th>
                    <th className="pb-2 pr-4 font-medium">Value</th>
                    <th className="pb-2 pr-4 font-medium">TTL</th>
                    <th className="pb-2 font-medium text-right">Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {records.map((record, i) => {
                    const rid =
                      record.id ||
                      `${record.record_type}-${record.name}-${record.rdata}-${record.ttl}-${i}`;
                    const isEditing = editingId === rid;
                    return (
                      <tr
                        key={rid}
                        className="border-b border-border/50 last:border-0"
                      >
                        <td className="py-3 pr-4">
                          <Badge className="font-mono">
                            {record.record_type}
                          </Badge>
                        </td>
                        <td className="py-3 pr-4 font-mono text-muted-foreground whitespace-nowrap">
                          {record.name || "@ (apex)"}
                        </td>
                        <td className="py-3 pr-4 font-mono break-all max-w-md">
                          {isEditing ? (
                            <Input
                              value={editValue}
                              onChange={(e) => setEditValue(e.target.value)}
                              className="h-8 font-mono"
                            />
                          ) : (
                            record.rdata
                          )}
                        </td>
                        <td className="py-3 pr-4 font-mono text-muted-foreground whitespace-nowrap">
                          {isEditing ? (
                            <Input
                              type="number"
                              value={editTtl}
                              onChange={(e) =>
                                setEditTtl(parseInt(e.target.value) || 3600)
                              }
                              className="h-8 w-24"
                            />
                          ) : (
                            `${record.ttl}s`
                          )}
                        </td>
                        <td className="py-3 text-right whitespace-nowrap">
                          {isEditing ? (
                            <div className="flex gap-2 justify-end">
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
                            <div className="flex gap-2 justify-end">
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
                                variant="destructive"
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
                  })}
                </tbody>
              </table>
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
                  <Badge className="font-mono">
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

export default function DomainPage() {
  return (
    <Suspense
      fallback={
        <Card>
          <CardContent className="py-16 text-center text-muted-foreground">
            Loading domain...
          </CardContent>
        </Card>
      }
    >
      <DomainDetailContent />
    </Suspense>
  );
}
