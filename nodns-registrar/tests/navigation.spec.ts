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

test.describe("Navigation", () => {
  test("[HAPPY] clicking NoDNS logo navigates to landing", async ({ page }) => {
    await page.goto("/#/wallet");

    await page.getByRole("link", { name: /NoDNS/ }).first().click();

    await expect(
      page.getByRole("heading", { name: /find your.*nodns.*name/i }),
    ).toBeVisible();
  });

  test("[HAPPY] clicking Dashboard link in header navigates to dashboard", async ({
    page,
  }) => {
    await loginEphemeral(page);

    await page.getByRole("link", { name: "Dashboard" }).first().click();

    await expect(
      page.getByRole("heading", { name: "Dashboard" }),
    ).toBeVisible({ timeout: 15_000 });
  });

  test("[HAPPY] clicking Wallet link in header navigates to wallet", async ({
    page,
  }) => {
    await loginEphemeral(page);

    await page.getByRole("link", { name: "Wallet" }).first().click();

    await expect(
      page.getByRole("heading", { name: "Wallet" }),
    ).toBeVisible({ timeout: 20_000 });
  });

  test("[HAPPY] logged-out header only shows Sign In", async ({ page }) => {
    await expect(
      page.getByRole("button", { name: "Sign In" }),
    ).toBeVisible();
    await expect(
      page.getByRole("link", { name: "Dashboard" }),
    ).not.toBeVisible();
    await expect(
      page.getByRole("link", { name: "Wallet" }),
    ).not.toBeVisible();
  });

  test("[HAPPY] logged-in header shows Dashboard, Wallet, npub, balance, Logout", async ({
    page,
  }) => {
    await loginEphemeral(page);

    await expect(
      page.getByRole("link", { name: "Dashboard" }),
    ).toBeVisible();
    await expect(page.getByRole("link", { name: "Wallet" })).toBeVisible();
    await expect(
      page.getByRole("button", { name: "Logout" }),
    ).toBeVisible();

    const npubCode = page.locator("code").filter({ hasText: /npub1/ });
    await expect(npubCode).toBeVisible();
  });

  test("[HAPPY] direct URL navigation to /#/dashboard works", async ({
    page,
  }) => {
    await page.goto("/#/dashboard");
    await page.waitForLoadState("networkidle");

    await expect(
      page.getByText("Sign in to view your dashboard"),
    ).toBeVisible();
  });

  test("[HAPPY] direct URL navigation to /#/wallet works", async ({ page }) => {
    await page.goto("/#/wallet");

    await expect(
      page.getByRole("heading", { name: "Wallet" }),
    ).toBeVisible({ timeout: 20_000 });
  });

  test("[HAPPY] direct URL navigation to /#/domain with query params works", async ({
    page,
  }) => {
    await loginEphemeral(page);

    await page.goto("/#/domain?name=testnav&zone=nodns.shop");
    await page.waitForLoadState("networkidle");

    await expect(page.getByText("Add Record").first()).toBeVisible({
      timeout: 15_000,
    });
  });

  test("[EDGE] unknown hash route falls back to landing", async ({ page }) => {
    await page.goto("/#/this-route-does-not-exist");
    await page.waitForLoadState("networkidle");

    await expect(
      page.getByRole("heading", { name: /find your.*nodns.*name/i }),
    ).toBeVisible();
  });

  test("[HAPPY] logout from dashboard returns to signed-out state", async ({
    page,
  }) => {
    await loginEphemeral(page);

    await page.goto("/#/dashboard");
    await page.waitForLoadState("networkidle");

    await expect(
      page.getByRole("heading", { name: "Dashboard" }),
    ).toBeVisible({ timeout: 15_000 });

    await page.getByRole("button", { name: "Logout" }).click();

    await expect(
      page.getByRole("button", { name: "Sign In" }),
    ).toBeVisible();
    await expect(
      page.getByRole("link", { name: "Dashboard" }),
    ).not.toBeVisible();
  });
});
