const DEFAULT_NOTARY_URL = 'https://notary.electrum.org/n/api';

export interface NotaryRequestParams {
  event_id: string;
  value_sats: number;
  nonce?: string;
}

export interface NotaryInvoice {
  invoice: string;
  rhash: string;
}

export interface NotaryProof {
  version: number;
  chain: string;
  merkle_index: number;
  merkle_hashes: string[];
  event_id: string;
  nonce: string;
  txid: string;
  leaf_value: number;
  block_height: number;
  upvoter_pubkey?: string;
  upvoter_signature?: string;
}

export async function createBurnRequest(
  params: NotaryRequestParams,
  notaryUrl: string = DEFAULT_NOTARY_URL,
): Promise<NotaryInvoice> {
  const nonce = params.nonce || generateNonce();
  const body = {
    event_id: params.event_id,
    value_sats: params.value_sats,
    nonce,
  };
  const resp = await fetch(`${notaryUrl}/add_request`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  if (!resp.ok) {
    throw new Error(`Notary request failed: ${resp.status} ${resp.statusText}`);
  }
  return resp.json();
}

export async function pollForProof(
  invoice: NotaryInvoice,
  notaryUrl: string = DEFAULT_NOTARY_URL,
  maxAttempts: number = 120,
  intervalMs: number = 3000,
): Promise<NotaryProof> {
  for (let i = 0; i < maxAttempts; i++) {
    const resp = await fetch(`${notaryUrl}/get_proof`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(invoice),
    });
    if (resp.ok) {
      const data = await resp.json();
      if (!data.error) {
        return data as NotaryProof;
      }
    }
    await new Promise((r) => setTimeout(r, intervalMs));
  }
  throw new Error('Timed out waiting for proof');
}

export async function verifyProof(
  proof: NotaryProof,
  notaryUrl: string = DEFAULT_NOTARY_URL,
): Promise<boolean> {
  const resp = await fetch(`${notaryUrl}/verify_proof`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(proof),
  });
  if (!resp.ok) return false;
  const data = await resp.json();
  return data.verified === true || data.confirmations !== undefined;
}

function generateNonce(): string {
  const bytes = new Uint8Array(32);
  crypto.getRandomValues(bytes);
  return Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('');
}

export { DEFAULT_NOTARY_URL };
