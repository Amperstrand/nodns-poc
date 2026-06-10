'use client';

import React, { createContext, useContext, useEffect, useRef, useState, useCallback } from 'react';
import type { Manager } from 'coco-cashu-core';
import { createWalletManager, MINT_URL } from '@/lib/wallet';
import { getOrCreateIdentity, nsecToSeed } from '@/lib/identity';

type WalletStatus = 'idle' | 'loading' | 'ready' | 'error';
type TopUpState = 'idle' | 'requesting' | 'waiting' | 'minting' | 'done' | 'error';

interface WalletState {
  manager: Manager | null;
  status: WalletStatus;
  error: string | null;
  balance: number;
  mintOnline: boolean;
  topUp: (amount: number) => Promise<{ invoice: string; operationId: string }>;
  topUpState: TopUpState;
  topUpError: string | null;
}

const defaultTopUp = async () => { throw new Error('Wallet not initialized'); };

const WalletContext = createContext<WalletState>({
  manager: null,
  status: 'idle',
  error: null,
  balance: 0,
  mintOnline: true,
  topUp: defaultTopUp,
  topUpState: 'idle',
  topUpError: null,
});

export function WalletProvider({ children }: { children: React.ReactNode }) {
  const [status, setStatus] = useState<WalletStatus>('idle');
  const [error, setError] = useState<string | null>(null);
  const [balance, setBalance] = useState(0);
  const [mintOnline, setMintOnline] = useState(true);
  const [topUpState, setTopUpState] = useState<TopUpState>('idle');
  const [topUpError, setTopUpError] = useState<string | null>(null);
  const managerRef = useRef<Manager | null>(null);

  const refreshBalance = useCallback(async () => {
    const mgr = managerRef.current;
    if (!mgr) return;
    try {
      const balances = await mgr.wallet.getBalances();
      setBalance(balances[MINT_URL] ?? 0);
    } catch {}
  }, []);

  const topUp = useCallback(async (amount: number): Promise<{ invoice: string; operationId: string }> => {
    const mgr = managerRef.current;
    if (!mgr) throw new Error('Wallet not initialized');

    setTopUpState('requesting');
    setTopUpError(null);

    try {
      const pending = await mgr.ops.mint.prepare({
        mintUrl: MINT_URL,
        amount,
        method: 'bolt11',
        methodData: {},
      });

      setTopUpState('waiting');
      return { invoice: pending.request, operationId: pending.id };
    } catch (err) {
      setTopUpState('error');
      setTopUpError(err instanceof Error ? err.message : String(err));
      throw err;
    }
  }, []);

  useEffect(() => {
    let disposed = false;
    const cleanups: Array<() => void> = [];

    async function init() {
      setStatus('loading');
      try {
        const identity = getOrCreateIdentity();
        const seedGetter = async () => nsecToSeed(identity.nsec);
        const { manager, mintOnline: mintOk } = await createWalletManager(seedGetter);

        if (disposed) {
          await manager.dispose();
          return;
        }

        managerRef.current = manager;

        const unsubSaved = manager.on('proofs:saved', () => refreshBalance());
        cleanups.push(unsubSaved);

        const unsubDeleted = manager.on('proofs:deleted', () => refreshBalance());
        cleanups.push(unsubDeleted);

        const unsubFinalized = manager.on('mint-op:finalized', () => {
          setTopUpState('done');
          refreshBalance();
        });
        cleanups.push(unsubFinalized);

        const unsubQuoteChanged = manager.on('mint-op:quote-state-changed', (payload) => {
          const op = payload.operation;
          if ('state' in op && op.state === 'failed') {
            setTopUpState('error');
            setTopUpError('error' in op ? (op.error ?? 'Mint operation failed') : 'Mint operation failed');
          } else if ('state' in op && op.state === 'executing') {
            setTopUpState('minting');
          }
        });
        cleanups.push(unsubQuoteChanged);

        let bal = 0;
        try {
          const balances = await manager.wallet.getBalances();
          bal = balances[MINT_URL] ?? 0;
        } catch {}

        setError(null);
        setMintOnline(mintOk);
        setBalance(bal);
        setStatus('ready');
      } catch (err) {
        if (!disposed) {
          setError(String(err));
          setMintOnline(false);
          setStatus('error');
        }
      }
    }

    init();

    return () => {
      disposed = true;
      cleanups.forEach(fn => fn());
      if (managerRef.current) {
        managerRef.current.dispose().catch(() => {});
        managerRef.current = null;
      }
    };
  }, [refreshBalance]);

  return (
    <WalletContext.Provider value={{
      manager: managerRef.current,
      status,
      error,
      balance,
      mintOnline,
      topUp,
      topUpState,
      topUpError,
    }}>
      {children}
    </WalletContext.Provider>
  );
}

export function useWallet() {
  return useContext(WalletContext);
}

export function useBalance() {
  const { balance } = useContext(WalletContext);
  return balance;
}
