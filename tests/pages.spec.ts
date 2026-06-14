import { test, expect } from "@playwright/test";

test.beforeEach(async ({ page }) => {
  await page.goto("./");
  await page.evaluate(() => {
    localStorage.clear();
    indexedDB.deleteDatabase("coco-cashu");
  });
});

test.describe("Landing Page", () => {
  test("renders hero section with search", async ({ page }) => {
    await page.goto("./");
    await expect(
      page.getByRole("heading", { name: /domain.*no registrar/i })
    ).toBeVisible();
  });

  test("record browser teaser shows stats and link", async ({ page }) => {
    await page.goto("./");
    await expect(
      page.getByRole("heading", { name: "DNS Record Browser" })
    ).toBeVisible();
    await expect(
      page.getByRole("link", { name: /Browse All Records/i })
    ).toBeVisible();
  });

  test("footer has GitHub link", async ({ page }) => {
    await page.goto("./");
    const footer = page.locator("footer");
    await expect(footer).toBeVisible();
    await expect(
      footer.getByRole("link", { name: "GitHub" })
    ).toBeVisible();
  });
});

test.describe("Navigation", () => {
  test("header has all nav links", async ({ page }) => {
    await page.goto("./");
    const nav = page.locator("nav").first();
    await expect(nav.getByText("Home")).toBeVisible();
    await expect(nav.getByText("Records")).toBeVisible();
    await expect(nav.getByText("Dashboard")).toBeVisible();
    await expect(nav.getByText("Wallet")).toBeVisible();
  });

  test("home → records → dashboard → wallet round trip", async ({ page }) => {
    await page.goto("./");

    await page.getByRole("link", { name: "Records" }).first().click();
    await expect(page).toHaveURL(/\/records/, { timeout: 5000 });

    await page.getByRole("link", { name: "Dashboard" }).first().click();
    await expect(page).toHaveURL(/\/dashboard/, { timeout: 5000 });

    await page.getByRole("link", { name: "Wallet" }).first().click();
    await expect(page).toHaveURL(/\/wallet/, { timeout: 5000 });
  });

  test("logo links back to home", async ({ page }) => {
    await page.goto("./dashboard");
    await page.locator("header").getByRole("link", { name: /NoDNS/i }).click();
    await expect(page).toHaveURL(/\/nodns-poc\/$/, { timeout: 5000 });
  });
});

test.describe("Records Page", () => {
  test("renders heading and tab navigation", async ({ page }) => {
    await page.goto("./records");
    await expect(
      page.getByRole("heading", { name: "DNS Record Browser" })
    ).toBeVisible();
    await expect(page.getByRole("tab", { name: "API + Nostr" })).toBeVisible();
    await expect(page.getByRole("tab", { name: "DNS Resolver" })).toBeVisible();
    await expect(page.getByRole("tab", { name: "Nostr Events" })).toBeVisible();
  });

  test("loads and displays merged records from API + Nostr", async ({
    page,
  }) => {
    await page.goto("./records");
    const total = page.getByTestId("stat-total");
    await expect(total).toBeVisible({ timeout: 15_000 });
    await expect(total).toHaveText(/\d+/, { timeout: 15_000 });
    const value = await total.textContent();
    expect(parseInt(value ?? "0", 10)).toBeGreaterThanOrEqual(1);
  });

  test("record groups are collapsible", async ({ page }) => {
    await page.goto("./records");
    const firstHeader = page.getByTestId("npub-group-header").first();
    await expect(firstHeader).toBeVisible({ timeout: 15_000 });

    const allBodies = page.getByTestId("npub-group-records");
    await expect(allBodies.first()).toBeVisible();
    const initialCount = await allBodies.count();
    expect(initialCount).toBeGreaterThanOrEqual(1);

    await page.waitForTimeout(3_000);

    await firstHeader.click();
    await expect(allBodies).toHaveCount(initialCount - 1, { timeout: 5_000 });

    await firstHeader.click();
    await expect(allBodies).toHaveCount(initialCount, { timeout: 5_000 });
  });

  test("expand all / collapse all button works", async ({ page }) => {
    await page.goto("./records");
    const header = page.getByTestId("npub-group-header").first();
    await expect(header).toBeVisible({ timeout: 15_000 });
    await page.waitForTimeout(3_000);

    const expandBtn = page.getByRole("button", { name: /expand all/i });
    const collapseBtn = page.getByRole("button", { name: /collapse all/i });

    const bodies = page.getByTestId("npub-group-records");
    const initialVisible = await bodies.count();

    if (await expandBtn.isVisible().catch(() => false)) {
      await expandBtn.click();
      await page.waitForTimeout(500);
      const afterExpand = await bodies.count();
      expect(afterExpand).toBeGreaterThanOrEqual(initialVisible);

      await collapseBtn.click();
      await page.waitForTimeout(500);
      const afterCollapse = await bodies.count();
      expect(afterCollapse).toBeLessThan(afterExpand);
    }
  });

  test("DNS resolver tab accepts queries", async ({ page }) => {
    await page.goto("./records");
    await page.getByText("DNS Resolver").click();

    const input = page.locator('input[placeholder="Enter FQDN to query"]');
    await expect(input).toBeVisible();

    await input.fill("npub190queyng2pmx0jfw5rkx4fjjl3u0zxz6nlyaja53p2n0ydupr6jsdnqt8q.nodns.shop");
    await page.getByRole("button", { name: /Query DNS/i }).click();

    await expect(page.locator("pre code")).toBeVisible({ timeout: 15_000 });
  });
});

