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

test.describe("Landing page detailed flows", () => {
  test("[HAPPY] search for short name shows higher price", async ({
    page,
  }) => {
    const shortName = `e2e${Date.now().toString(36)}`.slice(0, 3);

    await page.getByPlaceholder("alice").fill(shortName);
    await page.getByRole("button", { name: "Search" }).click();
    await page.waitForLoadState("networkidle");

    await expect(page.getByText("Available!")).toBeVisible({
      timeout: 15_000,
    });
    await expect(page.getByText("2 sats")).toBeVisible();
  });

  test("[HAPPY] search for long name shows lower or free price", async ({
    page,
  }) => {
    const longName = `e2elongname${Date.now().toString(36)}`;

    await page.getByPlaceholder("alice").fill(longName);
    await page.getByRole("button", { name: "Search" }).click();
    await page.waitForLoadState("networkidle");

    await expect(page.getByText("Available!")).toBeVisible({
      timeout: 15_000,
    });
    await expect(
      page.getByText("2 sats"),
    ).toBeVisible();
  });

  test("[FAIL] search with uppercase normalizes to lowercase", async ({
    page,
  }) => {
    const base = `e2e${Date.now().toString(36)}`;
    const upperName = base.toUpperCase();

    await page.getByPlaceholder("alice").fill(upperName);
    await page.getByRole("button", { name: "Search" }).click();
    await page.waitForLoadState("networkidle");

    await expect(page.getByText("Available!")).toBeVisible({
      timeout: 15_000,
    });
    await expect(
      page.getByText(`${base}.nodns.shop`),
    ).toBeVisible();
  });

  test("[FAIL] search with trailing .nodns.shop strips the suffix", async ({
    page,
  }) => {
    const uniqueName = `e2estrip${Date.now().toString(36)}`;

    await page.getByPlaceholder("alice").fill(`${uniqueName}.nodns.shop`);
    await page.getByRole("button", { name: "Search" }).click();
    await page.waitForLoadState("networkidle");

    await expect(page.getByText("Available!")).toBeVisible({
      timeout: 15_000,
    });
    await expect(
      page.getByText(`${uniqueName}.nodns.shop`),
    ).toBeVisible();
  });

  test("[EDGE] search with very long name (63+ chars) shows validation error", async ({
    page,
  }) => {
    const longName = "a".repeat(64);

    await page.getByPlaceholder("alice").fill(longName);
    await page.getByRole("button", { name: "Search" }).click();

    await expect(
      page.getByText(/too long/i),
    ).toBeVisible({ timeout: 10_000 });
  });

  test("[HAPPY] features section renders 3 cards with titles", async ({
    page,
  }) => {
    await expect(page.getByText("Nostr-native identity")).toBeVisible();
    await expect(page.getByText("Cashu payments")).toBeVisible();
    await expect(page.getByText("DNSSEC-signed").first()).toBeVisible();
  });

  test("[HAPPY] how-it-works section renders 3 steps", async ({ page }) => {
    await expect(page.getByText("Publish event")).toBeVisible();
    await expect(page.getByText("Bot validates")).toBeVisible();
    await expect(page.getByText("DNS live")).toBeVisible();
  });

  test("[HAPPY] clicking Top up wallet link navigates to wallet when insufficient balance", async ({
    page,
  }) => {
    await loginEphemeral(page);

    const shortName = `e2e${Date.now().toString(36)}`.slice(0, 3);

    await page.getByPlaceholder("alice").fill(shortName);
    await page.getByRole("button", { name: "Search" }).click();
    await page.waitForLoadState("networkidle");

    await expect(page.getByText("Available!")).toBeVisible({
      timeout: 15_000,
    });

    await expect(
      page.getByRole("link", { name: "Top up wallet" }),
    ).toBeVisible({ timeout: 10_000 });

    await page.getByRole("link", { name: "Top up wallet" }).click();

    await expect(
      page.getByRole("heading", { name: "Wallet" }),
    ).toBeVisible({ timeout: 20_000 });
  });
});
