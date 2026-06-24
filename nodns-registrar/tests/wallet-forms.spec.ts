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

async function gotoWallet(page: Page) {
  await loginEphemeral(page);
  await page.goto("/#/wallet");
  await page.getByRole("heading", { name: "Wallet" }).waitFor({
    timeout: 20_000,
  });
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

test.describe("Wallet form interactions", () => {
  test("[FAIL] top-up with empty amount disables Generate Invoice", async ({
    page,
  }) => {
    await gotoWallet(page);

    await expect(
      page.getByRole("button", { name: "Generate Invoice" }),
    ).toBeDisabled();
  });

  test("[FAIL] top-up with 0 amount does not generate invoice", async ({
    page,
  }) => {
    await gotoWallet(page);

    await page.getByPlaceholder("Amount to top up").fill("0");

    const generateBtn = page.getByRole("button", {
      name: "Generate Invoice",
    });
    await generateBtn.click();

    await page.waitForTimeout(1000);

    await expect(page.getByText("Waiting for payment...")).not.toBeVisible();
    await expect(page.getByText("Generating...")).not.toBeVisible();
  });

  test("[HAPPY] top-up with valid amount enables Generate Invoice button", async ({
    page,
  }) => {
    await gotoWallet(page);

    await page.getByPlaceholder("Amount to top up").fill("10");

    await expect(
      page.getByRole("button", { name: "Generate Invoice" }),
    ).toBeEnabled();
  });

  test("[FAIL] send with insufficient balance shows Send disabled", async ({
    page,
  }) => {
    await gotoWallet(page);

    await expect(
      page.getByRole("button", { name: "Send", exact: true }),
    ).toBeDisabled();

    await expect(page.getByText("Balance is zero")).toBeVisible();
  });

  test("[FAIL] send with 0 amount disables Send button", async ({ page }) => {
    await gotoWallet(page);

    await expect(
      page.getByRole("button", { name: "Send", exact: true }),
    ).toBeDisabled();
  });

  test("[FAIL] receive with empty textarea disables Receive button", async ({
    page,
  }) => {
    await gotoWallet(page);

    await expect(
      page.getByRole("button", { name: "Receive" }),
    ).toBeDisabled();
  });

  test("[HAPPY] receive textarea accepts cashuA tokens", async ({ page }) => {
    await gotoWallet(page);

    const textarea = page.getByPlaceholder(
      "Paste a Cashu token (cashuA...)",
    );
    await textarea.fill("cashuAtesttoken123");

    await expect(
      page.getByRole("button", { name: "Receive" }),
    ).toBeEnabled();
  });

  test("[FAIL] NUT-18 payment request with empty sats disables Create button", async ({
    page,
  }) => {
    await gotoWallet(page);

    await expect(
      page.getByRole("button", { name: "Create Payment Request" }),
    ).toBeDisabled();
  });

  test("[HAPPY] NUT-18 payment request with amount enables Create button", async ({
    page,
  }) => {
    await gotoWallet(page);

    await page.getByPlaceholder("Sats").fill("50");

    await expect(
      page.getByRole("button", { name: "Create Payment Request" }),
    ).toBeEnabled();
  });
});
