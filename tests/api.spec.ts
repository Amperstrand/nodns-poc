import { test, expect } from "@playwright/test";

const API = "https://nodns.shop";

test.describe("API Endpoints", () => {
  test("/api/records returns valid JSON with records array", async ({
    request,
  }) => {
    const resp = await request.get(`${API}/api/records`);
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body).toHaveProperty("records");
    expect(Array.isArray(body.records)).toBeTruthy();
    expect(body.records.length).toBeGreaterThanOrEqual(1);
  });

  test("/api/records?pubkey= filters results", async ({ request }) => {
    const resp = await request.get(
      `${API}/api/records?pubkey=0000000000000000000000000000000000000000000000000000000000000001`
    );
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body).toHaveProperty("records");
    expect(Array.isArray(body.records)).toBeTruthy();
  });

  test("/api/check returns availability status", async ({ request }) => {
    const resp = await request.get(`${API}/api/check?name=test`);
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
    const resp = await request.get(`${API}/api/zones/nodns.shop/pricing`);
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body).toHaveProperty("zone");
    expect(body).toHaveProperty("create_price");
    expect(body).toHaveProperty("update_price");
    expect(body).toHaveProperty("delete_price");
    expect(body.zone).toBe("nodns.shop");
  });
});
