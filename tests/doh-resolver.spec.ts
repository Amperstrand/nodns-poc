import { test, expect } from "@playwright/test";
import type { APIResponse } from "@playwright/test";

const DOH_ENDPOINT = "https://dns.nodns.shop/dns-query";
const RESOLVER_API = "https://dns.nodns.shop/api/resolver";

function buildDnsQuery(name: string, qtype: number): Buffer {
    const labels = name.split(".");
    let body = Buffer.alloc(12);
    body.writeUInt16BE(0xabcd, 0);
    body.writeUInt16BE(0x0100, 2);
    body.writeUInt16BE(1, 4);
    for (const label of labels) {
        body = Buffer.concat([body, Buffer.from([label.length]), Buffer.from(label, "ascii")]);
    }
    body = Buffer.concat([body, Buffer.from([0])]);
    body = Buffer.concat([body, Buffer.from([0, qtype, 0, 1])]);
    return body;
}

function parseDnsResponse(buf: Buffer): { rcode: number; ancount: number } {
    if (buf.length < 8) return { rcode: -1, ancount: 0 };
    const flags = buf.readUInt16BE(2);
    const ancount = buf.readUInt16BE(6);
    return { rcode: flags & 0x0f, ancount };
}

test.describe("DoH Resolver — Free Tier", () => {
    test("resolves nodns.shop SOA", async ({ request }) => {
        const query = buildDnsQuery("nodns.shop", 6);
        const resp: APIResponse = await request.post(DOH_ENDPOINT, {
            headers: { "Content-Type": "application/dns-message" },
            data: query,
        });
        expect(resp.status()).toBe(200);
        const dns = parseDnsResponse(await resp.body());
        expect(dns.rcode).toBe(0);
        expect(dns.ancount).toBeGreaterThanOrEqual(1);
    });

    test("resolves dns4sats.xyz SOA", async ({ request }) => {
        const query = buildDnsQuery("dns4sats.xyz", 6);
        const resp = await request.post(DOH_ENDPOINT, {
            headers: { "Content-Type": "application/dns-message" },
            data: query,
        });
        expect(resp.status()).toBe(200);
        const dns = parseDnsResponse(await resp.body());
        expect(dns.rcode).toBe(0);
        expect(dns.ancount).toBeGreaterThanOrEqual(1);
    });

    test("returns DNSKEY for nodns.shop (DNSSEC)", async ({ request }) => {
        const query = buildDnsQuery("nodns.shop", 48);
        const resp = await request.post(DOH_ENDPOINT, {
            headers: { "Content-Type": "application/dns-message" },
            data: query,
        });
        expect(resp.status()).toBe(200);
        const dns = parseDnsResponse(await resp.body());
        expect(dns.rcode).toBe(0);
        expect(dns.ancount).toBeGreaterThanOrEqual(1);
    });

    test("REFUSES google.com (browser fallback)", async ({ request }) => {
        const query = buildDnsQuery("google.com", 1);
        const resp = await request.post(DOH_ENDPOINT, {
            headers: { "Content-Type": "application/dns-message" },
            data: query,
        });
        expect(resp.status()).toBe(200);
        const dns = parseDnsResponse(await resp.body());
        expect(dns.rcode).toBe(5);
        expect(dns.ancount).toBe(0);
    });

    test("REFUSES example.org, github.com, cloudflare.com", async ({ request }) => {
        for (const domain of ["example.org", "github.com", "cloudflare.com"]) {
            const query = buildDnsQuery(domain, 1);
            const resp = await request.post(DOH_ENDPOINT, {
                headers: { "Content-Type": "application/dns-message" },
                data: query,
            });
            expect(resp.status()).toBe(200);
            const dns = parseDnsResponse(await resp.body());
            expect(dns.rcode).toBe(5);
        }
    });
});

test.describe("DoH Resolver — Premium Tier (gating)", () => {
    test("returns 402 without subscription", async ({ request }) => {
        const query = buildDnsQuery("google.com", 1);
        const resp = await request.post(`${DOH_ENDPOINT}/premium`, {
            headers: { "Content-Type": "application/dns-message" },
            data: query,
        });
        expect(resp.status()).toBe(402);
    });

    test("returns 402 with fake subscription token", async ({ request }) => {
        const query = buildDnsQuery("google.com", 1);
        const resp = await request.post(`${DOH_ENDPOINT}/premium`, {
            headers: {
                "Content-Type": "application/dns-message",
                "X-Subscription": "fake-token-12345",
            },
            data: query,
        });
        expect(resp.status()).toBe(402);
    });

    test("returns 402 with empty subscription", async ({ request }) => {
        const query = buildDnsQuery("google.com", 1);
        const resp = await request.post(`${DOH_ENDPOINT}/premium`, {
            headers: {
                "Content-Type": "application/dns-message",
                "X-Subscription": "",
            },
            data: query,
        });
        expect(resp.status()).toBe(402);
    });
});

