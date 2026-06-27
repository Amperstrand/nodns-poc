/**
 * nodns client-log Worker — always-available error reporting endpoint.
 *
 * Deployed as a standalone Worker (always up, even when the Rust bot is down).
 * Receives POST /api/client-log from registrar/explorer frontends,
 * logs to Workers Observability, and optionally forwards to the bot.
 *
 * Routes:
 *   POST /api/client-log  — receive errors, log them
 *   OPTIONS /api/client-log — CORS preflight
 *   GET /health           — liveness probe
 */

const BOT_FORWARD_URL = "https://nodns.shop/api/client-log";
const MAX_BODY_BYTES = 64 * 1024;
const RATE_LIMIT = 50;
const RATE_WINDOW_MS = 60_000;
const rateBuckets = new Map<string, { count: number; windowStart: number }>();

const CORS_HEADERS: Record<string, string> = {
  "access-control-allow-origin": "*",
  "access-control-allow-methods": "POST, OPTIONS",
  "access-control-allow-headers": "content-type",
  "access-control-expose-headers": "x-correlation-id",
};

function json(data: unknown, status = 200): Response {
  return new Response(JSON.stringify(data), {
    status,
    headers: { "content-type": "application/json", ...CORS_HEADERS },
  });
}

function checkRateLimit(ip: string): boolean {
  const now = Date.now();
  const bucket = rateBuckets.get(ip);
  if (!bucket || now - bucket.windowStart > RATE_WINDOW_MS) {
    rateBuckets.set(ip, { count: 1, windowStart: now });
    return true;
  }
  bucket.count++;
  return bucket.count <= RATE_LIMIT;
}

export default {
  async fetch(request: Request): Promise<Response> {
    const url = new URL(request.url);
    const path = url.pathname;
    const correlationId = request.headers.get("x-correlation-id") ?? crypto.randomUUID();

    if (path === "/health") {
      return json({ status: "ok", timestamp: Date.now() }, 200);
    }

    if (path === "/api/client-log" && request.method === "OPTIONS") {
      return new Response(null, { status: 204, headers: CORS_HEADERS });
    }

    if (path === "/api/client-log" && request.method === "POST") {
      const ip = request.headers.get("cf-connecting-ip") || "unknown";
      if (!checkRateLimit(ip)) {
        return json({ error: "Rate limit exceeded" }, 429);
      }

      const contentLength = parseInt(request.headers.get("content-length") || "0", 10);
      if (contentLength > MAX_BODY_BYTES) {
        return json({ error: "Body too large (max 64KB)" }, 413);
      }

      let body: { errors?: unknown[] };
      try {
        body = await request.json() as typeof body;
      } catch {
        return json({ error: "Invalid JSON" }, 400);
      }

      const errors = Array.isArray(body.errors) ? body.errors : [];
      for (const entry of errors) {
        const err = entry as Record<string, unknown>;
        console.log(JSON.stringify({
          level: "info",
          event: "client_error",
          correlationId,
          type: typeof err.type === "string" ? err.type : "unknown",
          message: typeof err.message === "string" ? err.message.substring(0, 500) : "",
          stack: typeof err.stack === "string" ? err.stack.substring(0, 500) : "",
          url: typeof err.url === "string" ? err.url.substring(0, 200) : "",
          userAgent: typeof err.userAgent === "string" ? err.userAgent.substring(0, 100) : "",
          source: "nodns-worker-proxy",
        }));
      }

      // Best-effort forward to bot (don't block on this)
      const forwardBody = JSON.stringify({ errors });
      fetch(BOT_FORWARD_URL, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: forwardBody,
      }).catch(() => {});

      return json({ ok: true, received: errors.length });
    }

    return json({ error: "Not found" }, 404);
  },
};
