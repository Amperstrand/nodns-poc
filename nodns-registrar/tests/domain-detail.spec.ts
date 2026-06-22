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
      !e.includes("blocked: not on white-list") &&
      !e.includes("not on white-list") &&
      !e.includes("relay"),
  );
  expect(
    significant,
    `Unexpected console errors:\n${significant.join("\n")}`,
  ).toEqual([]);
});

test.describe("Domain detail page", () => {
  test("[HAPPY] navigate to domain page and it loads with fqdn heading", async ({
    page,
  }) => {
    await loginEphemeral(page);

    await page.goto("/domain?name=test&zone=nodns.shop");
    await page.waitForLoadState("networkidle");

    await expect(
      page.getByRole("heading", { name: "test.nodns.shop" }),
    ).toBeVisible({ timeout: 15_000 });
  });

  test("[HAPPY] add record form renders with type selector", async ({
    page,
  }) => {
    await loginEphemeral(page);

    await page.goto("/domain?name=test&zone=nodns.shop");
    await page.waitForLoadState("networkidle");

    await expect(page.getByText("Add Record").first()).toBeVisible({
      timeout: 15_000,
    });
    await expect(page.getByRole("combobox")).toBeVisible();
    await expect(page.getByRole("combobox")).toHaveValue("A");
  });

  test.fixme(
    "[FAIL] invalid IP in A record shows validation error",
    async ({ page }) => {
      await loginEphemeral(page);

      await page.goto("/domain?name=test&zone=nodns.shop");
      await page.waitForLoadState("networkidle");

      await expect(page.getByText("Add Record").first()).toBeVisible({
        timeout: 15_000,
      });

      await page.getByPlaceholder("203.0.113.1").fill("10.0.0.1");
      await page.getByRole("button", { name: /Add Record/ }).first().click();

      await expect(page.getByText("Private IP")).toBeVisible({
        timeout: 5_000,
      });
    },
  );

  test.fixme(
    "[EDGE] TXT record over 512 chars shows validation error",
    async ({ page }) => {
      await loginEphemeral(page);

      await page.goto("/domain?name=test&zone=nodns.shop");
      await page.waitForLoadState("networkidle");

      await expect(page.getByText("Add Record").first()).toBeVisible({
        timeout: 15_000,
      });

      await page.getByRole("combobox").selectOption("TXT");

      const longText = "a".repeat(513);
      await page
        .getByPlaceholder("v=spf1 include:_spf.example.com ~all")
        .fill(longText);
      await page.getByRole("button", { name: /Add Record/ }).first().click();

      await expect(
        page.getByText("TXT record exceeds 512 characters"),
      ).toBeVisible();
    },
  );
});
