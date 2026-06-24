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

test.describe("State persistence", () => {
  test("[HAPPY] ephemeral identity persists across page reload", async ({
    page,
  }) => {
    await loginEphemeral(page);

    const npubBefore = await page
      .locator("code")
      .filter({ hasText: /npub1/ })
      .textContent();

    await page.reload();
    await page.waitForLoadState("networkidle");

    await expect(
      page.getByRole("link", { name: "Dashboard" }),
    ).toBeVisible({ timeout: 15_000 });

    const npubAfter = await page
      .locator("code")
      .filter({ hasText: /npub1/ })
      .textContent();

    expect(npubBefore).toBeTruthy();
    expect(npubAfter).toBeTruthy();
    expect(npubBefore).toBe(npubAfter);
  });

  test("[HAPPY] ephemeral identity persists when navigating to different route", async ({
    page,
  }) => {
    await loginEphemeral(page);

    const npubBefore = await page
      .locator("code")
      .filter({ hasText: /npub1/ })
      .textContent();

    await page.goto("/#/wallet");
    await page.waitForLoadState("networkidle");

    await expect(
      page.getByRole("link", { name: "Dashboard" }),
    ).toBeVisible();

    const npubAfter = await page
      .locator("code")
      .filter({ hasText: /npub1/ })
      .textContent();

    expect(npubBefore).toBeTruthy();
    expect(npubAfter).toBeTruthy();
    expect(npubBefore).toBe(npubAfter);
  });

  test("[EDGE] clearing localStorage logs out user on next page load", async ({
    page,
  }) => {
    await loginEphemeral(page);

    await expect(
      page.getByRole("link", { name: "Dashboard" }),
    ).toBeVisible();

    await clearStorageAndReload(page);

    await expect(
      page.getByRole("button", { name: "Sign In" }),
    ).toBeVisible();
    await expect(
      page.getByRole("link", { name: "Dashboard" }),
    ).not.toBeVisible();
  });
});
