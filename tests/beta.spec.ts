import { test, expect } from "@playwright/test";

const BASE = "https://beta.nodns.shop";

test.beforeEach(async ({ page }) => {
  await page.goto("/");
  await page.evaluate(() => {
    localStorage.clear();
    indexedDB.deleteDatabase("coco-cashu");
  });
});

test.describe("Home Page", () => {
  test("renders hero section", async ({ page }) => {
    await page.goto("/");
    await expect(
      page.getByRole("heading", { name: /Your domain.*No registrar/i })
    ).toBeVisible();
    await expect(
      page.getByText(/Register a .nodns.shop subdomain/i)
    ).toBeVisible();
  });

  test("navigation links are present", async ({ page }) => {
    await page.goto("/");
    const nav = page.locator("nav");
    await expect(nav).toBeVisible();
    await expect(nav.getByText("Home")).toBeVisible();
    await expect(nav.getByText("Dashboard")).toBeVisible();
    await expect(nav.getByText("Wallet")).toBeVisible();
  });

  test("footer has beta notice and link to nodns.shop", async ({ page }) => {
    await page.goto("/");
    const footer = page.locator("footer");
    await expect(footer).toBeVisible();
    await expect(footer.getByText(/You're on the beta/)).toBeVisible();
    await expect(footer.getByRole("link", { name: "nodns.shop" })).toBeVisible();
  });
});

test.describe("Dashboard Page", () => {
  test("renders heading and empty domains state", async ({ page }) => {
    await page.goto("/dashboard");
    await expect(
      page.getByRole("heading", { name: "No domains yet" })
    ).toBeVisible();
    await expect(
      page.getByText(/Register your first subdomain/i)
    ).toBeVisible();
  });

  test("shows identity pill in header with npub", async ({ page }) => {
    await page.goto("/dashboard");
    await page.waitForTimeout(1000);
    const header = page.locator("header");
    await expect(header.getByText(/npub1/)).toBeVisible({ timeout: 5000 });
  });

  test("shows sats balance in header", async ({ page }) => {
    await page.goto("/dashboard");
    await page.waitForTimeout(1000);
    const header = page.locator("header");
    await expect(header.getByText(/sats/)).toBeVisible({ timeout: 5000 });
  });

  test("shows empty state or register link for new identity", async ({ page }) => {
    await page.evaluate(() => {
      localStorage.removeItem("nodns-identity");
    });
    await page.goto("/dashboard");
    const hasEmptyState = await page.getByRole("heading", { name: "No domains yet" }).isVisible({ timeout: 10000 }).catch(() => false);
    const hasRegisterLink = await page.getByRole("link", { name: /register/i }).first().isVisible().catch(() => false);
    expect(hasEmptyState || hasRegisterLink).toBeTruthy();
  });
});

test.describe("Search Page", () => {
  test("renders search prompt", async ({ page }) => {
    await page.goto("/search");
    await expect(
      page.getByRole("heading", { name: /Search for a domain/i })
    ).toBeVisible();
    await expect(page.getByText(/nodns.shop domain/i)).toBeVisible();
  });

  test("has link to go home for searching", async ({ page }) => {
    await page.goto("/search");
    await expect(
      page.getByRole("link", { name: /go home/i })
    ).toBeVisible();
  });
});

test.describe("Wallet Page", () => {
  test("initializes wallet without crashing", async ({ page }) => {
    await page.goto("/wallet");
    await page.waitForTimeout(6000);
    await expect(
      page.getByRole("heading", { name: "Wallet" })
    ).toBeVisible();
  });

  test("shows balance section", async ({ page }) => {
    await page.goto("/wallet");
    await page.waitForTimeout(6000);
    const main = page.locator("main");
    await expect(main.getByText(/Balance/)).toBeVisible();
    await expect(main.getByText(/\d+ sats/)).toBeVisible();
  });

  test("shows identity section", async ({ page }) => {
    await page.goto("/wallet");
    await page.waitForTimeout(6000);
    await expect(page.getByText(/Public Key.*npub/)).toBeVisible();
    await expect(page.getByText(/Private Key.*nsec/)).toBeVisible();
  });

  test("shows receive tokens section", async ({ page }) => {
    await page.goto("/wallet");
    await page.waitForTimeout(6000);
    await expect(
      page.getByRole("heading", { name: /Receive Tokens/i })
    ).toBeVisible();
    await expect(page.getByPlaceholder(/cashuA/i)).toBeVisible();
  });

  test("shows send tokens section", async ({ page }) => {
    await page.goto("/wallet");
    await page.waitForTimeout(6000);
    await expect(
      page.getByRole("heading", { name: /Send Tokens/i })
    ).toBeVisible();
  });

  test("shows transaction history", async ({ page }) => {
    await page.goto("/wallet");
    await page.waitForTimeout(6000);
    await expect(
      page.getByRole("heading", { name: "History" })
    ).toBeVisible();
  });

  test("handles mint offline gracefully", async ({ page }) => {
    await page.goto("/wallet");
    await page.waitForTimeout(6000);
    const main = page.locator("main").first();
    const text = await main.textContent();
    const hasValidState =
      text?.includes("mint offline") ||
      text?.includes("Mint temporarily unavailable") ||
      text?.includes("ready");
    expect(hasValidState).toBeTruthy();
  });

  test("action buttons are disabled when inputs are empty", async ({
    page,
  }) => {
    await page.goto("/wallet");
    await page.waitForTimeout(6000);

    // Buttons are disabled because no token/amount entered yet (regardless of mint state)
    const receiveBtn = page.getByRole("button", { name: /^Receive$/ });
    const sendBtn = page.getByRole("button", { name: /Create Token/i });

    await expect(receiveBtn).toBeDisabled();
    await expect(sendBtn).toBeDisabled();
  });
});

test.describe("Register Page", () => {
  test("renders with prompt to search first", async ({ page }) => {
    await page.goto("/register");
    await expect(
      page.getByRole("heading", { name: /No domain selected/i })
    ).toBeVisible();
    await expect(
      page.getByText(/Search for a domain first/i)
    ).toBeVisible();
    await expect(
      page.getByRole("link", { name: /Search for a domain/i })
    ).toBeVisible();
  });
});

test.describe("Domain Page", () => {
  test("renders domain detail view", async ({ page }) => {
    await page.goto("/domain");
    await expect(page.locator("main")).toBeVisible();
    await expect(
      page.getByRole("heading", { name: /domain/i }).first()
    ).toBeVisible();
  });
});

test.describe("API Endpoints", () => {
  test("/api/records returns valid JSON with records array", async ({
    request,
  }) => {
    const resp = await request.get(`${BASE}/api/records`);
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body).toHaveProperty("records");
    expect(Array.isArray(body.records)).toBeTruthy();
  });

  test("/api/records?pubkey= filters results", async ({ request }) => {
    const resp = await request.get(
      `${BASE}/api/records?pubkey=0000000000000000000000000000000000000000000000000000000000000001`
    );
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body).toHaveProperty("records");
    expect(Array.isArray(body.records)).toBeTruthy();
  });

  test("/api/check returns availability status", async ({ request }) => {
    const resp = await request.get(`${BASE}/api/check?name=test`);
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body).toHaveProperty("name");
    expect(body).toHaveProperty("zone");
    expect(body).toHaveProperty("api");
    expect(body).toHaveProperty("dns");
    expect(body.api).toHaveProperty("registered");
    expect(body.dns).toHaveProperty("registered");
  });

  test("/api/zones/nodns.shop/pricing returns pricing", async ({
    request,
  }) => {
    const resp = await request.get(`${BASE}/api/zones/nodns.shop/pricing`);
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body).toHaveProperty("zone");
    expect(body).toHaveProperty("create_price");
    expect(body).toHaveProperty("update_price");
    expect(body).toHaveProperty("delete_price");
    expect(body.zone).toBe("nodns.shop");
  });
});

