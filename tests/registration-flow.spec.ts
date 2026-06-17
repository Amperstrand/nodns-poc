import { test, expect } from "@playwright/test";

const API_BASE = "https://nodns.shop";

test.beforeEach(async ({ page }) => {
  await page.goto("./");
  await page.evaluate(() => {
    localStorage.clear();
    indexedDB.deleteDatabase("coco-cashu");
  });
  await page.goto("./");
});

test.describe("Wallet Top-Up", () => {
  // FLAKY: This test depends on the Cashu testnet (testnut.cashu.space) being
  // reachable and responding within 60s. When the testnet is down or slow, the
  // browser wallet component never finishes initializing, so the top-up input
  // field (placeholder="100") never renders and the test times out at
  // getByPlaceholder("100").fill("50").  The failure is in external infra, not
  // our code.  Re-enable once we have a reliable testnet or local mock mint.
  test.fixme("top up increases balance via testnut auto-settle", async ({ page }) => {
    await page.goto("./wallet");
    await expect(page.getByRole("heading", { name: "Wallet" })).toBeVisible({ timeout: 10_000 });
    await page.waitForTimeout(4_000);

    const balanceEl = page.locator("main").getByText(/^\d+ sats$/).first();
    const initialText = await balanceEl.textContent();
    const initialBalance = parseInt(initialText?.match(/(\d+)/)?.[1] ?? "0", 10);

    await page.getByPlaceholder("100").fill("50");
    await page.getByRole("button", { name: "Get Invoice" }).click();

    await expect(page.getByText(/Top-up successful/i)).toBeVisible({ timeout: 20_000 });

    await expect.poll(
      async () => {
        const text = await page.locator("main").getByText(/^\d+ sats$/).first().textContent();
        return parseInt(text?.match(/(\d+)/)?.[1] ?? "0", 10);
      },
      { timeout: 15_000, intervals: [2_000] }
    ).toBe(initialBalance + 50);
  });
});

test.describe("Registration - UI Flow", () => {
  test("order summary shows correct domain and price", async ({ page }) => {
    await page.goto("./register?name=verylongtestname");

    await expect(page.getByRole("heading", { name: "Order Summary" })).toBeVisible({ timeout: 10_000 });
    await expect(page.getByText("verylongtestname.nodns.shop")).toBeVisible();
    await expect(page.getByText("1 year")).toBeVisible();
    await expect(page.getByText("4 sats")).toBeVisible();
  });

  test("insufficient balance shows error and add funds link", async ({ page }) => {
    await page.goto("./register?name=verylongtestname");
    await expect(page.getByRole("heading", { name: "Order Summary" })).toBeVisible({ timeout: 10_000 });
    await expect(page.getByText(/more sats/i)).toBeVisible({ timeout: 10_000 });
    await expect(page.getByRole("link", { name: /Add funds/i })).toBeVisible();
  });

  test("order summary shows correct price for short name", async ({ page }) => {
    await page.goto("./register?name=ab");
    await expect(page.getByRole("heading", { name: "Order Summary" })).toBeVisible({ timeout: 10_000 });
    await expect(page.getByText("ab.nodns.shop")).toBeVisible();
    await expect(page.getByText("200 sats")).toBeVisible();
  });

  test("order summary shows wallet balance", async ({ page }) => {
    await page.goto("./register?name=testdomain123");
    await expect(page.getByRole("heading", { name: "Order Summary" })).toBeVisible({ timeout: 10_000 });
    await expect(page.getByText(/Wallet balance/i)).toBeVisible();
    await expect(page.getByText(/\d+ sats/).first()).toBeVisible();
  });

  test("register page without name shows empty state", async ({ page }) => {
    await page.goto("./register");
    await expect(
      page.getByRole("heading", { name: /No domain selected/i })
    ).toBeVisible({ timeout: 10_000 });
  });

  test("search to register navigation preserves name param", async ({ page }) => {
    await page.goto("./search?q=myregtest");
    await expect(page.getByText("Available!")).toBeVisible({ timeout: 10_000 });

    await page.getByRole("link", { name: /Register this domain/i }).click();
    await expect(page).toHaveURL(/register\?name=myregtest/, { timeout: 5_000 });
  });
});

