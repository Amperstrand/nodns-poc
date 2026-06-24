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
      !e.includes("ERR_INTERNET_DISCONNECTED") &&
      !e.includes("ERR_NAME_NOT_RESOLVED") &&
      !e.includes("cashu") &&
      !e.includes("Cashu") &&
      !e.includes("indexedDB") &&
      !e.includes("IDB") &&
      !e.includes("unhandledrejection") &&
      !e.includes("wallet") &&
      !e.includes("mint") &&
      !e.includes("relay") &&
      !e.includes("404") &&
      !e.includes("Failed to load resource"),
  );
  expect(
    significant,
    `Unexpected console errors:\n${significant.join("\n")}`,
  ).toEqual([]);
});

test.describe("Zone discovery", () => {
  test("[HAPPY] testing status banner appears on landing page", async ({
    page,
  }) => {
    await expect(
      page.getByText(/TESTING MODE/),
    ).toBeVisible({ timeout: 20_000 });
  });

  test("[HAPPY] testnet badge appears in header when zone is testnet", async ({
    page,
  }) => {
    await expect(
      page.getByRole("link", { name: /NoDNS/ }).getByText("testnet"),
    ).toBeVisible({ timeout: 20_000 });
  });

  test("[HAPPY] landing page shows discovered zone in hero heading", async ({
    page,
  }) => {
    await expect(
      page.getByRole("heading", { name: /find your/i }),
    ).toBeVisible();
  });

  test("[EDGE] search form remains functional with zone discovery active", async ({
    page,
  }) => {
    await expect(
      page.getByText(/TESTING MODE/),
    ).toBeVisible({ timeout: 20_000 });

    const searchInput = page.getByPlaceholder("alice");
    await expect(searchInput).toBeVisible();

    const searchButton = page.getByRole("button", { name: "Search" });
    await expect(searchButton).toBeDisabled();

    await searchInput.fill("testname");
    await expect(searchButton).toBeEnabled();
  });
});
