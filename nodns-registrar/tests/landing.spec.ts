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
      !e.includes("relay"),
  );
  expect(
    significant,
    `Unexpected console errors:\n${significant.join("\n")}`,
  ).toEqual([]);
});

test.describe("Landing page", () => {
  test("[HAPPY] page loads with hero heading", async ({ page }) => {
    await expect(
      page.getByRole("heading", { name: /find your.*nodns.*name/i }),
    ).toBeVisible();
  });

  test("[HAPPY] beta banner shows EXPERIMENTAL PILOT", async ({ page }) => {
    await expect(page.getByText("EXPERIMENTAL PILOT")).toBeVisible();
  });

  test("[HAPPY] search for available name shows Available with price", async ({
    page,
  }) => {
    const uniqueName = `e2e${Date.now().toString(36)}`;

    await page.getByPlaceholder("alice").fill(uniqueName);
    await page.getByRole("button", { name: "Search" }).click();

    await page.waitForLoadState("networkidle");

    await expect(page.getByText("Available!")).toBeVisible({ timeout: 15_000 });
    await expect(page.getByText(`${uniqueName}.nodns.shop`)).toBeVisible();
  });

  test("[HAPPY] search for a taken name shows Taken", async ({ page }) => {
    test.skip(true, "No custom names are currently registered on nodns.shop");
  });

  test("[FAIL] empty search input keeps Search button disabled", async ({
    page,
  }) => {
    const searchButton = page.getByRole("button", { name: "Search" });
    await expect(searchButton).toBeDisabled();
  });

  test("[FAIL] not logged in shows Sign in to register button", async ({
    page,
  }) => {
    const uniqueName = `e2e${Date.now().toString(36)}`;

    await page.getByPlaceholder("alice").fill(uniqueName);
    await page.getByRole("button", { name: "Search" }).click();

    await page.waitForLoadState("networkidle");

    await expect(
      page.getByRole("button", { name: "Sign in to register" }),
    ).toBeVisible({ timeout: 15_000 });
  });

  test("[EDGE] search with special characters does not crash", async ({
    page,
  }) => {
    await page.getByPlaceholder("alice").fill("test!@#$%");
    await page.getByRole("button", { name: "Search" }).click();

    await page.waitForLoadState("networkidle");

    await expect(
      page.getByText(/only lowercase letters/i),
    ).toBeVisible({ timeout: 10_000 });
  });
});
