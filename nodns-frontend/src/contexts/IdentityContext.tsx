'use client';

import React, { createContext, useContext, useState, useEffect } from 'react';
import { getOrCreateIdentity } from '@/lib/identity';
import type { Identity } from '@/lib/identity';

interface IdentityState {
  npub: string;
  nsec: string;
  pk: string;
  initialized: boolean;
}

const IdentityContext = createContext<IdentityState>({
  npub: '',
  nsec: '',
  pk: '',
  initialized: false,
});

export function IdentityProvider({ children }: { children: React.ReactNode }) {
  const [identity, setIdentity] = useState<IdentityState>({
    npub: '',
    nsec: '',
    pk: '',
    initialized: false,
  });

  useEffect(() => {
    const id: Identity = getOrCreateIdentity();
    setIdentity({
      npub: id.npub,
      nsec: id.nsec,
      pk: id.pk,
      initialized: true,
    });
  }, []);

  return (
    <IdentityContext.Provider value={identity}>
      {children}
    </IdentityContext.Provider>
  );
}

export function useIdentity() {
  return useContext(IdentityContext);
}
