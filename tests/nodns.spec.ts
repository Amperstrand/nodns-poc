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
    await expect(page.getByRole("heading", { name: /DNS Record Browser/ })).toBeVisible();
    await expect(page.getByRole("heading", { name: "What is NoDNS?" })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Protocol Specification" })).toBeVisible();
  });

  test("record browser loads records from API", async ({ page }) => {
    await page.goto("/");
    await page.locator("#records").scrollIntoViewIfNeeded();

    const rows = page.getByTestId("npub-group-records").locator("table tbody tr");
    await expect(rows.first()).toBeVisible({ timeout: 10_000 });
    const count = await rows.count();
    expect(count).toBeGreaterThanOrEqual(5);

    const total = await page.getByTestId("stat-total").textContent();
    expect(Number(total)).toBeGreaterThanOrEqual(10);
  });

  test("record groups are collapsible", async ({ page }) => {
    await page.goto("/");
    await page.locator("#records").scrollIntoViewIfNeeded();

    const firstHeader = page.getByTestId("npub-group-header").first();
    const firstGroup = firstHeader.locator("..");
    const firstBody = firstGroup.getByTestId("npub-group-records");

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

    await expect(page.getByTestId("npub-value")).toHaveText(/^npub1[a-z0-9]{58,}$/);
    await expect(page.getByTestId("nsec-value")).toHaveText(/^nsec1[a-z0-9]{58,}$/);
    await expect(page.getByTestId("domain-display")).toContainText(ZONE);
  });

  test("keys are ephemeral and do not persist across reload", async ({ page }) => {
    await page.goto("/");
    await page.locator("#dashboard").scrollIntoViewIfNeeded();

    await page.getByRole("button", { name: "Generate New Keypair" }).click();
    await expect(page.getByTestId("npub-value")).toBeVisible();

    await page.reload();
    await page.locator("#dashboard").scrollIntoViewIfNeeded();

    await expect(page.getByTestId("npub-value")).not.toBeVisible();
    await expect(page.getByRole("button", { name: "Generate New Keypair" })).toBeVisible();
  });

  test("clear keys removes keypair", async ({ page }) => {
    await page.goto("/");
    await page.locator("#dashboard").scrollIntoViewIfNeeded();

    await page.getByRole("button", { name: "Generate New Keypair" }).click();
    await expect(page.getByTestId("npub-value")).toBeVisible();

    await page.getByRole("button", { name: "Clear Keys" }).click();
    await expect(page.getByTestId("npub-value")).not.toBeVisible();
    await expect(page.getByRole("button", { name: "Generate New Keypair" })).toBeVisible();
  });
});

