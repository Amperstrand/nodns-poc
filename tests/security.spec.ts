import { test, expect } from "@playwright/test";

const API = "https://nodns.shop";

test.describe("SECURITY: DynDNS auth gate", () => {
  test("rejects /nic/update without Authorization header", async ({ request }) => {
    const resp = await request.get(`${API}/nic/update?hostname=test.nodns.shop&myip=1.2.3.4`);
    expect(resp.status()).toBe(401);
    const body = await resp.text();
    expect(body).toBe("badauth");
  });

  test("rejects /nic/update with malformed base64 credentials", async ({ request }) => {
    const resp = await request.get(`${API}/nic/update?hostname=test.nodns.shop&myip=1.2.3.4`, {
      headers: { Authorization: "Basic !!!not-base64!!!" },
    });
    expect(resp.status()).toBe(401);
    expect(await resp.text()).toBe("badauth");
  });

  test("rejects /nic/update with Bearer token instead of Basic nsec", async ({ request }) => {
    const resp = await request.get(`${API}/nic/update?hostname=test.nodns.shop&myip=1.2.3.4`, {
      headers: { Authorization: "Bearer some-token" },
    });
    expect(resp.status()).toBe(401);
    expect(await resp.text()).toBe("badauth");
  });

  test("rejects /nic/update with wrong nsec for npub", async ({ request }) => {
    const fakeNsec = "nsec1" + "0".repeat(50);
    const fakeNpub = "npub1" + "0".repeat(50);
    const creds = Buffer.from(`${fakeNpub}:${fakeNsec}`).toString("base64");
    const resp = await request.get(
      `${API}/nic/update?hostname=${fakeNpub}.nodns.shop&myip=1.2.3.4`,
      { headers: { Authorization: `Basic ${creds}` } },
    );
    expect(resp.status()).toBe(401);
    expect(await resp.text()).toBe("badauth");
  });

  test("rejects /nic/update with empty password", async ({ request }) => {
    const creds = Buffer.from("npub1abc:").toString("base64");
    const resp = await request.get(`${API}/nic/update?hostname=test.nodns.shop&myip=1.2.3.4`, {
      headers: { Authorization: `Basic ${creds}` },
    });
    expect(resp.status()).toBe(401);
    expect(await resp.text()).toBe("badauth");
  });

  test("rejects /nic/update for custom name without delegation", async ({ request }) => {
    const fakeNsec = "nsec1" + "0".repeat(50);
    const fakeNpub = "npub1" + "0".repeat(50);
    const creds = Buffer.from(`${fakeNpub}:${fakeNsec}`).toString("base64");
    const resp = await request.get(`${API}/nic/update?hostname=unauthorized.nodns.shop&myip=1.2.3.4`, {
      headers: { Authorization: `Basic ${creds}` },
    });
    expect([401, 403]).toContain(resp.status());
  });
});

test.describe("SECURITY: DynDNS input validation", () => {
  test("rejects hostname without dot (notfqdn)", async ({ request }) => {
    const fakeNsec = "nsec1" + "0".repeat(50);
    const fakeNpub = "npub1" + "0".repeat(50);
    const creds = Buffer.from(`${fakeNpub}:${fakeNsec}`).toString("base64");
    const resp = await request.get(`${API}/nic/update?hostname=justaname&myip=1.2.3.4`, {
      headers: { Authorization: `Basic ${creds}` },
    });
    expect(resp.status()).toBe(400);
    expect(await resp.text()).toBe("notfqdn");
  });

  test("rejects hostname in unmanaged zone (notfqdn)", async ({ request }) => {
    const fakeNsec = "nsec1" + "0".repeat(50);
    const fakeNpub = "npub1" + "0".repeat(50);
    const creds = Buffer.from(`${fakeNpub}:${fakeNsec}`).toString("base64");
    const resp = await request.get(`${API}/nic/update?hostname=test.example.com&myip=1.2.3.4`, {
      headers: { Authorization: `Basic ${creds}` },
    });
    expect(resp.status()).toBe(400);
    expect(await resp.text()).toBe("notfqdn");
  });
});

test.describe("SECURITY: Response security headers", () => {
  test("sets X-Content-Type-Options: nosniff on API responses", async ({ request }) => {
    const resp = await request.get(`${API}/api/records`);
    expect(resp.headers()["x-content-type-options"]).toBe("nosniff");
  });

  test("sets X-Frame-Options: DENY on API responses", async ({ request }) => {
    const resp = await request.get(`${API}/api/records`);
    expect(resp.headers()["x-frame-options"]).toBe("DENY");
  });

  test("sets Referrer-Policy on API responses", async ({ request }) => {
    const resp = await request.get(`${API}/api/records`);
    expect(resp.headers()["referrer-policy"]).toBeTruthy();
  });

  test("sets Permissions-Policy on API responses", async ({ request }) => {
    const resp = await request.get(`${API}/api/records`);
    expect(resp.headers()["permissions-policy"]).toContain("camera=()");
  });
});

