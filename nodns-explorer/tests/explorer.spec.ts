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
      !e.includes("ERR_") &&
      !e.includes("cashu") &&
      !e.includes("Cashu") &&
      !e.includes("indexedDB") &&
      !e.includes("IDB") &&
      !e.includes("unhandledrejection") &&
      !e.includes("wallet") &&
      !e.includes("mint") &&
      !e.includes("relay") &&
      !e.includes("AbortError") &&
      !e.includes("dns") &&
      !e.includes("DOH"),
  );
  expect(
    significant,
    `Unexpected console errors:\n${significant.join("\n")}`,
  ).toEqual([]);
});

test.describe("Explorer", () => {
  test("[HAPPY] page loads with header nodns explorer", async ({ page }) => {
    await expect(page.getByText("nodns explorer")).toBeVisible();
  });

  test("[HAPPY] TESTNET badge visible in header", async ({ page }) => {
    await expect(
      page.getByRole("banner").getByText("TESTNET", { exact: true }),
    ).toBeVisible();
  });

  test("[HAPPY] zone card shows nodns.shop with verified badge", async ({
    page,
  }) => {
    await expect(page.getByText("nodns.shop").first()).toBeVisible({
      timeout: 20_000,
    });
    await expect(page.getByText(/verified/).first()).toBeVisible({
      timeout: 20_000,
    });
  });

  test("[HAPPY] filter bar has npub input + type/kind/payment/validity dropdowns", async ({
    page,
  }) => {
    await expect(
      page.getByPlaceholder("filter by npub or pubkey..."),
    ).toBeVisible();

    await expect(page.getByRole("combobox")).toHaveCount(4);
  });

  test("[HAPPY] clicking Zone Monitor tab switches view", async ({ page }) => {
    await page.getByRole("button", { name: "Zone Monitor" }).click();

    await expect(
      page.getByRole("heading", { name: "Zone Monitor" }),
    ).toBeVisible();
  });

  test("[HAPPY] zone monitor shows source cards (Relay / Bot API / Live DNS)", async ({
    page,
  }) => {
    await page.getByRole("button", { name: "Zone Monitor" }).click();

    await expect(
      page.getByRole("heading", { name: "Zone Monitor" }),
    ).toBeVisible();

    await expect(page.getByText("Relay", { exact: true })).toBeVisible();
    await expect(page.getByText("Bot API", { exact: true })).toBeVisible();
    await expect(page.getByText("Live DNS", { exact: true })).toBeVisible();
  });

  test("[HAPPY] event feed shows events after relay subscription", async ({
    page,
  }) => {
    await expect(
      page
        .getByText("RECORD", { exact: true })
        .or(page.getByText("ZONE", { exact: true }))
        .first(),
    ).toBeVisible({ timeout: 20_000 });
  });

  test("[HAPPY] event rows have expandable details (click to expand)", async ({
    page,
  }) => {
    const recordBadge = page
      .getByText("RECORD", { exact: true })
      .or(page.getByText("ZONE", { exact: true }))
      .first();

    await expect(recordBadge).toBeVisible({ timeout: 20_000 });
    await recordBadge.click();

    await expect(
      page.getByText("Tags", { exact: true }),
    ).toBeVisible({ timeout: 10_000 });
  });
});
