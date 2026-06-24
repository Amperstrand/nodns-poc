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

test.describe("Wallet page", () => {
  test("[FAIL] not logged in still renders wallet (no identity gate)", async ({
    page,
  }) => {
    await page.goto("/#/wallet");

    await expect(
      page.getByRole("heading", { name: "Wallet" }),
    ).toBeVisible({ timeout: 20_000 });
  });

  test("[HAPPY] logged in shows balance of 0 for new wallet", async ({
    page,
  }) => {
    await loginEphemeral(page);

    await page.goto("/#/wallet");

    await expect(
      page.getByRole("heading", { name: "Wallet" }),
    ).toBeVisible({ timeout: 20_000 });

    await expect(page.getByText("Test sats").first()).toBeVisible();
    await expect(page.getByText("0", { exact: true })).toBeVisible();
  });

  test("[HAPPY] shows testnut.cashu.space mint URL", async ({ page }) => {
    await loginEphemeral(page);

    await page.goto("/#/wallet");

    await expect(
      page.getByRole("heading", { name: "Wallet" }),
    ).toBeVisible({ timeout: 20_000 });

    await expect(page.getByText("testnut.cashu.space").first()).toBeVisible();
  });

  test("[HAPPY] top-up section renders with generate invoice button", async ({
    page,
  }) => {
    await loginEphemeral(page);

    await page.goto("/#/wallet");

    await expect(
      page.getByRole("heading", { name: "Wallet" }),
    ).toBeVisible({ timeout: 20_000 });

    await expect(page.getByText("Top Up").first()).toBeVisible();
    await expect(
      page.getByRole("button", { name: "Generate Invoice" }),
    ).toBeVisible();
  });

  test("[HAPPY] send tokens section renders", async ({ page }) => {
    await loginEphemeral(page);

    await page.goto("/#/wallet");

    await expect(
      page.getByRole("heading", { name: "Wallet" }),
    ).toBeVisible({ timeout: 20_000 });

    await expect(page.getByText("Send Tokens").first()).toBeVisible();
    await expect(
      page.getByRole("button", { name: "Send", exact: true }),
    ).toBeVisible();
  });

  test("[HAPPY] receive tokens section renders", async ({ page }) => {
    await loginEphemeral(page);

    await page.goto("/#/wallet");

    await expect(
      page.getByRole("heading", { name: "Wallet" }),
    ).toBeVisible({ timeout: 20_000 });

    await expect(page.getByText("Receive Tokens")).toBeVisible();
    await expect(
      page.getByPlaceholder("Paste a Cashu token (cashuA...)"),
    ).toBeVisible();
  });

  test("[HAPPY] NUT-18 payment request section renders", async ({ page }) => {
    await loginEphemeral(page);

    await page.goto("/#/wallet");

    await expect(
      page.getByRole("heading", { name: "Wallet" }),
    ).toBeVisible({ timeout: 20_000 });

    await expect(page.getByText("Payment Request").first()).toBeVisible();
    await expect(page.getByPlaceholder("Sats")).toBeVisible();
    await expect(
      page.getByPlaceholder("Description (optional)"),
    ).toBeVisible();
    await expect(
      page.getByRole("button", { name: "Create Payment Request" }),
    ).toBeVisible();
  });
});
