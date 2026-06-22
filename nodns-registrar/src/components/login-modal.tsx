"use client";

import { useState, useCallback } from "react";
import { useIdentity } from "@/contexts/IdentityContext";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { validateNsec } from "@/lib/validation";
interface LoginModalProps {
  open: boolean;
  onClose: () => void;
}

export function LoginModal({ open, onClose }: LoginModalProps) {
  const {
    extensionAvailable,
    savedAccounts,
    loginWithExtension,
    loginWithNsec,
    loginWithEphemeral,
    loginWithSavedAccount,
    generateNewKey,
    removeSavedAccount,
  } = useIdentity();

  const [nsecInput, setNsecInput] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [showNsec, setShowNsec] = useState(false);
  const [rememberKey, setRememberKey] = useState(true);

  const handleExtensionLogin = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      await loginWithExtension();
      onClose();
    } catch (e) {
      const msg = String(e);
      if (msg.includes("timed out") || msg.includes("timeout")) {
        setError("Extension timed out. If using Amber, make sure the app is open.");
      } else if (msg.includes("reject") || msg.includes("denied") || msg.includes("cancel")) {
        setError("Request rejected by extension.");
      } else {
        setError("Extension error: " + msg);
      }
    } finally {
      setBusy(false);
    }
  }, [loginWithExtension, onClose]);

  const handleNsecLogin = useCallback(async () => {
    setError(null);
    const nsec = nsecInput.trim();
    const vErr = validateNsec(nsec);
    if (vErr) {
      setError(vErr);
      return;
    }
    setBusy(true);
    try {
      await loginWithNsec(nsec, rememberKey);
      setNsecInput("");
      onClose();
    } catch (e) {
      setError("Failed to decode nsec: " + String(e));
    } finally {
      setBusy(false);
    }
  }, [nsecInput, loginWithNsec, rememberKey, onClose]);

  const handleGenerate = useCallback(() => {
    setError(null);
    generateNewKey();
    onClose();
  }, [generateNewKey, onClose]);

  const handleSavedAccount = useCallback(
    async (pubkey: string) => {
      setBusy(true);
      setError(null);
      try {
        await loginWithSavedAccount(pubkey);
        onClose();
      } catch (e) {
        setError("Failed to login: " + String(e));
      } finally {
        setBusy(false);
      }
    },
    [loginWithSavedAccount, onClose],
  );

  const handleEphemeral = useCallback(() => {
    loginWithEphemeral();
    onClose();
  }, [loginWithEphemeral, onClose]);

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-[100] flex items-center justify-center bg-black/70 p-4"
      onClick={(e) => e.target === e.currentTarget && onClose()}
    >
      <Card className="w-full max-w-md max-h-[90vh] overflow-y-auto">
        <CardHeader>
          <CardTitle>Sign in to NoDNS</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          {error && (
            <div className="rounded-md border border-destructive/50 bg-destructive/10 p-3 text-sm text-destructive">
              {error}
            </div>
          )}

          {savedAccounts.length > 0 && (
            <div className="space-y-2">
              <p className="text-xs font-medium text-muted-foreground">
                SAVED ACCOUNTS
              </p>
              {savedAccounts.map((a) => (
                <div
                  key={a.pubkey}
                  className="flex items-center justify-between rounded-md border border-border bg-secondary/50 px-3 py-2"
                >
                  <button
                    className="flex-1 text-left text-sm hover:text-primary"
                    onClick={() => handleSavedAccount(a.pubkey)}
                    disabled={busy}
                  >
                    {a.npub.slice(0, 20)}...{a.npub.slice(-10)}
                  </button>
                  <button
                    className="ml-2 text-muted-foreground hover:text-destructive"
                    onClick={() => removeSavedAccount(a.pubkey)}
                    aria-label="Remove account"
                  >
                    ✕
                  </button>
                </div>
              ))}
            </div>
          )}

          <Button
            className="w-full"
            variant="outline"
            onClick={handleEphemeral}
            disabled={busy}
          >
            Try with ephemeral key
          </Button>

          <div className="relative">
            <div className="absolute inset-0 flex items-center">
              <span className="w-full border-t border-border" />
            </div>
            <div className="relative flex justify-center text-xs">
              <span className="bg-card px-2 text-muted-foreground">
                or use Nostr extension
              </span>
            </div>
          </div>

          {extensionAvailable ? (
            <Button
              className="w-full"
              onClick={handleExtensionLogin}
              disabled={busy}
            >
              {busy ? "Waiting for extension..." : "Sign In with Extension"}
            </Button>
          ) : (
            <p className="text-center text-xs text-muted-foreground">
              No extension detected. Install{" "}
              <a
                href="https://getalby.com"
                target="_blank"
                rel="noopener"
                className="text-primary hover:underline"
              >
                Alby
              </a>{" "}
              or{" "}
              <a
                href="https://github.com/jiftechnify/nos2x"
                target="_blank"
                rel="noopener"
                className="text-primary hover:underline"
              >
                nos2x
              </a>
            </p>
          )}

          <div className="relative">
            <div className="absolute inset-0 flex items-center">
              <span className="w-full border-t border-border" />
            </div>
            <div className="relative flex justify-center text-xs">
              <span className="bg-card px-2 text-muted-foreground">
                or paste your nsec
              </span>
            </div>
          </div>

          <div className="space-y-2">
            <div className="rounded-md border border-destructive/30 bg-destructive/5 p-2 text-xs text-muted-foreground">
              <strong className="text-destructive">Warning:</strong> Pasting
              your nsec gives full access to your identity. Use a Nostr
              extension instead when possible.
            </div>
            <div className="relative">
              <Input
                type={showNsec ? "text" : "password"}
                placeholder="nsec1..."
                value={nsecInput}
                onChange={(e) => setNsecInput(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && handleNsecLogin()}
                className="pr-12"
              />
              <button
                className="absolute right-3 top-1/2 -translate-y-1/2 text-xs text-muted-foreground hover:text-foreground"
                onClick={() => setShowNsec(!showNsec)}
              >
                {showNsec ? "Hide" : "Show"}
              </button>
            </div>
            <label className="flex items-center gap-2 text-xs">
              <input
                type="checkbox"
                checked={rememberKey}
                onChange={(e) => setRememberKey(e.target.checked)}
              />
              Remember on this device
            </label>
            <Button
              className="w-full"
              variant="secondary"
              onClick={handleNsecLogin}
              disabled={busy || !nsecInput.trim()}
            >
              Sign In with nsec
            </Button>
          </div>

          <div className="relative">
            <div className="absolute inset-0 flex items-center">
              <span className="w-full border-t border-border" />
            </div>
            <div className="relative flex justify-center text-xs">
              <span className="bg-card px-2 text-muted-foreground">
                or generate a new key
              </span>
            </div>
          </div>

          <Button
            className="w-full"
            variant="ghost"
            onClick={handleGenerate}
            disabled={busy}
          >
            Generate New Identity
          </Button>
        </CardContent>
      </Card>
    </div>
  );
}
