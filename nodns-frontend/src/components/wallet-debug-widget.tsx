'use client';

import { useWallet } from '@/contexts/WalletContext';
import { useIdentity } from '@/contexts/IdentityContext';

function truncateNpub(npub: string, chars = 8): string {
  if (!npub) return '';
  return `${npub.slice(0, chars + 6)}...${npub.slice(-chars)}`;
}

export function WalletDebugWidget() {
  const { status, balance, error } = useWallet();
  const { npub, initialized } = useIdentity();

  if (!initialized) {
    return (
      <div className="fixed bottom-4 right-4 z-50 rounded-lg border border-yellow-800 bg-yellow-950/80 px-3 py-2 text-xs font-mono text-yellow-300 backdrop-blur-sm">
        Initializing identity...
      </div>
    );
  }

  const statusColor =
    status === 'ready'
      ? 'border-emerald-800 bg-emerald-950/80 text-emerald-300'
      : status === 'error'
        ? 'border-red-800 bg-red-950/80 text-red-300'
        : 'border-yellow-800 bg-yellow-950/80 text-yellow-300';

  return (
    <div className={`fixed bottom-4 right-4 z-50 rounded-lg border px-3 py-2 text-xs font-mono backdrop-blur-sm ${statusColor}`}>
      <div className="flex items-center gap-3">
        <span>
          {truncateNpub(npub)}
        </span>
        <span className="opacity-60">|</span>
        <span>
          {balance} sats
        </span>
        <span className="opacity-60">|</span>
        <span className="capitalize">{status}</span>
      </div>
      {error && (
        <div className="mt-1 text-red-400 max-w-64 truncate">
          {error}
        </div>
      )}
    </div>
  );
}