test.describe("Subscribe — NUT-24 Payment Challenge", () => {
    test("returns 402 with X-Cashu header (NUT-18 creqA)", async ({ request }) => {
        const resp = await request.post(`${RESOLVER_API}/subscribe`, {});
        expect(resp.status()).toBe(402);
        const cashuHeader = resp.headers()["x-cashu"];
        expect(cashuHeader).toBeTruthy();
        expect(cashuHeader).toContain("creqA");
    });

    test("402 body has accepts.cashu JSON", async ({ request }) => {
        const resp = await request.post(`${RESOLVER_API}/subscribe`, {});
        expect(resp.status()).toBe(402);
        const body = await resp.json();
        expect(body.accepts).toBeTruthy();
        expect(body.accepts.cashu).toBeTruthy();
        expect(body.accepts.cashu.amount).toBeGreaterThan(0);
        expect(body.accepts.cashu.unit).toBe("sat");
        expect(body.accepts.cashu.mint).toContain("://");
    });

    test("rejects invalid Cashu token with 400", async ({ request }) => {
        const resp = await request.post(`${RESOLVER_API}/subscribe`, {
            headers: { "X-Cashu": "cashuBinvalidtoken" },
        });
        expect(resp.status()).toBe(400);
    });

    test("rejects garbage token with 400", async ({ request }) => {
        const resp = await request.post(`${RESOLVER_API}/subscribe`, {
            headers: { "X-Cashu": "garbage" },
        });
        expect(resp.status()).toBe(400);
    });
});

test.describe("Stats and Status", () => {
    test("stats endpoint returns resolver state", async ({ request }) => {
        const resp = await request.get(`${RESOLVER_API}/stats`);
        expect(resp.status()).toBe(200);
        const body = await resp.json();
        expect(body).toHaveProperty("active_subscriptions");
        expect(body).toHaveProperty("total_subscriptions");
        expect(body).toHaveProperty("queries_today");
        expect(body).toHaveProperty("resolver_enabled", true);
    });

    test("status returns 400 without token", async ({ request }) => {
        const resp = await request.get(`${RESOLVER_API}/status`);
        expect(resp.status()).toBe(400);
    });

    test("status returns 404 with nonexistent token", async ({ request }) => {
        const resp = await request.get(`${RESOLVER_API}/status`, {
            headers: { "X-Subscription": "nonexistent-token" },
        });
        expect(resp.status()).toBe(404);
    });
});

test.describe("NIP-05 Identity Verification", () => {
    test("domain owner (_ ) resolves", async ({ request }) => {
        const resp = await request.get(
            "https://nodns.shop/.well-known/nostr.json?name=_"
        );
        expect(resp.status()).toBe(200);
        const body = await resp.json();
        expect(body.names).toHaveProperty("_");
        expect(body.names._).toMatch(/^[0-9a-f]{64}$/);
    });

    test("queried name is used as key (not underscore)", async ({ request }) => {
        const testNpub = "npub1hw6amg8p24ne08c9gdq8hhpqx0t0pwanpae9z25crn7m9uy7yarse465gr";
        const resp = await request.get(
            `https://nodns.shop/.well-known/nostr.json?name=${testNpub}`
        );
        expect(resp.status()).toBe(200);
        const body = await resp.json();
        const keys = Object.keys(body.names);
        expect(keys).toContain(testNpub);
        expect(keys).not.toContain("_");
    });

    test("unknown name returns empty names", async ({ request }) => {
        const resp = await request.get(
            "https://nodns.shop/.well-known/nostr.json?name=nonexistent12345"
        );
        expect(resp.status()).toBe(200);
        const body = await resp.json();
        expect(Object.keys(body.names)).toHaveLength(0);
    });
});

test.describe("Landing Page", () => {
    test("dns.nodns.shop serves resolver landing page", async ({ request }) => {
        const resp = await request.get("https://dns.nodns.shop/");
        expect(resp.status()).toBe(200);
        const body = await resp.text();
        expect(body).toContain("DoH Resolver");
        expect(body).toContain("Free");
        expect(body).toContain("Premium");
        expect(body).toContain("dns.nodns.shop/dns-query");
    });

    test("landing page has subscribe form with Cashu token input", async ({ request }) => {
        const resp = await request.get("https://dns.nodns.shop/");
        const body = await resp.text();
        expect(body).toContain("Subscribe");
        expect(body).toContain("cashuB");
        expect(body).toContain("doh-proxy.py");
    });
});

test.describe("Health and Regression", () => {
    test("bot health returns 200 with ok status", async ({ request }) => {
        const resp = await request.get("https://nodns.shop/api/health");
        expect(resp.status()).toBe(200);
        const body = await resp.json();
        expect(body.status).toBe("ok");
        expect(body).toHaveProperty("resolver");
        expect(body.resolver.enabled).toBe(true);
    });

    test("open resolver stays closed (google.com REFUSED on port 53)", async () => {
        const query = buildDnsQuery("google.com", 1);
        const resp = await fetch(`https://dns.nodns.shop/dns-query`, {
            method: "POST",
            headers: { "Content-Type": "application/dns-message" },
            body: query,
        });
        const dns = parseDnsResponse(Buffer.from(await resp.arrayBuffer()));
        expect(dns.rcode).toBe(5);
    });
});
