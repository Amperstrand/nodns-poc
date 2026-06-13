import { test, expect } from "@playwright/test";

test.describe("Learn Page", () => {
  test("renders architecture section", async ({ page }) => {
    await page.goto("./learn");
    await expect(
      page.getByRole("heading", { name: /Architecture/i })
    ).toBeVisible({ timeout: 10_000 });
  });

  test("renders consensus rules section", async ({ page }) => {
    await page.goto("./learn");
    await expect(
      page.getByRole("heading", { name: "Consensus Rules" })
    ).toBeVisible({ timeout: 10_000 });
  });

  test("renders protocol spec section", async ({ page }) => {
    await page.goto("./learn");
    await expect(
      page.getByRole("heading", { name: "Protocol Specification" })
    ).toBeVisible();
  });

  test("renders roadmap section", async ({ page }) => {
    await page.goto("./learn");
    await expect(
      page.getByText(/Roadmap/i)
    ).toBeVisible();
  });

  test("no console errors", async ({ page }) => {
    const errors: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error") {
        const text = msg.text();
        if (
          !text.includes("Failed to load resource") &&
          !text.includes("Minified React error")
        ) {
          errors.push(text);
        }
      }
    });
    await page.goto("./learn");
    await page.waitForTimeout(2_000);
    expect(errors.length).toBe(0);
  });
});

test.describe("Discoveries Page", () => {
  test("renders heading and subtitle", async ({ page }) => {
    await page.goto("./discoveries");
    await expect(
      page.getByRole("heading", { name: /Discoveries/i })
    ).toBeVisible({ timeout: 10_000 });
  });

  test("renders zone-agnostic wire format section", async ({ page }) => {
    await page.goto("./discoveries");
    await expect(
      page.getByRole("heading", { name: /Zone-Agnostic/i })
    ).toBeVisible({ timeout: 10_000 });
  });

  test("renders .nostr TLD section", async ({ page }) => {
    await page.goto("./discoveries");
    await expect(
      page.getByRole("heading", { name: /\.nostr/i })
    ).toBeVisible({ timeout: 10_000 });
  });

  test("no console errors", async ({ page }) => {
    const errors: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error") {
        const text = msg.text();
        if (
          !text.includes("Failed to load resource") &&
          !text.includes("Minified React error")
        ) {
          errors.push(text);
        }
      }
    });
    await page.goto("./discoveries");
    await page.waitForTimeout(2_000);
    expect(errors.length).toBe(0);
  });
});
