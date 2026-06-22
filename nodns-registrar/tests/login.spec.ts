import { test, expect, type Page } from "@playwright/test";

const consoleErrors: string[] = [];

test.beforeEach(async ({ page }) => {
  consoleErrors.length = 0;

  await page.goto("/");
  await page.evaluate(() => {
    localStorage.clear();
    sessionStorage.clear();
  });
  await page.reload();
  await page.waitForLoadState("networkidle");

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

test.describe("Login flow", () => {
  test("[HAPPY] click Sign In opens login modal", async ({ page }) => {
    await page.getByRole("button", { name: "Sign In" }).click();

    await expect(
      page.getByRole("heading", { name: "Sign in to NoDNS" }),
    ).toBeVisible();
  });

  test("[HAPPY] ephemeral key login creates session", async ({ page }) => {
    await page.getByRole("button", { name: "Sign In" }).click();

    await page
      .getByRole("button", { name: "Try with ephemeral key" })
      .click();

    await expect(
      page.getByRole("link", { name: "Dashboard" }),
    ).toBeVisible();
    await expect(page.getByRole("link", { name: "Wallet" })).toBeVisible();
    await expect(
      page.getByRole("button", { name: "Logout" }),
    ).toBeVisible();
  });

  test("[HAPPY] after login npub is shown in header", async ({ page }) => {
    await page.getByRole("button", { name: "Sign In" }).click();
    await page
      .getByRole("button", { name: "Try with ephemeral key" })
      .click();

    await expect(page.getByRole("link", { name: "Dashboard" })).toBeVisible();

    const npubCode = page.locator("code").filter({ hasText: /npub1/ });
    await expect(npubCode).toBeVisible();
  });

  test("[HAPPY] logout clears session and restores Sign In button", async ({
    page,
  }) => {
    await page.getByRole("button", { name: "Sign In" }).click();
    await page
      .getByRole("button", { name: "Try with ephemeral key" })
      .click();

    await expect(page.getByRole("link", { name: "Dashboard" })).toBeVisible();

    await page.getByRole("button", { name: "Logout" }).click();

    await expect(
      page.getByRole("button", { name: "Sign In" }),
    ).toBeVisible();
    await expect(
      page.getByRole("link", { name: "Dashboard" }),
    ).not.toBeVisible();
  });

  test("[FAIL] invalid nsec shows error message", async ({ page }) => {
    await page.getByRole("button", { name: "Sign In" }).click();

    await page.getByPlaceholder("nsec1...").fill("invalid_key");
    await page.getByRole("button", { name: "Sign In with nsec" }).click();

    await expect(
      page.getByText("Invalid nsec: must start with 'nsec1'"),
    ).toBeVisible();
  });

  test("[EDGE] extension not detected shows install message", async ({
    page,
  }) => {
    await page.getByRole("button", { name: "Sign In" }).click();

    await expect(page.getByText("No extension detected")).toBeVisible();
    await expect(
      page.getByRole("link", { name: "Alby" }),
    ).toBeVisible();
    await expect(
      page.getByRole("link", { name: "nos2x" }),
    ).toBeVisible();
  });
});