test.describe("Registration - End-to-End", () => {
  // FLAKY: Same root cause as the Wallet Top-Up test above — depends on
  // testnut.cashu.space being reachable for wallet initialization.
  test.fixme("full registration flow: top-up → search → register → API verify", async ({ page }) => {
    test.setTimeout(120_000);

    // Step 1: Initialize wallet and top up
    await page.goto("./wallet");
    await expect(page.getByRole("heading", { name: "Wallet" })).toBeVisible({ timeout: 10_000 });
    await page.waitForTimeout(5_000);

    const balanceEl = page.locator("main").getByText(/sats/).first();
    const balanceText = await balanceEl.textContent();
    const balance = parseInt(balanceText?.match(/\d+/)?.[0] ?? "0", 10);

    if (balance < 10) {
      await page.getByPlaceholder("100").fill("50");
      await page.getByRole("button", { name: "Get Invoice" }).click();
      await expect(page.getByText(/Top-up successful/i)).toBeVisible({ timeout: 20_000 });
    }

    // Step 2: Search for unique domain
    const name = `e2e${Date.now().toString(36)}`;
    await page.goto(`./search?q=${name}`);
    await expect(page.getByText("Available!")).toBeVisible({ timeout: 10_000 });
    await expect(page.locator("h1.break-all")).toContainText(name);

    // Step 3: Register
    await page.getByRole("link", { name: /Register this domain/i }).click();
    await expect(page.getByRole("heading", { name: "Order Summary" })).toBeVisible({ timeout: 5_000 });
    await expect(page.getByText(`${name}.nodns.shop`)).toBeVisible();

    await page.getByRole("button", { name: /Pay.*Register/i }).click();

    // Step 4: Verify success screen
    await expect(page.getByText(/Domain Registered/i)).toBeVisible({ timeout: 30_000 });
    await expect(page.getByText(`${name}.nodns.shop`)).toBeVisible();
    await expect(page.getByText(/Event:/)).toBeVisible();

    // Step 5: Verify records appear in API (poll up to 60s)
    const eventMatch = await page.getByText(/Event:/).textContent();
    expect(eventMatch).toBeTruthy();

    await expect.poll(
      async () => {
        const resp = await page.request.get(`${API_BASE}/api/records?name=${name}`);
        if (!resp.ok()) return 0;
        const body = await resp.json();
        return body.records?.filter(
          (r: { rdata: string }) => r.rdata === "registered via nodns.shop"
        ).length ?? 0;
      },
      { timeout: 60_000, intervals: [5_000] }
    ).toBeGreaterThanOrEqual(1);
  });
});

test.describe("Dashboard - Registered Domains", () => {
  test("dashboard shows empty state for new identity", async ({ page }) => {
    await page.goto("./dashboard");
    await expect(page.getByRole("heading", { name: "My Domains" })).toBeVisible({ timeout: 10_000 });
    await page.waitForTimeout(2_000);
    const domainCount = await page.getByTestId("npub-group-header").count();
    expect(domainCount).toBe(0);
  });
});

test.describe("No Critical Errors During Registration", () => {
  test("registration pages load without critical JS errors", async ({ page }) => {
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
          !text.includes("Minified React error") &&
          !text.includes("Hydration") &&
          !text.includes("Text content did not match")
        ) {
          errors.push(text);
        }
      }
    });

    await page.goto("./wallet");
    await page.waitForTimeout(4_000);
    await page.goto("./register?name=errtestdomain");
    await page.waitForTimeout(2_000);

    if (errors.length > 0) {
      console.log("Critical JS errors:", errors);
    }
    expect(errors.length).toBe(0);
  });
});
