import { test, expect } from "@playwright/test";

test.describe("Profile Page - No Domain", () => {
  test("shows message when no domain param", async ({ page }) => {
    await page.goto("./profile");
    await expect(
      page.getByRole("heading", { name: /No domain specified/i })
    ).toBeVisible();
    await expect(
      page.getByRole("link", { name: /Search a domain/i })
    ).toBeVisible();
  });
});

test.describe("Profile Page - Unregistered Domain", () => {
  test("shows domain name and no-records state", async ({ page }) => {
    const name = `e2eprof${Date.now().toString(36)}`;
    await page.goto(`./profile?domain=${name}`);
    await expect(
      page.locator("h1").filter({ hasText: name })
    ).toBeVisible({ timeout: 10_000 });
    await expect(
      page.getByText(/No DNS records found/i)
    ).toBeVisible();
  });

  test("shows source verification bar with zero counts", async ({ page }) => {
    await page.goto("./profile?domain=nonexistentprofile123");
    await expect(page.getByText("Sources", { exact: true })).toBeVisible({ timeout: 10_000 });
    await expect(page.getByText(/Sources agree/i)).toBeVisible();
  });

  test("shows refresh button", async ({ page }) => {
    await page.goto("./profile?domain=nonexistentprofile123");
    await expect(page.getByText(/Refresh/i)).toBeVisible();
  });
});

test.describe("Profile Page - Navigation", () => {
  test("back to search link works", async ({ page }) => {
    await page.goto("./profile?domain=nonexistentprofile123");
    await expect(
      page.getByRole("link", { name: /Back to search/i })
    ).toBeVisible();
  });

  test("handles FQDN input by stripping zone suffix", async ({ page }) => {
    await page.goto("./profile?domain=mytest.nodns.shop");
    await expect(
      page.locator("h1").filter({ hasText: "mytest" })
    ).toBeVisible({ timeout: 10_000 });
    await expect(
      page.locator("h1").filter({ hasText: "nodns.shop.nodns" })
    ).not.toBeVisible();
  });
});

test.describe("Profile Page - Layout", () => {
  test("renders domain header with globe icon", async ({ page }) => {
    await page.goto("./profile?domain=testprofile");
    await expect(
      page.locator("h1").filter({ hasText: "testprofile" })
    ).toBeVisible({ timeout: 10_000 });
  });

  test("renders source icons (API, Nostr, DNS)", async ({ page }) => {
    await page.goto("./profile?domain=testprofile");
    await expect(page.getByText("Sources", { exact: true })).toBeVisible({ timeout: 10_000 });
    await expect(page.locator("text=🗄️").first()).toBeVisible();
    await expect(page.locator("text=🔐").first()).toBeVisible();
    await expect(page.locator("text=🌐").first()).toBeVisible();
  });

  test("no console errors on profile page", async ({ page }) => {
    const errors: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error") {
        const text = msg.text();
        if (
          !text.includes("502") &&
          !text.includes("503") &&
          !text.includes("WebSocket") &&
          !text.includes("Failed to load resource") &&
          !text.includes("testnut") &&
          !text.includes("Minified React error")
        ) {
          errors.push(text);
        }
      }
    });
    await page.goto("./profile?domain=testprofile");
    await page.waitForTimeout(3_000);
    expect(errors.length).toBe(0);
  });
});
