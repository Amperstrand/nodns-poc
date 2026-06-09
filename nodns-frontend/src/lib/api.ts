const API_BASE = "";

/** Default request timeout in milliseconds. */
const REQUEST_TIMEOUT_MS = 30_000;

/**
 * Sanitize an error message so we never expose raw server internals to the user.
 * If the message looks like a generic HTTP error, pass it through.
 * Otherwise, return a safe fallback.
 */
function sanitizeErrorMessage(msg: string): string {
  // Allow known safe patterns
  if (/^HTTP \d{3}/.test(msg)) return msg;
  if (msg === "Request failed") return msg;
  if (msg === "Request timed out. Please try again.") return msg;
  if (msg === "Unable to connect. Please check your network.") return msg;
  // For anything else, return a generic message
  return "An unexpected error occurred. Please try again.";
}

/**
 * Create an AbortController that fires after REQUEST_TIMEOUT_MS.
 */
function timeoutController(): AbortController {
  const controller = new AbortController();
  setTimeout(() => controller.abort(), REQUEST_TIMEOUT_MS);
  return controller;
}

/**
 * Wrapper around fetch that enforces timeout, catches network errors,
 * and sanitizes error messages for user display.
 */
async function safeFetch(
  url: string,
  init: RequestInit,
): Promise<Response> {
  const controller = timeoutController();
  const combinedInit: RequestInit = {
    ...init,
    signal: controller.signal,
  };

  let response: Response;
  try {
    response = await fetch(url, combinedInit);
  } catch (err: unknown) {
    if (err instanceof DOMException && err.name === "AbortError") {
      throw new Error("Request timed out. Please try again.");
    }
    throw new Error("Unable to connect. Please check your network.");
  }

  if (!response.ok) {
    let userMessage: string;
    try {
      const body = await response.json();
      const raw: string =
        typeof body === "object" && body !== null && typeof body.error === "string"
          ? body.error
          : "";
      // Only surface a sanitized subset of server messages
      if (raw && raw.length < 200 && !raw.includes("<") && !raw.includes("stack")) {
        userMessage = raw;
      } else {
        userMessage = `HTTP ${response.status}`;
      }
    } catch {
      userMessage = `HTTP ${response.status}`;
    }
    throw new Error(sanitizeErrorMessage(userMessage));
  }

  return response;
}

export interface AcmeOrderRequest {
  domain: string;
  csr_der?: string;
  environment?: string;
  ca?: string;
}

export interface AcmeOrderResponse {
  order_id: string;
  status: string;
}

export interface AcmeLogEntry {
  created_at: number;
  stage: string;
  message: string;
  details?: string;
}

export interface AcmeOrderStatus {
  order_id: string;
  status:
    | "pending"
    | "challenge_published"
    | "verifying"
    | "issued"
    | "failed";
  domain: string;
  certificate_pem: string | null;
  private_key_pem: string | null;
  error: string | null;
  acme_environment: string;
  logs: AcmeLogEntry[];
}

export async function requestCertificate(
  domain: string,
  csrDer?: string,
  environment?: string,
  ca?: string,
  npub?: string,
): Promise<AcmeOrderResponse> {
  const body: AcmeOrderRequest = { domain };
  if (csrDer) {
    body.csr_der = csrDer;
  }
  if (environment) {
    body.environment = environment;
  }
  if (ca) {
    body.ca = ca;
  }
  const headers: Record<string, string> = { "Content-Type": "application/json" };
  if (npub) {
    headers["X-Nostr-Npub"] = npub;
  }
  const res = await safeFetch(`${API_BASE}/api/acme/order`, {
    method: "POST",
    headers,
    body: JSON.stringify(body),
  });
  return res.json();
}

export async function getCertificateOrder(
  orderId: string,
  npub?: string,
): Promise<AcmeOrderStatus> {
  const headers: Record<string, string> = {};
  if (npub) {
    headers["X-Nostr-Npub"] = npub;
  }
  const res = await safeFetch(`${API_BASE}/api/acme/order/${orderId}`, { headers });
  return res.json();
}

export interface ZonePricing {
  zone: string;
  enabled: boolean;
  create_price: number;
  update_price: number;
  delete_price: number;
  npub_names_free: boolean;
  mint_url: string;
  mint_filter: string;
}

export async function fetchZonePricing(zone: string): Promise<ZonePricing> {
  const res = await safeFetch(`${API_BASE}/api/zones/${encodeURIComponent(zone)}/pricing`, {
    headers: {},
  });
  return res.json();
}
