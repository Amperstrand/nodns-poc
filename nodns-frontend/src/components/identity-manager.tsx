"use client";

import { useState, useCallback } from "react";
import { Button } from "@/components/ui/button";
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
import { useIdentity } from "@/contexts/IdentityContext";
import { DEFAULT_ZONE } from "@/lib/constants";
import {
  CopyIcon,
  CheckIcon,
  KeyIcon,
  DownloadIcon,
  UploadIcon,
  RefreshCwIcon,
  TriangleAlertIcon,
  EyeIcon,
  EyeOffIcon,
} from "lucide-react";

function truncateMiddle(str: string, start = 16, end = 12): string {
  if (!str) return "";
  if (str.length <= start + end + 3) return str;
  return `${str.slice(0, start)}...${str.slice(-end)}`;
}

function useCopy(): [boolean, (text: string) => void] {
  const [copied, setCopied] = useState(false);

  const copy = useCallback(
    (text: string) => {
      navigator.clipboard.writeText(text).then(() => {
        setCopied(true);
        setTimeout(() => setCopied(false), 2000);
      });
    },
    []
  );

  return [copied, copy];
}

export function IdentityManager() {
  const { npub, nsec, initialized, importKey, resetKey } = useIdentity();

  const [npubCopied, copyNpub] = useCopy();
  const [nsecCopied, copyNsec] = useCopy();

  const [exportOpen, setExportOpen] = useState(false);
  const [importOpen, setImportOpen] = useState(false);
  const [resetOpen, setResetOpen] = useState(false);

  const [showNsec, setShowNsec] = useState(false);
  const [importValue, setImportValue] = useState("");
  const [importError, setImportError] = useState("");

  const handleImport = () => {
    setImportError("");
    const result = importKey(importValue);
    if (result.success) {
      setImportOpen(false);
      setImportValue("");
    } else {
      setImportError(result.error || "Invalid key");
    }
  };

  const handleReset = () => {
    resetKey();
    setResetOpen(false);
  };

  if (!initialized) {
    return (
      <div className="rounded-xl bg-card ring-1 ring-foreground/10 p-6 animate-pulse">
        <div className="h-5 w-40 bg-muted rounded mb-4" />
        <div className="h-4 w-64 bg-muted rounded" />
      </div>
    );
  }

  const domainName = npub ? `${npub.slice(0, 16)}...${DEFAULT_ZONE}` : "";

  return (
    <div className="rounded-xl bg-card ring-1 ring-foreground/10 overflow-hidden">
      <div className="flex items-center gap-2.5 px-5 py-3 border-b border-border bg-muted/30">
        <KeyIcon className="size-4 text-primary" />
        <h2 className="text-xs font-semibold uppercase tracking-wider text-foreground/70">
          Identity &amp; Keys
        </h2>
      </div>

      <div className="p-5 space-y-5">
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          <div>
            <div className="text-xs text-muted-foreground mb-1.5">Public Key (npub)</div>
            <div className="flex items-center gap-2">
              <code className="flex-1 min-w-0 font-mono text-xs text-foreground bg-muted/50 rounded-md px-2.5 py-1.5 truncate">
                {truncateMiddle(npub, 20, 16)}
              </code>
              <Button
                variant="outline"
                size="icon-sm"
                onClick={() => copyNpub(npub)}
                title="Copy npub"
              >
                {npubCopied ? (
                  <CheckIcon className="size-3.5 text-emerald-400" />
                ) : (
                  <CopyIcon className="size-3.5" />
                )}
              </Button>
            </div>
          </div>

          <div>
            <div className="text-xs text-muted-foreground mb-1.5">Your Free Domain</div>
            <div className="font-mono text-xs text-primary bg-primary/5 rounded-md px-2.5 py-1.5 truncate ring-1 ring-primary/20">
              {domainName}
            </div>
          </div>
        </div>

        <div className="flex flex-wrap items-center gap-2 pt-1">
          <Button variant="outline" size="sm" onClick={() => setExportOpen(true)}>
            <DownloadIcon className="size-3.5" />
            Export nsec
          </Button>
          <Button variant="outline" size="sm" onClick={() => setImportOpen(true)}>
            <UploadIcon className="size-3.5" />
            Import nsec
          </Button>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setResetOpen(true)}
            className="text-muted-foreground hover:text-destructive"
          >
            <RefreshCwIcon className="size-3.5" />
            Generate New
          </Button>
        </div>
      </div>

      <Dialog open={exportOpen} onOpenChange={setExportOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <TriangleAlertIcon className="size-4 text-yellow-400" />
              Export Secret Key
            </DialogTitle>
            <DialogDescription>
              Anyone with this key can impersonate you and control all your
              domains. Store it safely and never share it.
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-3">
            <div className="rounded-lg border border-yellow-800/60 bg-yellow-950/30 px-3 py-2.5 text-xs text-yellow-400/90">
              This is your <strong className="font-semibold">nsec</strong> &mdash; the
              private key for your Nostr identity. Anyone who obtains it has full
              control over your records.
            </div>

            <div>
              <div className="flex items-center justify-between mb-1.5">
                <span className="text-xs text-muted-foreground">Your secret key</span>
                <button
                  onClick={() => setShowNsec(!showNsec)}
                  className="flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground transition-colors"
                >
                  {showNsec ? (
                    <>
                      <EyeOffIcon className="size-3" />
                      Hide
                    </>
                  ) : (
                    <>
                      <EyeIcon className="size-3" />
                      Reveal
                    </>
                  )}
                </button>
              </div>
              <div className="relative">
                <code className="block font-mono text-xs text-foreground bg-muted/60 rounded-md px-3 py-2.5 break-all min-h-[2.5rem]">
                  {showNsec ? nsec : "•".repeat(48)}
                </code>
              </div>
            </div>

            <Button
              variant="outline"
              size="sm"
              className="w-full"
              onClick={() => copyNsec(nsec)}
              disabled={!showNsec}
            >
              {nsecCopied ? (
                <>
                  <CheckIcon className="size-3.5 text-emerald-400" />
                  Copied!
                </>
              ) : (
                <>
                  <CopyIcon className="size-3.5" />
                  Copy Secret Key
                </>
              )}
            </Button>
          </div>

          <DialogFooter>
            <DialogClose render={<Button variant="outline" />}>
              Close
            </DialogClose>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={importOpen} onOpenChange={setImportOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Import Secret Key</DialogTitle>
            <DialogDescription>
              Paste your nsec to switch to a different Nostr identity. This will
              replace your current session key.
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-3">
            <div>
              <Input
                type="password"
                placeholder="nsec1..."
                value={importValue}
                onChange={(e) => {
                  setImportValue(e.target.value);
                  setImportError("");
                }}
                className="font-mono"
                aria-invalid={!!importError}
              />
              {importError && (
                <p className="mt-1.5 text-xs text-destructive">{importError}</p>
              )}
            </div>

            <div className="rounded-lg border border-border bg-muted/30 px-3 py-2 text-xs text-muted-foreground">
              Only valid <code className="font-mono text-foreground/80">nsec1...</code> keys
              are accepted. Your current key will be overwritten.
            </div>
          </div>

          <DialogFooter>
            <DialogClose render={<Button variant="outline" />}>
              Cancel
            </DialogClose>
            <Button onClick={handleImport} disabled={!importValue.trim()}>
              <UploadIcon className="size-3.5" />
              Import Key
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={resetOpen} onOpenChange={setResetOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <TriangleAlertIcon className="size-4 text-destructive" />
              Generate New Identity
            </DialogTitle>
            <DialogDescription>
              This will create a brand new keypair and replace your current
              identity. <strong className="text-foreground">Your old records
              cannot be updated</strong> without the old key.
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-3">
            <div className="rounded-lg border border-destructive/40 bg-destructive/5 px-3 py-2.5 text-xs text-destructive/90">
              Make sure you have exported your current nsec if you want to keep
              access to existing domains. This action cannot be undone.
            </div>
          </div>

          <DialogFooter>
            <DialogClose render={<Button variant="outline" />}>
              Cancel
            </DialogClose>
            <Button variant="destructive" onClick={handleReset}>
              <RefreshCwIcon className="size-3.5" />
              Generate New Identity
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
