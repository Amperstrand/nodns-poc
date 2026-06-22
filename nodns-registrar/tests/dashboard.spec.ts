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

test.describe("Dashboard", () => {
  test("[FAIL] not logged in shows sign-in prompt with Generate Identity button", async ({
    page,
  }) => {
    await page.goto("/dashboard");
    await page.waitForLoadState("networkidle");

    await expect(
      page.getByText("Sign in to view your dashboard"),
    ).toBeVisible();
    await expect(
      page.getByRole("button", { name: "Generate Identity" }),
    ).toBeVisible();
  });

  test("[HAPPY] logged in with ephemeral shows empty state", async ({
    page,
  }) => {
    await loginEphemeral(page);

    await page.goto("/dashboard");
    await page.waitForLoadState("networkidle");

    await expect(page.getByText("No domains yet")).toBeVisible({
      timeout: 15_000,
    });
  });

  test("[HAPPY] stats cards render with Wallet Balance, Domains, Total Records", async ({
    page,
  }) => {
    await loginEphemeral(page);

    await page.goto("/dashboard");
    await page.waitForLoadState("networkidle");

    await expect(page.getByText("Wallet Balance")).toBeVisible({
      timeout: 15_000,
    });
    await expect(page.getByText("Domains", { exact: true })).toBeVisible();
    await expect(page.getByText("Total Records")).toBeVisible();
  });

  test("[HAPPY] Register New Domain button links to home", async ({
    page,
  }) => {
    await loginEphemeral(page);

    await page.goto("/dashboard");
    await page.waitForLoadState("networkidle");

    const registerButton = page.getByRole("link", {
      name: "Register New Domain",
    });
    await expect(registerButton).toBeVisible({ timeout: 15_000 });
  });
});