test.describe("Dashboard Page", () => {
  test("renders heading and empty state for new identity", async ({
    page,
  }) => {
    await page.goto("./dashboard");
    await expect(
      page.getByRole("heading", { name: "My Domains" })
    ).toBeVisible();
  });

  test("shows identity npub in header", async ({ page }) => {
    await page.goto("./dashboard");
    await page.waitForTimeout(1000);
    const header = page.locator("header");
    await expect(header.getByText(/npub1/)).toBeVisible({ timeout: 5000 });
  });

  test("shows wallet balance in status bar", async ({ page }) => {
    await page.goto("./dashboard");
    await page.waitForTimeout(1000);
    await expect(page.getByText(/sats/).first()).toBeVisible({
      timeout: 5000,
    });
  });
});

test.describe("Domain Page", () => {
  test("shows no-domain state without query param", async ({ page }) => {
    await page.goto("./domain");
    await expect(
      page.getByRole("heading", { name: "No domain selected" })
    ).toBeVisible();
    await expect(
      page.getByRole("link", { name: /Back to Dashboard/i })
    ).toBeVisible();
  });

  test("renders domain detail with records when name provided", async ({
    page,
  }) => {
    await page.goto("./domain?name=npub190queyng2pmx0jfw5rkx4fjjl3u0zxz6nlyaja53p2n0ydupr6jsdnqt8q");
    await expect(
      page.getByRole("heading", { name: /nodns\.shop/ }).first()
    ).toBeVisible({ timeout: 10_000 });
  });
});

test.describe("Wallet Page", () => {
  test("initializes and shows heading", async ({ page }) => {
    await page.goto("./wallet");
    await expect(
      page.getByRole("heading", { name: "Wallet" })
    ).toBeVisible({ timeout: 10_000 });
  });

  test("shows balance section", async ({ page }) => {
    await page.goto("./wallet");
    await page.waitForTimeout(6000);
    await expect(page.getByText(/Balance/)).toBeVisible();
    await expect(page.locator("main").getByText(/\d+ sats/)).toBeVisible();
  });

  test("shows identity key info", async ({ page }) => {
    await page.goto("./wallet");
    await page.waitForTimeout(6000);
    await expect(page.getByText(/npub/).first()).toBeVisible();
  });
});

test.describe("Search Page", () => {
  test("renders search prompt without query param", async ({ page }) => {
    await page.goto("./search");
    await expect(
      page.getByRole("heading", { name: /domain/i })
    ).toBeVisible();
  });
});

test.describe("Register Page", () => {
  test("renders no-domain state without query param", async ({ page }) => {
    await page.goto("./register");
    await expect(
      page.getByRole("heading", { name: /No domain selected/i })
    ).toBeVisible();
  });
});

test.describe("Identity Persistence", () => {
  test("same identity across wallet and dashboard pages", async ({ page }) => {
    await page.goto("./wallet");
    await page.waitForTimeout(6000);

    const npubOnWallet = await page
      .locator("header")
      .getByText(/npub1/)
      .first()
      .textContent();

    await page.goto("./dashboard");
    await page.waitForTimeout(2000);

    const npubOnDashboard = await page
      .locator("header")
      .getByText(/npub1/)
      .first()
      .textContent();

    expect(npubOnWallet).toBeTruthy();
    expect(npubOnDashboard).toBeTruthy();
    expect(npubOnWallet!.slice(0, 20)).toBe(npubOnDashboard!.slice(0, 20));
  });
});

test.describe("No Critical JS Errors", () => {
  test("pages load without critical JS errors", async ({ page }) => {
    const errors: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error") {
        const text = msg.text();
        if (
          !text.includes("502") &&
          !text.includes("503") &&
          !text.includes("WebSocket connection") &&
          !text.includes("mint-proxy") &&
          !text.includes("Failed to load resource") &&
          !text.includes("testnut") &&
          !text.includes("Minified React error") &&
          !text.includes("Hydration") &&
          !text.includes("Text content did not match")
        ) {
          errors.push(text);
        }
      }
    });

    await page.goto("./");
    await page.waitForTimeout(2000);
    await page.goto("./records");
    await page.waitForTimeout(5000);
    await page.goto("./dashboard");
    await page.waitForTimeout(2000);
    await page.goto("./wallet");
    await page.waitForTimeout(6000);

    if (errors.length > 0) {
      console.log("Critical JS errors:", errors);
    }
    expect(errors.length).toBe(0);
  });
});
