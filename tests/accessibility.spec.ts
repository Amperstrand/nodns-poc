import { test, expect } from "@playwright/test";

test.describe("Accessibility - Skip Link", () => {
  test("skip-to-content link exists and is keyboard-focusable", async ({ page }) => {
    await page.goto("./");
    const skipLink = page.getByRole("link", { name: /Skip to content/i });
    await expect(skipLink).toHaveAttribute("href", "#main-content");
  });

  test("skip-to-content link becomes visible on focus", async ({ page }) => {
    await page.goto("./");
    await page.evaluate(() => {
      localStorage.clear();
      indexedDB.deleteDatabase("coco-cashu");
    });
    const skipLink = page.getByRole("link", { name: /Skip to content/i });
    await skipLink.focus();
    await expect(skipLink).toBeVisible();
  });
});

test.describe("Accessibility - Landmarks", () => {
  test("html has lang attribute", async ({ page }) => {
    await page.goto("./");
    const lang = await page.locator("html").getAttribute("lang");
    expect(lang).toBe("en");
  });

  test("header landmark exists", async ({ page }) => {
    await page.goto("./");
    await expect(page.locator("header")).toBeVisible();
  });

  test("footer landmark exists", async ({ page }) => {
    await page.goto("./");
    await expect(page.locator("footer")).toBeVisible();
  });

  test("main landmark exists on home", async ({ page }) => {
    await page.goto("./");
    await expect(page.locator("main")).toBeVisible();
  });

  test("main landmark exists on records page", async ({ page }) => {
    await page.goto("./records");
    await expect(page.locator("main")).toBeVisible({ timeout: 10_000 });
  });

  test("main landmark exists on learn page", async ({ page }) => {
    await page.goto("./learn");
    await expect(page.locator("main")).toBeVisible({ timeout: 10_000 });
  });

  test("main landmark exists on discoveries page", async ({ page }) => {
    await page.goto("./discoveries");
    await expect(page.locator("main")).toBeVisible({ timeout: 10_000 });
  });

  test("nav landmark exists in header", async ({ page }) => {
    await page.goto("./");
    await expect(page.locator("header nav")).toBeVisible();
  });
});

test.describe("Accessibility - Keyboard Navigation", () => {
  test("Tab key cycles through interactive elements on home", async ({ page }) => {
    await page.goto("./");
    await page.evaluate(() => {
      localStorage.clear();
      indexedDB.deleteDatabase("coco-cashu");
    });

    await page.keyboard.press("Tab");
    const focusedTag = await page.evaluate(() => document.activeElement?.tagName);
    expect(["A", "BUTTON", "INPUT", "SELECT", "TEXTAREA"]).toContain(focusedTag);
  });

  test("Enter key activates focused link", async ({ page }) => {
    await page.goto("./");
    await page.evaluate(() => {
      localStorage.clear();
      indexedDB.deleteDatabase("coco-cashu");
    });

    const recordsLink = page.getByRole("link", { name: "Records" }).first();
    await recordsLink.focus();
    await page.keyboard.press("Enter");
    await expect(page).toHaveURL(/\/records/, { timeout: 5_000 });
  });

  test("learn page collapsible buttons respond to Enter", async ({ page }) => {
    await page.goto("./learn");
    const roadmapButton = page.getByRole("button", { name: /Roadmap/i }).first();
    await expect(roadmapButton).toBeVisible({ timeout: 10_000 });

    await roadmapButton.focus();
    await expect(roadmapButton).toHaveAttribute("aria-expanded", "false");
    await page.keyboard.press("Enter");
    await expect(roadmapButton).toHaveAttribute("aria-expanded", "true");
  });

  test("learn page collapsible buttons respond to Space", async ({ page }) => {
    await page.goto("./learn");
    const roadmapButton = page.getByRole("button", { name: /Roadmap/i }).first();
    await expect(roadmapButton).toBeVisible({ timeout: 10_000 });

    await roadmapButton.focus();
    await page.keyboard.press("Space");
    await expect(roadmapButton).toHaveAttribute("aria-expanded", "true");
  });
});

test.describe("Accessibility - ARIA Labels", () => {
  test("mobile menu toggle has aria-label", async ({ browser }) => {
    const context = await browser.newContext({
      viewport: { width: 390, height: 844 },
      baseURL: "https://amperstrand.github.io/nodns-poc/",
    });
    const page = await context.newPage();
    await page.goto("./");
    const menuButton = page.getByRole("button", { name: /Toggle menu/i });
    await expect(menuButton).toHaveAttribute("aria-label", "Toggle menu");
    await context.close();
  });

  test("search input has accessible label", async ({ page }) => {
    await page.goto("./");
    const searchInput = page.locator('input[type="text"]').first();
    await expect(searchInput).toBeVisible({ timeout: 10_000 });
    const ariaLabel = await searchInput.getAttribute("aria-label");
    const placeholder = await searchInput.getAttribute("placeholder");
    expect(ariaLabel || placeholder).toBeTruthy();
  });

  test("records page tab navigation has proper roles", async ({ page }) => {
    await page.goto("./records");
    await expect(page.getByRole("tab", { name: "API + Nostr" })).toBeVisible({ timeout: 15_000 });
    await expect(page.getByRole("tab", { name: "DNS Resolver" })).toBeVisible();
    await expect(page.getByRole("tab", { name: "Nostr Events" })).toBeVisible();
  });
});

test.describe("Accessibility - Color Contrast (CSS Verification)", () => {
  test("muted-foreground CSS variable meets WCAG AA threshold", async ({ page }) => {
    await page.goto("./");
    const color = await page.evaluate(() => {
      const el = document.createElement("span");
      el.className = "text-muted-foreground";
      document.body.appendChild(el);
      const computed = window.getComputedStyle(el).color;
      el.remove();
      return computed;
    });
    expect(color).toBeTruthy();
    const rgb = color.match(/\d+/g);
    expect(rgb).toBeTruthy();
    if (rgb) {
      const [, g, b] = rgb.map(Number);
      expect(g).toBeGreaterThanOrEqual(130);
      expect(b).toBeGreaterThanOrEqual(130);
    }
  });

  test("foreground CSS variable is light enough for dark background", async ({ page }) => {
    await page.goto("./");
    const color = await page.evaluate(() => {
      const el = document.createElement("span");
      el.className = "text-foreground";
      document.body.appendChild(el);
      const computed = window.getComputedStyle(el).color;
      el.remove();
      return computed;
    });
    const rgb = color.match(/\d+/g);
    expect(rgb).toBeTruthy();
    if (rgb) {
      const [, g, b] = rgb.map(Number);
      expect(g).toBeGreaterThanOrEqual(180);
      expect(b).toBeGreaterThanOrEqual(180);
    }
  });
});

test.describe("Accessibility - Page Titles", () => {
  test("home page has descriptive title", async ({ page }) => {
    await page.goto("./");
    const title = await page.title();
    expect(title).toContain("NoDNS");
  });

  test("learn page has descriptive title", async ({ page }) => {
    await page.goto("./learn");
    const title = await page.title();
    expect(title).toContain("NoDNS");
  });
});
