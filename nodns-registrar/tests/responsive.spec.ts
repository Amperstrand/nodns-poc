import { test, expect, type Page } from "@playwright/test";

const consoleErrors: string[] = [];

async function clearStorageAndReload(page: Page) {
  await page.evaluate(() => {
    localStorage.clear();
    sessionStorage.clear();
  });
  await page.reload();
  await page.waitForLoadState("networkidle");
}

async function loginEphemeral(page: Page) {
  await page.getByRole("button", { name: "Sign In" }).click();
  await page
    .getByRole("button", { name: "Try with ephemeral key" })
    .click();
  await page.getByRole("link", { name: "Dashboard" }).waitFor();
}

test.beforeEach(async ({ page }) => {
  consoleErrors.length = 0;

  await page.goto("/");
  await clearStorageAndReload(page);

  page.on("console", (msg) => {
    if (msg.type() === "error") {
      consoleErrors.push(msg.text());
    }
  });
});

test.afterEach(() => {
  const significant = consoleErrors.filter(
    (e) =>
      !e.includes("WebSocket") &&
      !e.includes("Failed to fetch") &&
      !e.includes("NetworkError") &&
      !e.includes("ERR_") &&
      !e.includes("cashu") &&
      !e.includes("Cashu") &&
      !e.includes("indexedDB") &&
      !e.includes("IDB") &&
      !e.includes("unhandledrejection") &&
      !e.includes("wallet") &&
      !e.includes("mint") &&
      !e.includes("relay"),
  );
  expect(
    significant,
    `Unexpected console errors:\n${significant.join("\n")}`,
  ).toEqual([]);
});

test.describe("Responsive at 375px", () => {
  test.use({ viewport: { width: 375, height: 812 } });

  test("[HAPPY] landing page hero + search render correctly at 375px", async ({
    page,
  }) => {
    await expect(
      page.getByRole("heading", { name: /find your.*nodns.*name/i }),
    ).toBeVisible();
    await expect(page.getByPlaceholder("alice")).toBeVisible();
    await expect(
      page.getByRole("button", { name: "Search" }),
    ).toBeVisible();
  });

  test("[HAPPY] header collapses gracefully at 375px (balance badge hidden, npub hidden)", async ({
    page,
  }) => {
    await loginEphemeral(page);

    await expect(
      page.getByRole("link", { name: "Dashboard" }),
    ).toBeVisible();

    const npubCode = page.locator("code").filter({ hasText: /npub1/ });
    await expect(npubCode).not.toBeVisible();
  });

  test("[HAPPY] dashboard cards stack vertically at 375px", async ({
    page,
  }) => {
    await loginEphemeral(page);

    await page.goto("/#/dashboard");
    await page.waitForLoadState("networkidle");

    await expect(page.getByText("Wallet Balance")).toBeVisible({
      timeout: 15_000,
    });
    await expect(page.getByText("Total Records")).toBeVisible();

    const balanceCard = page.getByText("Wallet Balance");
    const recordsCard = page.getByText("Total Records");
    const balanceBox = await balanceCard.boundingBox();
    const recordsBox = await recordsCard.boundingBox();

    expect(balanceBox).not.toBeNull();
    expect(recordsBox).not.toBeNull();
    expect(Math.abs(balanceBox!.x - recordsBox!.x)).toBeLessThan(10);
  });

  test("[HAPPY] wallet grid becomes single column at 375px", async ({
    page,
  }) => {
    await loginEphemeral(page);

    await page.goto("/#/wallet");

    await expect(
      page.getByRole("heading", { name: "Wallet" }),
    ).toBeVisible({ timeout: 20_000 });

    const topUp = page.getByText("Top Up").first();
    const sendTokens = page.getByText("Send Tokens").first();

    const topUpBox = await topUp.boundingBox();
    const sendBox = await sendTokens.boundingBox();

    expect(topUpBox).not.toBeNull();
    expect(sendBox).not.toBeNull();
    expect(Math.abs(topUpBox!.x - sendBox!.x)).toBeLessThan(10);
  });

  test("[HAPPY] domain form is usable at 375px", async ({ page }) => {
    await loginEphemeral(page);

    await page.goto("/#/domain?name=testmobile&zone=nodns.shop");
    await page.waitForLoadState("networkidle");

    await expect(page.getByText("Add Record").first()).toBeVisible({
      timeout: 15_000,
    });
    await expect(page.getByRole("combobox")).toBeVisible();
  });
});
