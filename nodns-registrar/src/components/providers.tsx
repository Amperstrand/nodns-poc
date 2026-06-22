"use client";

import { useEffect, type ReactNode } from "react";
import { IdentityProvider } from "@/contexts/IdentityContext";
import { WalletProvider } from "@/contexts/WalletContext";
import { ErrorBoundary, initGlobalErrorHandler } from "@/components/error-boundary";

export function Providers({ children }: { children: ReactNode }) {
  useEffect(() => {
    initGlobalErrorHandler();
  }, []);

  return (
    <ErrorBoundary>
      <IdentityProvider>
        <WalletProvider>{children}</WalletProvider>
      </IdentityProvider>
    </ErrorBoundary>
  );
}