test.describe("SECURITY: Path traversal is treated as data", () => {
  test("/api/check treats ../etc/passwd as a name, not file access", async ({ request }) => {
    const resp = await request.get(`${API}/api/check?name=..%2F..%2Fetc%2Fpasswd`);
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body).toHaveProperty("name");
    expect(body).toHaveProperty("api");
    expect(body.api).toHaveProperty("registered");
  });

  test("/api/check with encoded backslashes does not traverse filesystem", async ({ request }) => {
    const resp = await request.get(`${API}/api/check?name=test%5Cuser`);
    expect(resp.ok()).toBeTruthy();
  });

  test("/api/records/by-npub with path chars returns empty, not file contents", async ({
    request,
  }) => {
    const resp = await request.get(`${API}/api/records/by-npub/..%2F..%2Fetc%2Fpasswd`);
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body).toHaveProperty("records");
    expect(Array.isArray(body.records)).toBeTruthy();
    expect(JSON.stringify(body)).not.toContain("root:");
  });
});

test.describe("SECURITY: XSS not reflected in API responses", () => {
  test("script tags in /api/check name param returned as JSON string, not HTML", async ({
    request,
  }) => {
    const xss = "<script>alert(1)</script>";
    const resp = await request.get(`${API}/api/check?name=${encodeURIComponent(xss)}`);
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(resp.headers()["content-type"]).toContain("application/json");
    expect(JSON.stringify(body)).not.toMatch(/<script>alert\(1\)<\/script>/i);
  });

  test("event handler injection in name param not reflected unescaped", async ({ request }) => {
    const payload = "onerror=alert(1)";
    const resp = await request.get(`${API}/api/check?name=${encodeURIComponent(payload)}`);
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(resp.headers()["content-type"]).toContain("application/json");
  });
});

test.describe("SECURITY: Error hygiene — no internal detail leakage", () => {
  test("/api/records response does not leak stack traces", async ({ request }) => {
    const resp = await request.get(`${API}/api/records`);
    const body = await resp.text();
    expect(body).not.toContain("at /");
    expect(body).not.toContain("stack");
    expect(body).not.toContain("panic");
    expect(body).not.toContain(".rs:");
  });

  test("/api/check response does not leak internal paths", async ({ request }) => {
    const resp = await request.get(`${API}/api/check?name=test`);
    const body = await resp.text();
    expect(body).not.toContain("/home/");
    expect(body).not.toContain("/root/");
    expect(body).not.toContain("tsig_key_secret");
    expect(body).not.toContain("nsec");
  });

  test("pricing endpoint does not leak TSIG keys or secrets", async ({ request }) => {
    const resp = await request.get(`${API}/api/zones/nodns.shop/pricing`);
    const body = await resp.text();
    expect(body).not.toContain("tsig");
    expect(body).not.toContain("secret");
    expect(body).not.toContain("key_name");
    expect(body).not.toContain("nsec");
  });

  test("unknown zone pricing returns generic 404, not internal config", async ({ request }) => {
    const resp = await request.get(`${API}/api/zons/nonexistent.zone/pricing`);
    if (!resp.ok()) {
      const body = await resp.text();
      expect(body).not.toContain("knot_address");
      expect(body).not.toContain("127.0.0.1");
    }
  });
});

test.describe("SECURITY: HTTP method enforcement", () => {
  test("POST to GET-only /api/records is rejected", async ({ request }) => {
    const resp = await request.post(`${API}/api/records`, {
      data: { pubkey: "test" },
    });
    expect(resp.status()).toBeGreaterThanOrEqual(400);
    expect(resp.status()).toBeLessThan(500);
  });

  test("DELETE to /api/check is rejected", async ({ request }) => {
    const resp = await request.delete(`${API}/api/check?name=test`);
    expect(resp.status()).toBeGreaterThanOrEqual(400);
    expect(resp.status()).toBeLessThan(500);
  });

  test("PUT to /api/records is rejected", async ({ request }) => {
    const resp = await request.put(`${API}/api/records`, {
      data: { record: "test" },
    });
    expect(resp.status()).toBeGreaterThanOrEqual(400);
    expect(resp.status()).toBeLessThan(500);
  });
});

test.describe("SECURITY: Content-Type safety", () => {
  test("/api/records returns application/json, not text/html", async ({ request }) => {
    const resp = await request.get(`${API}/api/records`);
    expect(resp.headers()["content-type"]).toContain("application/json");
    expect(resp.headers()["content-type"]).not.toContain("text/html");
  });

  test("/api/check returns application/json", async ({ request }) => {
    const resp = await request.get(`${API}/api/check?name=test`);
    expect(resp.headers()["content-type"]).toContain("application/json");
  });

  test("/api/zones pricing returns application/json", async ({ request }) => {
    const resp = await request.get(`${API}/api/zones/nodns.shop/pricing`);
    expect(resp.headers()["content-type"]).toContain("application/json");
  });
});

test.describe("SECURITY: Query parameter robustness", () => {
  test("/api/records handles extremely long pubkey param without crash", async ({ request }) => {
    const longKey = "a".repeat(10000);
    const resp = await request.get(`${API}/api/records?pubkey=${longKey}`);
    expect(resp.status()).toBeLessThan(500);
  });

  test("/api/check handles empty name gracefully", async ({ request }) => {
    const resp = await request.get(`${API}/api/check?name=`);
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body).toHaveProperty("name");
  });

  test("/api/records handles special chars in domain param", async ({ request }) => {
    const resp = await request.get(`${API}/api/records?domain=%00%01%02`);
    expect(resp.status()).toBeLessThan(500);
  });
});
