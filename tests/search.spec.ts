import { test, expect } from "@playwright/test";

test.describe("Search - Available Domain", () => {
  test("shows Available badge and pricing for unregistered domain", async ({
    page,
  }) => {
    const name = `e2eavail${Date.now().toString(36)}`;
    await page.goto(`./search?q=${name}`);
    await expect(page.getByText("Available!")).toBeVisible({ timeout: 10_000 });
    await expect(page.getByText(/sats\/year/)).toBeVisible();
    await expect(
      page.getByRole("link", { name: /Register this domain/i })
    ).toBeVisible();
  });

  test("shows correct pricing tier for 7+ char name (4 sats)", async ({
    page,
  }) => {
    await page.goto("./search?q=verylongname");
    await expect(page.getByText("Available!")).toBeVisible({ timeout: 10_000 });
    const price = page.locator(".text-3xl");
    await expect(price).toContainText("4");
  });

  test("shows correct pricing tier for 4-6 char name (20 sats)", async ({
    page,
  }) => {
    await page.goto("./search?q=test");
    await expect(page.getByText("Available!")).toBeVisible({ timeout: 10_000 });
    const price = page.locator(".text-3xl");
    await expect(price).toContainText("20");
  });

  test("shows correct pricing tier for 1-3 char name (200 sats)", async ({
    page,
  }) => {
    await page.goto("./search?q=ab");
    await expect(page.getByText("Available!")).toBeVisible({ timeout: 10_000 });
    const price = page.locator(".text-3xl");
    await expect(price).toContainText("200");
  });
});

test.describe("Search - Edge Cases", () => {
  test("shows prompt when no query param", async ({ page }) => {
    await page.goto("./search");
    await page.waitForLoadState("domcontentloaded");
    await expect(
      page.getByRole("heading", { name: /domain/i })
    ).toBeVisible({ timeout: 15_000 });
  });

  test("strips .nodns.shop suffix from FQDN input", async ({ page }) => {
    await page.goto("./search?q=mytest.nodns.shop");
    await expect(page.getByText("Available!")).toBeVisible({ timeout: 10_000 });
    await expect(page.locator("h1").filter({ hasText: "mytest" })).toBeVisible();
    await expect(
      page.locator("h1").filter({ hasText: "nodns.shop.nodns" })
    ).not.toBeVisible();
  });

  test("sanitizes special characters keeping only valid DNS chars", async ({
    page,
  }) => {
    await page.goto("./search?q=test!@#$");
    await expect(page.getByText("Available!")).toBeVisible({ timeout: 10_000 });
    await expect(page.locator("h1").filter({ hasText: "test" })).toBeVisible();
  });

  test("long domain name does not break layout", async ({ page }) => {
    const longName = "a".repeat(63);
    await page.goto(`./search?q=${longName}`);
    await expect(page.getByText("Available!")).toBeVisible({ timeout: 10_000 });
    const heading = page.locator("h1.break-all");
    await expect(heading).toBeVisible();
  });
});

test.describe("Search - Source Verification", () => {
  test("shows source bar with API, Nostr, DNS counts for unregistered domain", async ({
    page,
  }) => {
    const name = `e2esrc${Date.now().toString(36)}`;
    await page.goto(`./search?q=${name}`);
    await expect(page.getByText("Available!")).toBeVisible({ timeout: 10_000 });
    await expect(page.getByText("Sources", { exact: true })).toBeVisible({ timeout: 5_000 });
    await expect(page.getByText(/Sources agree/i)).toBeVisible();
  });

  test("DNS source does not show false positive wildcard A records", async ({
    page,
  }) => {
    const name = `e2dnswild${Date.now().toString(36)}`;
    await page.goto(`./search?q=${name}`);
    await expect(page.getByText("Available!")).toBeVisible({ timeout: 10_000 });
    await page.waitForTimeout(2_000);
    const dnsSection = page.locator("text=🌐").locator("..");
    const dnsText = await dnsSection.textContent();
    expect(dnsText).not.toContain("1 record");
  });
});

test.describe("Search - Register Link", () => {
  test("register link contains correct name param", async ({ page }) => {
    await page.goto("./search?q=myregtest");
    await expect(page.getByText("Available!")).toBeVisible({ timeout: 10_000 });
    const registerLink = page.getByRole("link", {
      name: /Register this domain/i,
    });
    const href = await registerLink.getAttribute("href");
    expect(href).toContain("name=myregtest");
  });
});

test.describe("Search - Pricing Tiers Display", () => {
  test("shows all three pricing tier cards", async ({ page }) => {
    await page.goto("./search?q=test");
    await expect(page.getByText("1-3 chars")).toBeVisible({ timeout: 10_000 });
    await expect(page.getByText("4-6 chars")).toBeVisible();
    await expect(page.getByText("7+ chars")).toBeVisible();
  });

  test("highlights correct tier based on name length", async ({ page }) => {
    await page.goto("./search?q=ab");
    await expect(page.getByText("Available!")).toBeVisible({ timeout: 10_000 });
    const tiers = page.locator("text=Pricing tiers").locator("..");
    const shortTier = tiers.locator("text=1-3 chars").locator("..");
    await expect(shortTier).toHaveClass(/border-primary/);
  });
});
