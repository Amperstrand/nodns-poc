'use client';

import React, { createContext, useContext, useState, useEffect, useCallback } from 'react';
import { getOrCreateIdentity, importIdentity, resetIdentity } from '@/lib/identity';
import type { Identity } from '@/lib/identity';

interface IdentityState {
  npub: string;
  nsec: string;
  pk: string;
  initialized: boolean;
  importKey: (nsec: string) => { success: boolean; error?: string };
  resetKey: () => void;
}

const IdentityContext = createContext<IdentityState>({
  npub: '',
  nsec: '',
  pk: '',
  initialized: false,
  importKey: () => ({ success: false, error: 'Not initialized' }),
  resetKey: () => {},
});

function toState(id: Identity) {
  return {
    npub: id.npub,
    nsec: id.nsec,
    pk: id.pk,
    initialized: true,
  };
}

export function IdentityProvider({ children }: { children: React.ReactNode }) {
  const [identity, setIdentity] = useState<Omit<IdentityState, 'importKey' | 'resetKey'>>({
    npub: '',
    nsec: '',
    pk: '',
    initialized: false,
  });

  useEffect(() => {
    requestAnimationFrame(() => {
      const id = getOrCreateIdentity();
      setIdentity(toState(id));
    });
  }, []);

  const importKey = useCallback((nsec: string): { success: boolean; error?: string } => {
    try {
      const id = importIdentity(nsec);
      setIdentity(toState(id));
      return { success: true };
    } catch (err) {
      return {
        success: false,
        error: err instanceof Error ? err.message : 'Failed to import key',
      };
    }
  }, []);

  const resetKey = useCallback(() => {
    const id = resetIdentity();
    setIdentity(toState(id));
  }, []);

  return (
    <IdentityContext.Provider value={{ ...identity, importKey, resetKey }}>
      {children}
    </IdentityContext.Provider>
  );
}

export function useIdentity() {
  return useContext(IdentityContext);
}
