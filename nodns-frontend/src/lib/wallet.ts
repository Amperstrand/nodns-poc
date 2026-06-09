import { Manager, ConsoleLogger } from 'coco-cashu-core';
import { IndexedDbRepositories } from 'coco-cashu-indexeddb';

export const MINT_URL = 'https://nofee.testnut.cashu.space';

export interface WalletInitResult {
  manager: Manager;
  mintOnline: boolean;
}

export async function createWalletManager(
  seedGetter: () => Promise<Uint8Array>,
): Promise<WalletInitResult> {
  const repos = new IndexedDbRepositories({});
  await repos.init();

  const logger = new ConsoleLogger('nodns-wallet', { level: 'info' });

  const manager = new Manager(repos, seedGetter, logger);

  let mintOnline = true;
  try {
    await manager.mint.addMint(MINT_URL);
  } catch (err) {
    console.warn('[nodns-wallet] Mint unavailable during init:', err);
    mintOnline = false;
  }

  return { manager, mintOnline };
}