test.describe("Cross-page Navigation", () => {
  test("home → dashboard → wallet → home", async ({ page }) => {
    await page.goto("/");

    await page.getByRole("link", { name: "Dashboard" }).click();
    await expect(page).toHaveURL(/\/dashboard/, { timeout: 5000 });

    await page.getByRole("link", { name: "Wallet" }).click();
    await expect(page).toHaveURL(/\/wallet/, { timeout: 5000 });

    await page.getByRole("link", { name: /Back to Home/i }).click();
    await expect(page).toHaveURL(/\/$/, { timeout: 5000 });
  });

  test("header logo links back to home from any page", async ({ page }) => {
    await page.goto("/dashboard");
    await page.locator("header").getByRole("link", { name: "NoDNS.shop" }).click();
    await expect(page).toHaveURL(/\/$/, { timeout: 5000 });
  });
});

test.describe("Identity Persistence", () => {
  test("same identity across wallet and dashboard pages", async ({ page }) => {
    await page.goto("/wallet");
    await page.waitForTimeout(6000);

    const npubOnWallet = await page
      .locator("header")
      .getByText(/npub1/)
      .first()
      .textContent();

    await page.goto("/dashboard");
    await page.waitForTimeout(2000);

    const npubOnDashboard = await page
      .locator("header")
      .getByText(/npub1/)
      .first()
      .textContent();

    expect(npubOnWallet).toBeTruthy();
    expect(npubOnDashboard).toBeTruthy();
    expect(npubOnWallet?.slice(0, 20)).toBe(npubOnDashboard?.slice(0, 20));
  });
});

test.describe("No Critical JS Errors", () => {
  test("pages load without critical JS errors (excluding mint 502s)", async ({
    page,
  }) => {
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

    await page.goto("/");
    await page.waitForTimeout(2000);

    await page.goto("/dashboard");
    await page.waitForTimeout(2000);

    await page.goto("/wallet");
    await page.waitForTimeout(6000);

    if (errors.length > 0) {
      console.log("Critical JS errors:", errors);
    }
    expect(errors.length).toBe(0);
  });
});
