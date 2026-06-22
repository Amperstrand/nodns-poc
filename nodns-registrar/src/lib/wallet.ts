import { Manager, ConsoleLogger } from "coco-cashu-core";
import { IndexedDbRepositories } from "coco-cashu-indexeddb";

export const MINT_URL = "https://testnut.cashu.space";

export interface WalletInitResult {
  manager: Manager;
  mintOnline: boolean;
}

export async function createWalletManager(
  seedGetter: () => Promise<Uint8Array>,
): Promise<WalletInitResult> {
  const repos = new IndexedDbRepositories({});
  await repos.init();

  const logger = new ConsoleLogger("nodns-registrar", { level: "info" });
  const manager = new Manager(repos, seedGetter, logger);

  let mintOnline = true;
  try {
    await manager.mint.addMint(MINT_URL, { trusted: true });
  } catch {
    mintOnline = false;
  }

  if (mintOnline) {
    try {
      await manager.enableMintOperationWatcher({
        watchExistingPendingOnStart: true,
      });
      await manager.enableMintOperationProcessor({
        processIntervalMs: 3000,
      });
    } catch {
      // Non-fatal — wallet still works for balance checks
    }
  }

  return { manager, mintOnline };
}
