const API_BASE = "";

export interface AcmeOrderRequest {
  domain: string;
}

export interface AcmeOrderResponse {
  order_id: string;
  status: string;
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
}

export async function requestCertificate(
  domain: string,
): Promise<AcmeOrderResponse> {
  const res = await fetch(`${API_BASE}/api/acme/order`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ domain }),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: "Request failed" }));
    throw new Error(err.error || `HTTP ${res.status}`);
  }
  return res.json();
}

export async function getCertificateOrder(
  orderId: string,
): Promise<AcmeOrderStatus> {
  const res = await fetch(`${API_BASE}/api/acme/order/${orderId}`);
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}
