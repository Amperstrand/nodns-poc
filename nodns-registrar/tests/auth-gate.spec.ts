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

test.describe("Authentication gates", () => {
  test("[HAPPY] dashboard without login shows sign-in prompt with Generate Identity", async ({
    page,
  }) => {
    await page.goto("/#/dashboard");
    await page.waitForLoadState("networkidle");

    await expect(
      page.getByText("Sign in to view your dashboard"),
    ).toBeVisible();
    await expect(
      page.getByRole("button", { name: "Generate Identity" }),
    ).toBeVisible();
  });

  test("[HAPPY] clicking Generate Identity on dashboard logs in and shows dashboard content", async ({
    page,
  }) => {
    await page.goto("/#/dashboard");
    await page.waitForLoadState("networkidle");

    await page.getByRole("button", { name: "Generate Identity" }).click();

    await expect(
      page.getByRole("heading", { name: "Dashboard" }),
    ).toBeVisible({ timeout: 15_000 });
  });

  test("[HAPPY] wallet page renders without login (no gate)", async ({
    page,
  }) => {
    await page.goto("/#/wallet");

    await expect(
      page.getByRole("heading", { name: "Wallet" }),
    ).toBeVisible({ timeout: 20_000 });
  });

  test("[HAPPY] landing page search works without login", async ({ page }) => {
    await expect(page.getByPlaceholder("alice")).toBeVisible();
    await expect(
      page.getByRole("button", { name: "Search" }),
    ).toBeDisabled();

    await page.getByPlaceholder("alice").fill("testsearch");
    await expect(
      page.getByRole("button", { name: "Search" }),
    ).toBeEnabled();
  });

  test("[HAPPY] after login, Dashboard and Wallet links appear in header", async ({
    page,
  }) => {
    await expect(
      page.getByRole("link", { name: "Dashboard" }),
    ).not.toBeVisible();

    await loginEphemeral(page);

    await expect(
      page.getByRole("link", { name: "Dashboard" }),
    ).toBeVisible();
    await expect(
      page.getByRole("link", { name: "Wallet" }),
    ).toBeVisible();
  });
});
