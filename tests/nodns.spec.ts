import { test, expect } from "@playwright/test";
import { execSync } from "child_process";

const ZONE = "nodns.shop";

async function waitForRecordInApi(
  page: any,
  npub: string,
  type: string,
  rdata: string,
  maxAttempts = 12
): Promise<boolean> {
  for (let i = 0; i < maxAttempts; i++) {
    await page.waitForTimeout(2000);
    const resp = await page.request.get("https://nodns.shop/api/records");
    const data = await resp.json();
    const match = data.records.find(
      (r: any) => r.npub === npub && r.type === type && r.rdata === rdata
    );
    if (match) return true;
  }
  return false;
}

test.describe("NoDNS Landing Page", () => {
  test("loads and shows key sections", async ({ page }) => {
    await page.goto("/");

    await expect(page.getByRole("heading", { name: "Nostr DNS Dashboard" })).toBeVisible();
    await expect(page.getByRole("heading", { name: "DNS Record Browser" })).toBeVisible();
    await expect(page.getByRole("heading", { name: "What is NoDNS?" })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Protocol Specification" })).toBeVisible();
  });

  test("record browser loads records from API", async ({ page }) => {
    await page.goto("/");
    await page.locator("#records").scrollIntoViewIfNeeded();

    const rows = page.locator(".npub-group-records table tbody tr");
    await expect(rows.first()).toBeVisible({ timeout: 10_000 });
    const count = await rows.count();
    expect(count).toBeGreaterThanOrEqual(5);

    const total = await page.locator("#stat-total").textContent();
    expect(Number(total)).toBeGreaterThanOrEqual(10);
  });

  test("record groups are collapsible", async ({ page }) => {
    await page.goto("/");
    await page.locator("#records").scrollIntoViewIfNeeded();

    const firstHeader = page.locator(".npub-group-header").first();
    const firstBody = page.locator(".npub-group-records").first();

    await expect(firstBody).toBeVisible();
    await firstHeader.click();
    await expect(firstBody).not.toBeVisible();
    await firstHeader.click();
    await expect(firstBody).toBeVisible();
  });

  test("try-it-now section has real resolving domain", async ({ page }) => {
    await page.goto("/");

    const code = page.locator("#try pre code");
    const text = await code.textContent();
    expect(text).toContain("npub190queyng2pmx0jfw5rkx4fjjl3u0zxz6nlyaja53p2n0ydupr6jsdnqt8q");
    expect(text).toContain("185.18.221.10");
    expect(text).not.toContain("anything.nodns.shop");
  });
});

test.describe("Key Generation", () => {
  test("generates a new keypair", async ({ page }) => {
    await page.goto("/");
    await page.locator("#dashboard").scrollIntoViewIfNeeded();

    await page.getByRole("button", { name: "Generate New Keypair" }).click();

    await expect(page.locator("#npub-display")).toHaveText(/^npub1[a-z0-9]{58,}$/);
    await expect(page.locator("#nsec-display")).toHaveText(/^nsec1[a-z0-9]{58,}$/);
    await expect(page.locator("#domain-display")).toContainText(ZONE);
  });

  test("persists keypair in localStorage", async ({ page }) => {
    await page.goto("/");
    await page.locator("#dashboard").scrollIntoViewIfNeeded();

    await page.getByRole("button", { name: "Generate New Keypair" }).click();
    const npub = await page.locator("#npub-display").textContent();

    await page.reload();
    await page.locator("#dashboard").scrollIntoViewIfNeeded();

    await expect(page.locator("#npub-display")).toHaveText(npub!);
  });

  test("clear keys removes keypair", async ({ page }) => {
    await page.goto("/");
    await page.locator("#dashboard").scrollIntoViewIfNeeded();

    await page.getByRole("button", { name: "Generate New Keypair" }).click();
    await expect(page.locator("#npub-display")).toBeVisible();

    await page.getByRole("button", { name: "Clear Keys" }).click();
    await expect(page.locator("#key-display")).not.toBeVisible();
    await expect(page.getByRole("button", { name: "Generate New Keypair" })).toBeVisible();
  });
});

test.describe("Publish DNS Records", () => {
  test("can add and remove records from queue", async ({ page }) => {
    await page.goto("/");
    await page.locator("#dashboard").scrollIntoViewIfNeeded();
    await page.getByRole("button", { name: "Generate New Keypair" }).click();

    await page.locator("#rec-name").fill("@");
    await page.locator("#rec-value").fill("hello world");
    await page.getByRole("button", { name: "Add" }).click();

    await expect(page.locator("#record-list")).toContainText("TXT");
    await expect(page.locator("#record-list")).toContainText("hello world");
    await expect(page.locator("#record-count")).toContainText("1 record queued");

    await page.locator(".record-item .remove-btn").click();
    await expect(page.locator("#record-list")).toBeEmpty();
  });

  test("publishes TXT record and it appears in the API", async ({ page }) => {
    await page.goto("/");
    await page.locator("#dashboard").scrollIntoViewIfNeeded();
    await page.getByRole("button", { name: "Generate New Keypair" }).click();

    const npub = (await page.locator("#npub-display").textContent())!;

    const rnd = Math.random().toString(36).slice(2, 8);
    await page.locator("#rec-name").fill("@");
    await page.locator("#rec-value").fill(`playwright-test-${rnd}`);
    await page.getByRole("button", { name: "Add" }).click();

    await page.getByRole("button", { name: "Publish to Nostr" }).click();
    await expect(page.locator("#publish-feedback")).toContainText("Published event", {
      timeout: 15_000,
    });

    const found = await waitForRecordInApi(page, npub, "TXT", `playwright-test-${rnd}`);
    expect(found).toBeTruthy();
  });

  test("publishes TXT record with subdomain and it appears in the API", async ({ page }) => {
    await page.goto("/");
    await page.locator("#dashboard").scrollIntoViewIfNeeded();
    await page.getByRole("button", { name: "Generate New Keypair" }).click();

    const npub = (await page.locator("#npub-display").textContent())!;

    await page.locator("#rec-type").selectOption("TXT");
    await page.locator("#rec-name").fill("test-sub");
    await page.locator("#rec-value").fill("hello from playwright");
    await page.getByRole("button", { name: "Add" }).click();

    await page.getByRole("button", { name: "Publish to Nostr" }).click();
    await expect(page.locator("#publish-feedback")).toContainText("Published event", {
      timeout: 15_000,
    });

    const found = await waitForRecordInApi(page, npub, "TXT", "hello from playwright");
    expect(found).toBeTruthy();
  });

  test("publishes multiple records in one event", async ({ page }) => {
    await page.goto("/");
    await page.locator("#dashboard").scrollIntoViewIfNeeded();
    await page.getByRole("button", { name: "Generate New Keypair" }).click();

    const npub = (await page.locator("#npub-display").textContent())!;
    const rnd = Math.random().toString(36).slice(2, 8);

    await page.locator("#rec-name").fill("@");
    await page.locator("#rec-value").fill(`multi-txt-${rnd}`);
    await page.getByRole("button", { name: "Add" }).click();

    await page.locator("#rec-type").selectOption("A");
    await page.locator("#rec-name").fill("@");
    await page.locator("#rec-value").fill("198.51.100.1");
    await page.getByRole("button", { name: "Add" }).click();

    await expect(page.locator("#record-count")).toContainText("2 records queued");

    await page.getByRole("button", { name: "Publish to Nostr" }).click();
    await expect(page.locator("#publish-feedback")).toContainText("Published event with 2 record", {
      timeout: 15_000,
    });

    const foundTxt = await waitForRecordInApi(page, npub, "TXT", `multi-txt-${rnd}`);
    const foundA = await waitForRecordInApi(page, npub, "A", "198.51.100.1");
    expect(foundTxt).toBeTruthy();
    expect(foundA).toBeTruthy();
  });
});

async function waitForDnsResolution(
  fqdn: string,
  type: string,
  expectedValue: string,
  maxAttempts = 15
): Promise<boolean> {
  for (let i = 0; i < maxAttempts; i++) {
    try {
      const output = execSync(
        `dig @ns1.nodns.shop ${fqdn} ${type} +short`,
        { timeout: 10000 }
      ).toString().trim();
      if (output.includes(expectedValue)) return true;
    } catch {}
    await new Promise(r => setTimeout(r, 3000));
  }
  return false;
}

test.describe("End-to-End DNS Resolution", () => {
  test("generates key, publishes TXT record, verifies DNS resolution", async ({ page }) => {
    test.setTimeout(120_000);

    await page.goto("/");
    await page.locator("#dashboard").scrollIntoViewIfNeeded();
    await page.getByRole("button", { name: "Generate New Keypair" }).click();

    const npub = (await page.locator("#npub-display").textContent())!;
    const rnd = Math.random().toString(36).slice(2, 8);
    const txtValue = `e2e-${rnd}`;

    await page.locator("#rec-name").fill("@");
    await page.locator("#rec-value").fill(txtValue);
    await page.getByRole("button", { name: "Add" }).click();

    await page.getByRole("button", { name: "Publish to Nostr" }).click();
    await expect(page.locator("#publish-feedback")).toContainText("Published event", {
      timeout: 15_000,
    });

    const found = await waitForRecordInApi(page, npub, "TXT", txtValue);
    expect(found).toBeTruthy();

    const fqdn = `${npub}.${ZONE}`;
    const resolved = await waitForDnsResolution(fqdn, "TXT", txtValue);
    expect(resolved).toBeTruthy();
  });

  test("generates key, publishes TXT subdomain record, verifies DNS resolution", async ({ page }) => {
    test.setTimeout(120_000);

    await page.goto("/");
    await page.locator("#dashboard").scrollIntoViewIfNeeded();
    await page.getByRole("button", { name: "Generate New Keypair" }).click();

    const npub = (await page.locator("#npub-display").textContent())!;

    const rnd = Math.random().toString(36).slice(2, 10);
    const subdomain = `test-e2e-${rnd}`;
    const txtValue = `e2e-dns-verify-${rnd}`;

    await page.locator("#rec-type").selectOption("TXT");
    await page.locator("#rec-name").fill(subdomain);
    await page.locator("#rec-value").fill(txtValue);
    await page.getByRole("button", { name: "Add" }).click();

    await page.getByRole("button", { name: "Publish to Nostr" }).click();
    await expect(page.locator("#publish-feedback")).toContainText("Published event", {
      timeout: 15_000,
    });

    const found = await waitForRecordInApi(page, npub, "TXT", txtValue);
    expect(found).toBeTruthy();

    const fqdn = `${subdomain}.${npub}.${ZONE}`;
    const resolved = await waitForDnsResolution(fqdn, "TXT", txtValue);
    expect(resolved).toBeTruthy();
  });

  test("published event appears in live feed", async ({ page }) => {
    test.setTimeout(60_000);

    await page.goto("/");
    await page.locator("#dashboard").scrollIntoViewIfNeeded();
    await page.getByRole("button", { name: "Generate New Keypair" }).click();

    const rnd = Math.random().toString(36).slice(2, 8);
    const txtValue = `live-feed-${rnd}`;

    await page.locator("#rec-name").fill("@");
    await page.locator("#rec-value").fill(txtValue);
    await page.getByRole("button", { name: "Add" }).click();

    await page.getByRole("button", { name: "Publish to Nostr" }).click();
    await expect(page.locator("#publish-feedback")).toContainText("Published event", {
      timeout: 15_000,
    });

    await page.locator("#live-feed-card").scrollIntoViewIfNeeded();
    await expect(page.locator("#live-feed")).toContainText(txtValue, {
      timeout: 30_000,
    });
  });
});
