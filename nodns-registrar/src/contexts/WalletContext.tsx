"use client";

import {
  createContext,
  useContext,
  useState,
  useEffect,
  useRef,
  useCallback,
  type ReactNode,
} from "react";
import type { Manager } from "coco-cashu-core";
import { PaymentRequest, PaymentRequestTransportType } from "@cashu/cashu-ts";
import { createWalletManager, MINT_URL } from "@/lib/wallet";

interface WalletContextValue {
  manager: Manager | null;
  balance: number;
  mintUrl: string;
  ready: boolean;
  topUp: (amount: number) => Promise<{ invoice: string; operationId: string }>;
  checkTopUpStatus: (operationId: string) => Promise<boolean>;
  refreshBalance: () => Promise<void>;
  sendTokens: (amountSats: number) => Promise<string>;
  receiveTokens: (token: string) => Promise<number>;
  createPaymentRequest: (amountSats: number, description: string) => string;
}

const WalletContext = createContext<WalletContextValue | null>(null);

export function WalletProvider({ children }: { children: ReactNode }) {
  const [manager, setManager] = useState<Manager | null>(null);
  const [balance, setBalance] = useState(0);
  const [ready, setReady] = useState(false);
  const managerRef = useRef<Manager | null>(null);

  const refreshBalance = useCallback(async () => {
    const mgr = managerRef.current;
    if (!mgr) return;
    try {
      const balances = await mgr.wallet.getBalances();
      setBalance(balances[MINT_URL] ?? 0);
    } catch {
      // non-fatal
    }
  }, []);

  useEffect(() => {
    let disposed = false;
    const cleanups: Array<() => void> = [];

    async function init() {
      setReady(false);
      try {
        const seedGetter = async () => {
          const { getWalletSeed } = await import("@/lib/identity");
          return getWalletSeed();
        };
        const { manager: mgr, mintOnline } = await createWalletManager(seedGetter);

        if (disposed) {
          await mgr.dispose();
          return;
        }

        managerRef.current = mgr;
        setManager(mgr);

        const unsubSaved = mgr.on("proofs:saved", () => refreshBalance());
        cleanups.push(unsubSaved);

        const unsubDeleted = mgr.on("proofs:deleted", () => refreshBalance());
        cleanups.push(unsubDeleted);

        const unsubFinalized = mgr.on("mint-op:finalized", () => refreshBalance());
        cleanups.push(unsubFinalized);

        if (mintOnline) {
          try {
            const balances = await mgr.wallet.getBalances();
            setBalance(balances[MINT_URL] ?? 0);
          } catch {
            // non-fatal
          }
        }

        setReady(true);
      } catch (e) {
        console.error("Wallet init failed:", e);
        setReady(true);
      }
    }

    init();

    return () => {
      disposed = true;
      cleanups.forEach((fn) => fn());
      if (managerRef.current) {
        managerRef.current.dispose().catch(() => {});
        managerRef.current = null;
      }
      setManager(null);
    };
  }, [refreshBalance]);

  const topUp = useCallback(
    async (amount: number): Promise<{ invoice: string; operationId: string }> => {
      const mgr = managerRef.current;
      if (!mgr) throw new Error("Wallet not ready");
      const pending = await mgr.ops.mint.prepare({
        mintUrl: MINT_URL,
        amount,
        method: "bolt11",
        methodData: {},
      });
      return { invoice: pending.request, operationId: pending.id };
    },
    [],
  );

  const checkTopUpStatus = useCallback(
    async (operationId: string): Promise<boolean> => {
      const mgr = managerRef.current;
      if (!mgr) throw new Error("Wallet not ready");
      try {
        await mgr.ops.mint.finalize(operationId);
        await refreshBalance();
        return true;
      } catch {
        return false;
      }
    },
    [refreshBalance],
  );

  const sendTokens = useCallback(
    async (amountSats: number): Promise<string> => {
      const mgr = managerRef.current;
      if (!mgr) throw new Error("Wallet not ready");
      const prepared = await mgr.ops.send.prepare({
        mintUrl: MINT_URL,
        amount: amountSats,
      });
      const result = await mgr.ops.send.execute(prepared.id);
      return typeof result === "string"
        ? result
        : JSON.stringify(result);
    },
    [],
  );

  const receiveTokens = useCallback(
    async (token: string): Promise<number> => {
      const mgr = managerRef.current;
      if (!mgr) throw new Error("Wallet not ready");
      const prepared = await mgr.ops.receive.prepare({ token });
      const result = await mgr.ops.receive.execute(prepared.id);
      await refreshBalance();
      const proofList = Array.isArray(result) ? result : [result];
      let total = 0;
      for (const proof of proofList) {
        if (typeof proof === "object" && proof && "amount" in proof) {
          total += (proof as { amount: number }).amount;
        }
      }
      return total;
    },
    [refreshBalance],
  );

  const createPaymentRequest = useCallback(
    (amountSats: number, description: string): string => {
      const pr = new PaymentRequest(
        [
          {
            type: PaymentRequestTransportType.POST,
            target: MINT_URL,
          },
        ],
        crypto.randomUUID(),
        amountSats,
        "sat",
        [MINT_URL],
        description,
      );
      return pr.toEncodedCreqA();
    },
    [],
  );

  return (
    <WalletContext.Provider
      value={{
        manager,
        balance,
        mintUrl: MINT_URL,
        ready,
        topUp,
        checkTopUpStatus,
        refreshBalance,
        sendTokens,
        receiveTokens,
        createPaymentRequest,
      }}
    >
      {children}
    </WalletContext.Provider>
  );
}

export function useWallet(): WalletContextValue {
  const ctx = useContext(WalletContext);
  if (!ctx) {
    throw new Error("useWallet must be used within WalletProvider");
  }
  return ctx;
}
