import { test, expect } from "@playwright/test";
import { execSync } from "child_process";
import * as path from "path";
import * as fs from "fs";

const BASE_URL = "https://amperstrand.github.io/nodns-poc/";
const API_BASE = "https://nodns.shop";

const gitHash = execSync("git rev-parse --short HEAD").toString().trim();
const screenshotDir = path.join("screenshots", gitHash);

fs.mkdirSync(screenshotDir, { recursive: true });

const desktopViewport = { width: 1440, height: 900 };
const mobileViewport = { width: 390, height: 844 };

function shot(name: string) {
  return path.join(screenshotDir, `${name}.png`);
}

test.describe.configure({ mode: "serial" });

test.describe("Visual QA - Desktop (1440x900)", () => {
  test.use({ viewport: desktopViewport });

  test.beforeEach(async ({ page }) => {
    await page.goto("./");
    await page.evaluate(() => {
      localStorage.clear();
      indexedDB.deleteDatabase("coco-cashu");
    });
  });

  test("home - full page", async ({ page }) => {
    await page.goto("./");
    await page.waitForLoadState("networkidle");
    await expect(page.getByRole("heading", { name: /Your domain/i })).toBeVisible({ timeout: 15_000 });
    await page.waitForTimeout(2_000);
    await page.screenshot({ path: shot("01-home-desktop"), fullPage: true });
  });

  test("home - hero section above fold", async ({ page }) => {
    await page.goto("./");
    await page.waitForLoadState("networkidle");
    await expect(page.getByRole("heading", { name: /Your domain/i })).toBeVisible({ timeout: 15_000 });
    await page.waitForTimeout(1_000);
    await page.screenshot({ path: shot("02-home-hero-desktop"), fullPage: false });
  });

  test("search - empty state", async ({ page }) => {
    await page.goto("./search");
    await page.waitForLoadState("networkidle");
    await page.waitForTimeout(1_500);
    await page.screenshot({ path: shot("03-search-empty-desktop"), fullPage: true });
  });

  test("search - available domain (7+ chars)", async ({ page }) => {
    await page.goto("./search?q=myawesomedomain");
    await page.waitForLoadState("networkidle");
    await expect(page.getByText("Available!")).toBeVisible({ timeout: 15_000 });
    await page.waitForTimeout(1_000);
    await page.screenshot({ path: shot("04-search-available-desktop"), fullPage: true });
  });

  test("search - short name pricing (1-3 chars)", async ({ page }) => {
    await page.goto("./search?q=ab");
    await page.waitForLoadState("networkidle");
    await expect(page.getByText("Available!")).toBeVisible({ timeout: 15_000 });
    await page.waitForTimeout(1_000);
    await page.screenshot({ path: shot("05-search-short-name-desktop"), fullPage: true });
  });

  test("search - medium name pricing (4-6 chars)", async ({ page }) => {
    await page.goto("./search?q=test");
    await page.waitForLoadState("networkidle");
    await expect(page.getByText("Available!")).toBeVisible({ timeout: 15_000 });
    await page.waitForTimeout(1_000);
    await page.screenshot({ path: shot("06-search-medium-name-desktop"), fullPage: true });
  });

  test("register - order summary with insufficient balance", async ({ page }) => {
    await page.goto("./register?name=myawesomedomain");
    await page.waitForLoadState("networkidle");
    await expect(page.getByRole("heading", { name: "Order Summary" })).toBeVisible({ timeout: 15_000 });
    await page.waitForTimeout(1_000);
    await page.screenshot({ path: shot("07-register-insufficient-desktop"), fullPage: true });
  });

  test("register - no domain selected", async ({ page }) => {
    await page.goto("./register");
    await page.waitForLoadState("networkidle");
    await page.waitForTimeout(1_000);
    await page.screenshot({ path: shot("08-register-empty-desktop"), fullPage: true });
  });

  test("records - with data", async ({ page }) => {
    await page.goto("./records");
    await page.waitForLoadState("networkidle");
    await page.waitForTimeout(3_000);
    await page.screenshot({ path: shot("09-records-desktop"), fullPage: true });
  });

  test("dashboard - empty state", async ({ page }) => {
    await page.goto("./dashboard");
    await page.waitForLoadState("networkidle");
    await expect(page.getByRole("heading", { name: "My Domains" })).toBeVisible({ timeout: 15_000 });
    await page.waitForTimeout(2_000);
    await page.screenshot({ path: shot("10-dashboard-empty-desktop"), fullPage: true });
  });

  test("wallet - initialized", async ({ page }) => {
    await page.goto("./wallet");
    await page.waitForLoadState("networkidle");
    await expect(page.getByRole("heading", { name: "Wallet" })).toBeVisible({ timeout: 15_000 });
    await page.waitForTimeout(4_000);
    await page.screenshot({ path: shot("11-wallet-desktop"), fullPage: true });
  });

  test("domain - profile page unregistered", async ({ page }) => {
    await page.goto("./domain?name=unregisteredtest123");
    await page.waitForLoadState("networkidle");
    await page.waitForTimeout(2_000);
    await page.screenshot({ path: shot("12-domain-unregistered-desktop"), fullPage: true });
  });

  test("learn - full page", async ({ page }) => {
    await page.goto("./learn");
    await page.waitForLoadState("networkidle");
    await page.waitForTimeout(1_500);
    await page.screenshot({ path: shot("13-learn-desktop"), fullPage: true });
  });

  test("discoveries - full page", async ({ page }) => {
    await page.goto("./discoveries");
    await page.waitForLoadState("networkidle");
    await page.waitForTimeout(1_500);
    await page.screenshot({ path: shot("14-discoveries-desktop"), fullPage: true });
  });
});

