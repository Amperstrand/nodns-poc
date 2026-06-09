'use client';

import { WalletProvider } from '@/contexts/WalletContext';
import { IdentityProvider } from '@/contexts/IdentityContext';

export function Providers({ children }: { children: React.ReactNode }) {
  return (
    <IdentityProvider>
      <WalletProvider>
        {children}
      </WalletProvider>
    </IdentityProvider>
  );
}
