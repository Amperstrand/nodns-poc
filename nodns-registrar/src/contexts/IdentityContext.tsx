import {
  createContext,
  useContext,
  useState,
  useEffect,
  useCallback,
  type ReactNode,
} from "react";
import { npubEncode } from "nostr-tools/nip19";
import { isExtensionAvailable, decodeNsec } from "@/lib/nostr";
import {
  getAccounts,
  saveAccount,
  removeAccount,
  getLastAccount,
  setLastAccount,
  clearLastAccount,
  getEphemeralNsec,
  generateEphemeral,
  bytesToHex,
} from "@/lib/identity";
import type { SavedAccount, Session } from "@/lib/types";

interface IdentityContextValue {
  session: Session | null;
  npub: string | null;
  nsec: string | null;
  secretKey: Uint8Array | null;
  extensionAvailable: boolean;
  savedAccounts: SavedAccount[];
  loading: boolean;

  loginWithExtension: () => Promise<void>;
  loginWithNsec: (nsec: string, remember: boolean) => Promise<void>;
  loginWithEphemeral: () => void;
  loginWithSavedAccount: (pubkey: string) => Promise<void>;
  generateNewKey: () => void;
  logout: () => void;
  removeSavedAccount: (pubkey: string) => void;
}

const IdentityContext = createContext<IdentityContextValue | null>(null);

export function IdentityProvider({ children }: { children: ReactNode }) {
  const [session, setSession] = useState<Session | null>(null);
  const [extensionAvailable, setExtensionAvailable] = useState(false);
  const [savedAccounts, setSavedAccounts] = useState<SavedAccount[]>([]);
  const [loading, setLoading] = useState(true);
  const [currentNsec, setCurrentNsec] = useState<string | null>(null);

  useEffect(() => {
    setSavedAccounts(getAccounts());

    if (isExtensionAvailable()) {
      setExtensionAvailable(true);
    } else {
      let tries = 0;
      const interval = setInterval(() => {
        tries++;
        if (isExtensionAvailable()) {
          setExtensionAvailable(true);
          clearInterval(interval);
        } else if (tries > 20) {
          clearInterval(interval);
        }
      }, 100);
    }

    const lastPubkey = getLastAccount();
    if (lastPubkey) {
      const account = getAccounts().find((a) => a.pubkey === lastPubkey);
      if (account) {
        try {
          const decoded = decodeNsec(account.nsec);
          setCurrentNsec(account.nsec);
          setSession({
            pubkey: decoded.pubkey,
            secretKeyHex: bytesToHex(decoded.secretKey),
            authMethod: "nsec",
          });
        } catch {
          // fall through to ephemeral
        }
      }
    }

    if (!session) {
      const ephemeralNsec = getEphemeralNsec();
      if (ephemeralNsec) {
        try {
          const decoded = decodeNsec(ephemeralNsec);
          setCurrentNsec(ephemeralNsec);
          setSession({
            pubkey: decoded.pubkey,
            secretKeyHex: bytesToHex(decoded.secretKey),
            authMethod: "ephemeral",
          });
        } catch {
          // ignore
        }
      }
    }

    setLoading(false);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const loginWithExtension = useCallback(async () => {
    if (!isExtensionAvailable()) {
      throw new Error("No Nostr extension found");
    }
    const pk = await window.nostr!.getPublicKey();
    setLastAccount(pk);
    setCurrentNsec(null);
    setSession({
      pubkey: pk,
      secretKeyHex: null,
      authMethod: "extension",
    });
  }, []);

  const loginWithNsec = useCallback(
    async (nsec: string, remember: boolean) => {
      const decoded = decodeNsec(nsec);
      if (remember) {
        saveAccount(decoded.pubkey, nsec);
        setSavedAccounts(getAccounts());
      }
      setLastAccount(decoded.pubkey);
      setCurrentNsec(nsec);
      setSession({
        pubkey: decoded.pubkey,
        secretKeyHex: bytesToHex(decoded.secretKey),
        authMethod: "nsec",
      });
    },
    [],
  );

  const loginWithEphemeral = useCallback(() => {
    const key = generateEphemeral();
    setLastAccount(key.pubkey);
    setCurrentNsec(key.nsec);
    setSession({
      pubkey: key.pubkey,
      secretKeyHex: bytesToHex(key.secretKey),
      authMethod: "ephemeral",
    });
  }, []);

  const loginWithSavedAccount = useCallback(async (pubkey: string) => {
    const account = getAccounts().find((a) => a.pubkey === pubkey);
    if (!account) throw new Error("Account not found");
    const decoded = decodeNsec(account.nsec);
    setLastAccount(pubkey);
    setCurrentNsec(account.nsec);
    setSession({
      pubkey: decoded.pubkey,
      secretKeyHex: bytesToHex(decoded.secretKey),
      authMethod: "nsec",
    });
  }, []);

  const generateNewKey = useCallback(() => {
    const key = generateEphemeral();
    setLastAccount(key.pubkey);
    setCurrentNsec(key.nsec);
    setSession({
      pubkey: key.pubkey,
      secretKeyHex: bytesToHex(key.secretKey),
      authMethod: "ephemeral",
    });
  }, []);

  const logout = useCallback(() => {
    clearLastAccount();
    setCurrentNsec(null);
    setSession(null);
  }, []);

  const removeSavedAccount = useCallback((pubkey: string) => {
    removeAccount(pubkey);
    setSavedAccounts(getAccounts());
  }, []);

  const npub = session ? npubEncode(session.pubkey) : null;
  const secretKey = session?.secretKeyHex
    ? new Uint8Array(
        session.secretKeyHex.match(/[\da-f]{2}/gi)!.map((h) => parseInt(h, 16)),
      )
    : null;

  return (
    <IdentityContext.Provider
      value={{
        session,
        npub,
        nsec: currentNsec,
        secretKey,
        extensionAvailable,
        savedAccounts,
        loading,
        loginWithExtension,
        loginWithNsec,
        loginWithEphemeral,
        loginWithSavedAccount,
        generateNewKey,
        logout,
        removeSavedAccount,
      }}
    >
      {children}
    </IdentityContext.Provider>
  );
}

export function useIdentity(): IdentityContextValue {
  const ctx = useContext(IdentityContext);
  if (!ctx) {
    throw new Error("useIdentity must be used within IdentityProvider");
  }
  return ctx;
}
