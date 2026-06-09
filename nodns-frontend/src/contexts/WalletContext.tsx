'use client';

import React, { createContext, useContext, useEffect, useState, useCallback } from 'react';
import type { Manager } from 'coco-cashu-core';
import { createWalletManager, MINT_URL } from '@/lib/wallet';
import { getOrCreateIdentity, nsecToSeed } from '@/lib/identity';

type WalletStatus = 'idle' | 'loading' | 'ready' | 'error';

interface WalletState {
  manager: Manager | null;
  status: WalletStatus;
  error: string | null;
  balance: number;
  mintOnline: boolean;
}

const WalletContext = createContext<WalletState>({
  manager: null,
  status: 'idle',
  error: null,
  balance: 0,
  mintOnline: true,
});

export function WalletProvider({ children }: { children: React.ReactNode }) {
  const [state, setState] = useState<WalletState>({
    manager: null,
    status: 'idle',
    error: null,
    balance: 0,
    mintOnline: true,
  });

  const refreshBalance = useCallback(async (mgr: Manager) => {
    try {
      const balances = await mgr.wallet.getBalances();
      const balance = balances[MINT_URL] ?? 0;
      setState(s => ({ ...s, balance }));
    } catch {
      // Balance refresh failure is non-fatal
    }
  }, []);

  useEffect(() => {
    let disposed = false;
    const cleanups: Array<() => void> = [];

    async function init() {
      setState(s => ({ ...s, status: 'loading' }));
      try {
        const identity = getOrCreateIdentity();
        const seedGetter = async () => nsecToSeed(identity.nsec);
        const { manager, mintOnline: mintOk } = await createWalletManager(seedGetter);

        if (disposed) {
          await manager.dispose();
          return;
        }

        const unsubSaved = manager.on('proofs:saved', () => {
          refreshBalance(manager);
        });
        cleanups.push(unsubSaved);

        const unsubDeleted = manager.on('proofs:deleted', () => {
          refreshBalance(manager);
        });
        cleanups.push(unsubDeleted);

        let balance = 0;
        try {
          const balances = await manager.wallet.getBalances();
          balance = balances[MINT_URL] ?? 0;
        } catch {
        }

        setState({ manager, status: 'ready', error: null, balance, mintOnline: mintOk });
      } catch (err) {
        if (!disposed) {
          setState({ manager: null, status: 'error', error: String(err), balance: 0, mintOnline: false });
        }
      }
    }

    init();

    return () => {
      disposed = true;
      cleanups.forEach(fn => fn());
    };
  }, [refreshBalance]);

  return (
    <WalletContext.Provider value={state}>
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
