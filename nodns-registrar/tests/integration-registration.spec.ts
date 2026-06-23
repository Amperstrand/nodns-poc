import { test, expect, type Page } from "@playwright/test";

const BASE_URL = "https://nodns-registrar.pages.dev";
const API_BASE = "https://nodns.shop";

async function clearState(page: Page) {
  await page.goto(BASE_URL);
  await page.evaluate(() => {
    localStorage.clear();
    sessionStorage.clear();
  });
  await page.reload();
  await page.waitForLoadState("networkidle");
}

async function loginEphemeral(page: Page) {
  await page.getByRole("button", { name: "Sign In" }).click();
  await page.getByRole("button", { name: "Try with ephemeral key" }).click();
  await page.getByRole("link", { name: "Dashboard" }).waitFor({ timeout: 10_000 });
}

async function waitForBalance(page: Page, minBalance: number, timeout = 30_000) {
  const start = Date.now();
  while (Date.now() - start < timeout) {
    const text = await page.evaluate(() => document.body.innerText);
    const match = text.match(/(\d+)\s*Test sats/);
    if (match && parseInt(match[1], 10) >= minBalance) return;
    await page.waitForTimeout(2000);
  }
  throw new Error(`Balance did not reach ${minBalance} within ${timeout}ms`);
}

async function checkDomainRegistered(name: string, timeout = 60_000): Promise<boolean> {
  const start = Date.now();
  while (Date.now() - start < timeout) {
    try {
      const res = await fetch(
        `${API_BASE}/api/check?name=${encodeURIComponent(name)}&zone=nodns.shop`,
      );
      if (res.ok) {
        const data = await res.json();
        if (data?.api?.registered || data?.dns?.registered) return true;
      }
    } catch {
      // retry
    }
    await new Promise((r) => setTimeout(r, 5000));
  }
  return false;
}

test.describe("Full registration flow integration", () => {
  test.describe.configure({ timeout: 180_000 });

  test("[INTEGRATION] top-up wallet → balance available in dashboard", async ({
    page,
  }) => {
    await clearState(page);
    await loginEphemeral(page);

    await page.goto(`${BASE_URL}/wallet`);
    await page.getByRole("heading", { name: "Wallet" }).waitFor({ timeout: 30_000 });

    await page.getByPlaceholder("Amount to top up").fill("10");
    await page.getByRole("button", { name: "Generate Invoice" }).click();

    await expect(page.getByText("Invoice paid")).toBeVisible({
      timeout: 60_000,
    });

    await page.goto(`${BASE_URL}/dashboard`);
    await page.waitForLoadState("networkidle");

    await expect(page.getByText(/Wallet Balance/i)).toBeVisible({
      timeout: 15_000,
    });
    await expect(page.locator("text=10").first()).toBeVisible({
      timeout: 10_000,
    });
  });

  test.fixme(
    "[INTEGRATION] register custom domain via UI (requires NIP-26 delegation support)",
    async () => {
      test.skip(true, "Custom name registration requires NIP-26 delegation — not yet implemented in registrar");
    },
  );

  test("[INTEGRATION] npub-derived TXT record publishes via nak and resolves", async () => {
    const { execSync } = await import("child_process");
    const testValue = `npub-integration-${Date.now().toString(36)}`;

    execSync(
      `nak event -k 11111 -c "" -t 'record=TXT;;3600;${testValue}' wss://relay.cashu.email wss://nos.lol`,
      { timeout: 15_000 },
    );

    const start = Date.now();
    let found = false;
    while (Date.now() - start < 60_000) {
      try {
        const res = await fetch(
          `${API_BASE}/api/records?pubkey=79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798`,
        );
        const data = await res.json();
        const records = data?.records ?? [];
        if (records.some((r: { rdata: string }) => r.rdata === testValue)) {
          found = true;
          break;
        }
      } catch {
      }
      await new Promise((r) => setTimeout(r, 5000));
    }
    expect(found, `TXT record "${testValue}" should appear in bot API within 60s`).toBe(true);
  });
});