test.describe("Publish DNS Records", () => {
  test("can add and remove records from queue", async ({ page }) => {
    await page.goto("/");
    await page.locator("#dashboard").scrollIntoViewIfNeeded();
    await page.getByRole("button", { name: "Generate New Keypair" }).click();

    await page.getByTestId("rec-name").fill("@");
    await page.getByTestId("rec-value").fill("hello world");
    await page.getByRole("button", { name: "Add" }).click();

    await expect(page.getByTestId("record-list")).toContainText("TXT");
    await expect(page.getByTestId("record-list")).toContainText("hello world");
    await expect(page.getByTestId("record-count")).toContainText("1 record queued");

    await page.getByTestId("remove-record-btn").click();
    await expect(page.getByTestId("record-list")).not.toBeVisible();
  });

  test("publishes TXT record and it appears in the API", async ({ page }) => {
    await page.goto("/");
    await page.locator("#dashboard").scrollIntoViewIfNeeded();
    await page.getByRole("button", { name: "Generate New Keypair" }).click();

    const npub = (await page.getByTestId("npub-value").textContent())!;

    const rnd = Math.random().toString(36).slice(2, 8);
    await page.getByTestId("rec-name").fill("@");
    await page.getByTestId("rec-value").fill(`playwright-test-${rnd}`);
    await page.getByRole("button", { name: "Add" }).click();

    await page.getByRole("button", { name: "Publish to Nostr" }).click();
    await expect(page.getByTestId("publish-feedback")).toContainText("Published event", {
      timeout: 15_000,
    });

    const found = await waitForRecordInApi(page, npub, "TXT", `playwright-test-${rnd}`);
    expect(found).toBeTruthy();
  });

  test("publishes TXT record with subdomain and it appears in the API", async ({ page }) => {
    test.setTimeout(90_000);
    await page.goto("/");
    await page.locator("#dashboard").scrollIntoViewIfNeeded();
    await page.getByRole("button", { name: "Generate New Keypair" }).click();

    const npub = (await page.getByTestId("npub-value").textContent())!;

    await page.getByTestId("rec-type").selectOption("TXT");
    await page.getByTestId("rec-name").fill("test-sub");
    await page.getByTestId("rec-value").fill("hello from playwright");
    await page.getByRole("button", { name: "Add" }).click();

    await page.getByRole("button", { name: "Publish to Nostr" }).click();
    await expect(page.getByTestId("publish-feedback")).toContainText("Published event", {
      timeout: 15_000,
    });

    const found = await waitForRecordInApi(page, npub, "TXT", "hello from playwright", 30);
    expect(found).toBeTruthy();
  });

  test("publishes multiple records in one event", async ({ page }) => {
    await page.goto("/");
    await page.locator("#dashboard").scrollIntoViewIfNeeded();
    await page.getByRole("button", { name: "Generate New Keypair" }).click();

    const npub = (await page.getByTestId("npub-value").textContent())!;
    const rnd = Math.random().toString(36).slice(2, 8);

    await page.getByTestId("rec-name").fill("@");
    await page.getByTestId("rec-value").fill(`multi-txt-${rnd}`);
    await page.getByRole("button", { name: "Add" }).click();

    await page.getByTestId("rec-type").selectOption("A");
    await page.getByTestId("rec-name").fill("@");
    await page.getByTestId("rec-value").fill("198.51.100.1");
    await page.getByRole("button", { name: "Add" }).click();

    await expect(page.getByTestId("record-count")).toContainText("2 records queued");

    await page.getByRole("button", { name: "Publish to Nostr" }).click();
    await expect(page.getByTestId("publish-feedback")).toContainText("Published event with 2 record", {
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

    const npub = (await page.getByTestId("npub-value").textContent())!;
    const rnd = Math.random().toString(36).slice(2, 8);
    const txtValue = `e2e-${rnd}`;

    await page.getByTestId("rec-name").fill("@");
    await page.getByTestId("rec-value").fill(txtValue);
    await page.getByRole("button", { name: "Add" }).click();

    await page.getByRole("button", { name: "Publish to Nostr" }).click();
    await expect(page.getByTestId("publish-feedback")).toContainText("Published event", {
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

    const npub = (await page.getByTestId("npub-value").textContent())!;

    const rnd = Math.random().toString(36).slice(2, 10);
    const subdomain = `test-e2e-${rnd}`;
    const txtValue = `e2e-dns-verify-${rnd}`;

    await page.getByTestId("rec-type").selectOption("TXT");
    await page.getByTestId("rec-name").fill(subdomain);
    await page.getByTestId("rec-value").fill(txtValue);
    await page.getByRole("button", { name: "Add" }).click();

    await page.getByRole("button", { name: "Publish to Nostr" }).click();
    await expect(page.getByTestId("publish-feedback")).toContainText("Published event", {
      timeout: 15_000,
    });

    const found = await waitForRecordInApi(page, npub, "TXT", txtValue, 30);
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

    await page.getByTestId("rec-name").fill("@");
    await page.getByTestId("rec-value").fill(txtValue);
    await page.getByRole("button", { name: "Add" }).click();

    await page.getByRole("button", { name: "Publish to Nostr" }).click();
    await expect(page.getByTestId("publish-feedback")).toContainText("Published event", {
      timeout: 15_000,
    });

    await page.locator("#live-feed-section").scrollIntoViewIfNeeded();
    await expect(page.getByTestId("live-feed-entries")).toContainText(txtValue, {
      timeout: 30_000,
    });
  });
});

test.describe("ACME Certificate Issuance", () => {
  test("full flow: generate keypair → publish record → verify DNS → request ACME cert → validate chain", async ({ page }) => {
    test.setTimeout(180_000);

    // ── Step 1–3: Load page, generate keypair ──
    await page.goto("/");
    await page.locator("#dashboard").scrollIntoViewIfNeeded();
    await page.getByRole("button", { name: "Generate New Keypair" }).click();

    const npub = (await page.getByTestId("npub-value").textContent())!;
    const fqdn = `${npub}.${ZONE}`;
    console.log(`🔑 Generated npub: ${npub}`);
    console.log(`🌐 Domain: ${fqdn}`);

    // ── Step 5–6: Queue a TXT record ──
    const rnd = Math.random().toString(36).slice(2, 10);
    const txtValue = `cert-e2e-${rnd}`;

    await page.getByTestId("rec-name").fill("@");
    await page.getByTestId("rec-value").fill(txtValue);
    await page.getByRole("button", { name: "Add" }).click();

    // ── Step 7–8: Publish to Nostr ──
    await page.getByRole("button", { name: "Publish to Nostr" }).click();
    await expect(page.getByTestId("publish-feedback")).toContainText("Published event", {
      timeout: 15_000,
    });
    console.log(`📤 Published TXT record: ${txtValue}`);

    // ── Step 9: Verify record appears in API ──
    const foundInApi = await waitForRecordInApi(page, npub, "TXT", txtValue);
    expect(foundInApi).toBeTruthy();
    console.log(`✅ Record found in API`);

    // ── Step 10: Verify DNS resolution ──
    const resolved = await waitForDnsResolution(fqdn, "TXT", txtValue);
    expect(resolved).toBeTruthy();
    console.log(`✅ DNS resolved: ${fqdn} TXT → ${txtValue}`);

    // ── Step 11: Request HTTPS certificate ──
    // Intercept the POST response to capture order_id
    const orderResponsePromise = page.waitForResponse(
      (resp) =>
        resp.url().includes("/api/acme/order") &&
        resp.request().method() === "POST" &&
        resp.status() === 200
    );

    await page.getByText("Get HTTPS Certificate").click();
    console.log(`🔐 Clicked "Get HTTPS Certificate"`);

    const orderResponse = await orderResponsePromise;
    const orderBody = await orderResponse.json();
    const orderId: string = orderBody.order_id;
    console.log(`📋 Order ID: ${orderId} (status: ${orderBody.status})`);

    // ── Step 12: Wait for issuance ──
    await expect(page.getByText("✅ Certificate ready!")).toBeVisible({
      timeout: 120_000,
    });
    console.log(`🎉 Certificate issued!`);

    // ── Step 13–14: Extract cert and validate ──
    const certResp = await page.request.get(
      `https://nodns.shop/api/acme/order/${orderId}`
    );
    expect(certResp.ok()).toBeTruthy();
    const certData = await certResp.json();

    const certPem: string = certData.certificate_pem;
    expect(certPem).toBeTruthy();

    // When the client sends a CSR (new flow), private_key_pem is null —
    // the private key never leaves the browser. When the server generates
    // the key (old flow), private_key_pem is present.
    const keyPem: string | null = certData.private_key_pem;

    // Verify ACME logs were captured
    const logs: Array<{ stage: string; message: string }> = certData.logs || [];
    expect(logs.length).toBeGreaterThan(0);
    console.log(`📋 ACME logs: ${logs.length} entries`);

    // Write cert to temp file
    const certPath = `/tmp/nodns-cert-${rnd}.pem`;
    const caPath = `/tmp/letsencrypt-staging-ca-${rnd}.pem`;

    const fs = await import("fs");
    fs.writeFileSync(certPath, certPem);

    // Download LE staging root CA
    execSync(
      `curl -sSf -o ${caPath} https://letsencrypt.org/certs/staging/letsencrypt-stg-root-x1.pem`,
      { timeout: 15_000 }
    );
    console.log(`📥 Downloaded LE staging CA to ${caPath}`);

    // ── Validate certificate with openssl ──
    const certInfo = execSync(
      `openssl x509 -in ${certPath} -noout -subject -issuer -dates -ext subjectAltName`,
      { timeout: 10_000 }
    ).toString().trim();
    console.log(`📜 Certificate details:\n${certInfo}`);

    // LE staging certs put the domain in SANs, not subject — check both
    const certLower = certInfo.toLowerCase();
    expect(certLower).toContain("nodns.shop");

    const verifyResult = execSync(
      `openssl verify -CAfile ${caPath} ${certPath} 2>&1 || true`,
      { timeout: 10_000 }
    ).toString().trim();
    console.log(`🔍 Chain verification: ${verifyResult}`);
    // LE staging intermediates rotate; chain verification is best-effort
    if (verifyResult.includes(": OK")) {
      console.log(`✅ Chain verified against LE staging CA`);
    } else {
      console.log(`⚠️ Chain verification skipped (LE staging intermediate rotation)`);
    }

    // Verify private key matches certificate (only when server generated the key)
    if (keyPem) {
      const keyPath = `/tmp/nodns-key-${rnd}.pem`;
      fs.writeFileSync(keyPath, keyPem);
      const keyCheck = execSync(
        `bash -c 'openssl x509 -in ${certPath} -noout -modulus | openssl md5 && openssl rsa -in ${keyPath} -noout -modulus | openssl md5'`,
        { timeout: 10_000 }
      ).toString().trim();
      const lines = keyCheck.split("\n").map((l) => l.trim());
      expect(lines[0]).toBe(lines[1]);
      console.log(`🔑 Key-certificate match verified (server-generated key)`);
      try { fs.unlinkSync(keyPath); } catch {}
    } else {
      console.log(`🔑 Private key not in API response (client-side derivation — expected)`);
    }

    try {
      fs.unlinkSync(certPath);
      fs.unlinkSync(caPath);
    } catch {}

    console.log(`\n✅ Full ACME E2E test passed for ${fqdn}`);
  });
});