test.describe("Visual QA - Mobile (390x844)", () => {
  test.beforeEach(async ({ browser }) => {
    const context = await browser.newContext({
      viewport: mobileViewport,
      baseURL: BASE_URL,
    });
    const page = await context.newPage();
    await page.goto("./");
    await page.evaluate(() => {
      localStorage.clear();
      indexedDB.deleteDatabase("coco-cashu");
    });
    await page.close();
    await context.close();
  });

  test("home - mobile", async ({ browser }) => {
    const context = await browser.newContext({
      viewport: mobileViewport,
      baseURL: BASE_URL,
    });
    const page = await context.newPage();
    await page.goto("./");
    await page.waitForLoadState("networkidle");
    await expect(page.getByRole("heading", { name: /Your domain/i })).toBeVisible({ timeout: 15_000 });
    await page.waitForTimeout(2_000);
    await page.screenshot({ path: shot("15-home-mobile"), fullPage: true });
    await context.close();
  });

  test("search available - mobile", async ({ browser }) => {
    const context = await browser.newContext({
      viewport: mobileViewport,
      baseURL: BASE_URL,
    });
    const page = await context.newPage();
    await page.goto("./search?q=myawesomedomain");
    await page.waitForLoadState("networkidle");
    await expect(page.getByText("Available!")).toBeVisible({ timeout: 15_000 });
    await page.waitForTimeout(1_000);
    await page.screenshot({ path: shot("16-search-available-mobile"), fullPage: true });
    await context.close();
  });

  test("register - mobile", async ({ browser }) => {
    const context = await browser.newContext({
      viewport: mobileViewport,
      baseURL: BASE_URL,
    });
    const page = await context.newPage();
    await page.goto("./register?name=myawesomedomain");
    await page.waitForLoadState("networkidle");
    await expect(page.getByRole("heading", { name: "Order Summary" })).toBeVisible({ timeout: 15_000 });
    await page.waitForTimeout(1_000);
    await page.screenshot({ path: shot("17-register-mobile"), fullPage: true });
    await context.close();
  });

  test("records - mobile", async ({ browser }) => {
    const context = await browser.newContext({
      viewport: mobileViewport,
      baseURL: BASE_URL,
    });
    const page = await context.newPage();
    await page.goto("./records");
    await page.waitForLoadState("networkidle");
    await page.waitForTimeout(3_000);
    await page.screenshot({ path: shot("18-records-mobile"), fullPage: true });
    await context.close();
  });

  test("wallet - mobile", async ({ browser }) => {
    const context = await browser.newContext({
      viewport: mobileViewport,
      baseURL: BASE_URL,
    });
    const page = await context.newPage();
    await page.goto("./wallet");
    await page.waitForLoadState("networkidle");
    await expect(page.getByRole("heading", { name: "Wallet" })).toBeVisible({ timeout: 15_000 });
    await page.waitForTimeout(4_000);
    await page.screenshot({ path: shot("19-wallet-mobile"), fullPage: true });
    await context.close();
  });

  test("dashboard - mobile", async ({ browser }) => {
    const context = await browser.newContext({
      viewport: mobileViewport,
      baseURL: BASE_URL,
    });
    const page = await context.newPage();
    await page.goto("./dashboard");
    await page.waitForLoadState("networkidle");
    await expect(page.getByRole("heading", { name: "My Domains" })).toBeVisible({ timeout: 15_000 });
    await page.waitForTimeout(2_000);
    await page.screenshot({ path: shot("20-dashboard-mobile"), fullPage: true });
    await context.close();
  });

  test("learn - mobile", async ({ browser }) => {
    const context = await browser.newContext({
      viewport: mobileViewport,
      baseURL: BASE_URL,
    });
    const page = await context.newPage();
    await page.goto("./learn");
    await page.waitForLoadState("networkidle");
    await page.waitForTimeout(1_500);
    await page.screenshot({ path: shot("21-learn-mobile"), fullPage: true });
    await context.close();
  });

  test("discoveries - mobile", async ({ browser }) => {
    const context = await browser.newContext({
      viewport: mobileViewport,
      baseURL: BASE_URL,
    });
    const page = await context.newPage();
    await page.goto("./discoveries");
    await page.waitForLoadState("networkidle");
    await page.waitForTimeout(1_500);
    await page.screenshot({ path: shot("22-discoveries-mobile"), fullPage: true });
    await context.close();
  });
});

