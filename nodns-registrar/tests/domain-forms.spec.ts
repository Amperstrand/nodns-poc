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

async function gotoDomain(page: Page) {
  await loginEphemeral(page);
  await page.goto("/#/domain?zone=nodns.shop");
  await page.waitForLoadState("networkidle");
  await page.getByText("Add Record").first().waitFor({ timeout: 15_000 });
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
      !e.includes("relay") &&
      !e.includes("blocked: not on white-list") &&
      !e.includes("not on white-list"),
  );
  expect(
    significant,
    `Unexpected console errors:\n${significant.join("\n")}`,
  ).toEqual([]);
});

test.describe("Domain record form interactions", () => {
  test("[HAPPY] default record type is A", async ({ page }) => {
    await gotoDomain(page);

    await expect(page.getByRole("combobox")).toHaveValue("A");
  });

  test("[HAPPY] changing type to TXT changes the value input placeholder", async ({
    page,
  }) => {
    await gotoDomain(page);

    await page.getByRole("combobox").selectOption("TXT");

    await expect(
      page.getByPlaceholder("v=spf1 include:_spf.example.com ~all"),
    ).toBeVisible();
  });

  test("[HAPPY] changing type to MX shows priority + mail exchange format", async ({
    page,
  }) => {
    await gotoDomain(page);

    await page.getByRole("combobox").selectOption("MX");

    await expect(
      page.getByPlaceholder("10 mail.example.com."),
    ).toBeVisible();
  });

  test("[FAIL] A record with private IP (10.x) shows validation error", async ({
    page,
  }) => {
    await gotoDomain(page);

    await page.getByPlaceholder("203.0.113.1").fill("10.0.0.1");
    await page.getByRole("button", { name: /Add Record/ }).first().click();

    await expect(page.getByText("Private IP")).toBeVisible({
      timeout: 5_000,
    });
  });

  test("[FAIL] AAAA record with private IP shows validation error", async ({
    page,
  }) => {
    await gotoDomain(page);

    await page.getByRole("combobox").selectOption("AAAA");
    await page.getByPlaceholder("2001:db8::1").fill("fd00::1");
    await page.getByRole("button", { name: /Add Record/ }).first().click();

    await expect(page.getByText("Private IPv6")).toBeVisible({
      timeout: 5_000,
    });
  });

  test("[FAIL] CNAME record with value too long shows validation error", async ({
    page,
  }) => {
    await gotoDomain(page);

    await page.getByRole("combobox").selectOption("CNAME");
    const longHost = "a".repeat(254) + ".com.";
    await page.getByPlaceholder("example.com.").fill(longHost);
    await page.getByRole("button", { name: /Add Record/ }).first().click();

    await expect(page.getByText("CNAME value too long")).toBeVisible({
      timeout: 5_000,
    });
  });

  test("[FAIL] TXT record over 512 chars shows validation error", async ({
    page,
  }) => {
    await gotoDomain(page);

    await page.getByRole("combobox").selectOption("TXT");
    const longText = "a".repeat(513);
    await page
      .getByPlaceholder("v=spf1 include:_spf.example.com ~all")
      .fill(longText);
    await page.getByRole("button", { name: /Add Record/ }).first().click();

    await expect(
      page.getByText("TXT record exceeds 512 characters"),
    ).toBeVisible();
  });

  test("[HAPPY] TTL selector renders and is clickable", async ({ page }) => {
    await gotoDomain(page);

    const ttlButton = page
      .getByRole("button", { name: /Auto.*3600|Auto/ })
      .first();
    await expect(ttlButton).toBeVisible();
    await ttlButton.click();

    await expect(page.getByText("5 min").first()).toBeVisible();
    await expect(page.getByText("30 min").first()).toBeVisible();
  });

  test("[EDGE] empty value field disables Add Record button", async ({
    page,
  }) => {
    await gotoDomain(page);

    const addBtn = page.getByRole("button", { name: /Add Record/ }).first();
    await expect(addBtn).toBeDisabled();
  });
});
