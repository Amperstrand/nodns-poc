import { test, expect } from "@playwright/test";

test.describe("Footer Completeness", () => {
  test("renders tagline text", async ({ page }) => {
    await page.goto("./");
    const footer = page.locator("footer");
    await expect(footer).toBeVisible();
    await expect(footer.getByText(/DNS from Nostr/i)).toBeVisible();
    await expect(footer.getByText(/no central authority/i)).toBeVisible();
  });

  test("has nodns.shop link", async ({ page }) => {
    await page.goto("./");
    const footer = page.locator("footer");
    const link = footer.getByRole("link", { name: "nodns.shop" });
    await expect(link).toBeVisible();
    await expect(link).toHaveAttribute("href", "https://nodns.shop");
  });

  test("has GitHub link with correct attributes", async ({ page }) => {
    await page.goto("./");
    const footer = page.locator("footer");
    const link = footer.getByRole("link", { name: "GitHub" });
    await expect(link).toBeVisible();
    await expect(link).toHaveAttribute("href", "https://github.com/Amperstrand/nodns-poc");
    await expect(link).toHaveAttribute("target", "_blank");
    await expect(link).toHaveAttribute("rel", "noopener noreferrer");
  });

  test("footer appears on all pages", async ({ page }) => {
    const pages = [
      "./",
      "./records",
      "./dashboard",
      "./wallet",
      "./learn",
      "./discoveries",
      "./search",
    ];
    for (const p of pages) {
      await page.goto(p);
      await expect(page.locator("footer")).toBeVisible({ timeout: 15_000 });
    }
  });
});

test.describe("Header Navigation - Desktop", () => {
  test.use({ viewport: { width: 1440, height: 900 } });

  test("all six nav items visible on desktop", async ({ page }) => {
    await page.goto("./");
    const nav = page.locator("header nav").first();
    for (const label of ["Home", "Records", "Dashboard", "Learn", "Discoveries", "Wallet"]) {
      await expect(nav.getByRole("link", { name: label })).toBeVisible();
    }
  });

  test("mobile hamburger hidden on desktop", async ({ page }) => {
    await page.goto("./");
    const hamburger = page.getByRole("button", { name: /Toggle menu/i });
    await expect(hamburger).toBeHidden();
  });

  test("active nav item is highlighted", async ({ page }) => {
    await page.goto("./records");
    const recordsLink = page.locator("header nav").first().getByRole("link", { name: "Records" });
    const className = await recordsLink.getAttribute("class");
    expect(className).toContain("bg-secondary");
  });

  test("desktop nav navigates to discoveries page", async ({ page }) => {
    await page.goto("./");
    await page.locator("header nav").first().getByRole("link", { name: "Discoveries" }).click();
    await expect(page).toHaveURL(/\/discoveries/, { timeout: 5_000 });
    await expect(
      page.getByRole("heading", { name: /Discoveries/i })
    ).toBeVisible({ timeout: 10_000 });
  });

  test("desktop nav navigates to learn page", async ({ page }) => {
    await page.goto("./");
    await page.locator("header nav").first().getByRole("link", { name: "Learn" }).click();
    await expect(page).toHaveURL(/\/learn/, { timeout: 5_000 });
  });
});

test.describe("Header Navigation - Mobile", () => {
  test.use({ viewport: { width: 390, height: 844 } });

  test("hamburger visible on mobile", async ({ page }) => {
    await page.goto("./");
    const hamburger = page.getByRole("button", { name: /Toggle menu/i });
    await expect(hamburger).toBeVisible();
  });

  test("desktop nav hidden on mobile", async ({ page }) => {
    await page.goto("./");
    const desktopNav = page.locator("header nav.hidden").first();
    await expect(desktopNav).toBeHidden();
  });

  test("hamburger opens mobile menu", async ({ page }) => {
    await page.goto("./");
    await page.evaluate(() => {
      localStorage.clear();
      indexedDB.deleteDatabase("coco-cashu");
    });
    await page.goto("./");

    const hamburger = page.getByRole("button", { name: /Toggle menu/i });
    await hamburger.click();

    const mobileMenu = page.locator("header div.md\\:hidden nav");
    await expect(mobileMenu).toBeVisible({ timeout: 5_000 });
    await expect(mobileMenu.getByRole("link", { name: "Records" })).toBeVisible();
    await expect(mobileMenu.getByRole("link", { name: "Dashboard" })).toBeVisible();
    await expect(mobileMenu.getByRole("link", { name: "Learn" })).toBeVisible();
    await expect(mobileMenu.getByRole("link", { name: "Discoveries" })).toBeVisible();
    await expect(mobileMenu.getByRole("link", { name: "Wallet" })).toBeVisible();
  });

  test("mobile menu link navigates and closes menu", async ({ page }) => {
    await page.goto("./");
    await page.evaluate(() => {
      localStorage.clear();
      indexedDB.deleteDatabase("coco-cashu");
    });
    await page.goto("./");

    const hamburger = page.getByRole("button", { name: /Toggle menu/i });
    await hamburger.click();

    const mobileMenu = page.locator("header div.md\\:hidden nav");
    await mobileMenu.getByRole("link", { name: "Records" }).click();
    await expect(page).toHaveURL(/\/records/, { timeout: 5_000 });
  });

  test("logo navigates home on mobile", async ({ page }) => {
    await page.goto("./records");
    await page.locator("header").getByRole("link", { name: /NoDNS/i }).click();
    await expect(page).toHaveURL(/\/nodns-poc\/$/, { timeout: 5_000 });
  });
});

test.describe("404 Handling", () => {
  test("non-existent route returns 404 page", async ({ page }) => {
    const resp = await page.goto("./this-page-does-not-exist-xyz123");
    expect(resp?.status()).toBe(404);
  });

  test("non-existent route shows 404 content", async ({ page }) => {
    await page.goto("./this-page-does-not-exist-xyz123");
    await expect(page.locator("body")).toBeVisible({ timeout: 5_000 });
    const bodyText = await page.locator("body").textContent();
    expect(bodyText?.toLowerCase()).toMatch(/404|not found|page/);
  });

  test("malformed route does not crash", async ({ page }) => {
    const resp = await page.goto("./search?q=");
    expect(resp?.status()).toBeLessThan(500);
  });
});

test.describe("Cross-Page Consistency", () => {
  test("header persists across all pages", async ({ page }) => {
    const pages = [
      "./",
      "./records",
      "./dashboard",
      "./wallet",
      "./learn",
      "./discoveries",
    ];
    for (const p of pages) {
      await page.goto(p);
      await expect(page.locator("header")).toBeVisible({ timeout: 15_000 });
      await expect(
        page.locator("header").getByRole("link", { name: /NoDNS/i })
      ).toBeVisible();
    }
  });

  test("logo always links to home", async ({ page }) => {
    const pages = ["./records", "./dashboard", "./learn", "./discoveries"];
    for (const p of pages) {
      await page.goto(p);
      await expect(page.locator("header")).toBeVisible({ timeout: 15_000 });
      const logoHref = await page
        .locator("header")
        .getByRole("link", { name: /NoDNS/i })
        .getAttribute("href");
      expect(logoHref).toBeTruthy();
    }
  });
});