test.describe("Visual QA - Registered Domain States", () => {
  test("domain page - registered domain with records", async ({ page }) => {
    await page.goto("./");
    await page.evaluate(() => {
      localStorage.clear();
      indexedDB.deleteDatabase("coco-cashu");
    });
    await page.goto("./domain?name=e2elivetest");
    await page.waitForLoadState("networkidle");
    await page.waitForTimeout(3_000);
    await page.screenshot({ path: shot("23-domain-registered-desktop"), fullPage: true });
  });

  test("search - registered domain (unavailable)", async ({ page }) => {
    await page.goto("./");
    await page.evaluate(() => {
      localStorage.clear();
      indexedDB.deleteDatabase("coco-cashu");
    });
    await page.goto("./search?q=e2elivetest");
    await page.waitForLoadState("networkidle");
    await page.waitForSelector("text=/Already registered|Available/", { timeout: 15_000 });
    await page.waitForTimeout(500);
    await page.screenshot({ path: shot("24-search-unavailable-desktop"), fullPage: true });
  });

  test("profile - registered domain via npub", async ({ page }) => {
    await page.goto("./");
    await page.evaluate(() => {
      localStorage.clear();
      indexedDB.deleteDatabase("coco-cashu");
    });
    await page.goto("./profile?name=e2elivetest");
    await page.waitForLoadState("networkidle");
    await page.waitForTimeout(3_000);
    await page.screenshot({ path: shot("25-profile-registered-desktop"), fullPage: true });
  });
});
