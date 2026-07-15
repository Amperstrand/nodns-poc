import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { createApiClient, ApiError, NetworkError } from './api-client.js';

const mockResponse = (status: number, body: unknown) => ({
  ok: status >= 200 && status < 300,
  status,
  json: () => Promise.resolve(body),
  headers: new Headers(),
}) as Response;

describe('createApiClient', () => {
  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn());
    vi.stubGlobal('AbortController', class {
      signal = { aborted: false };
      abort() { (this.signal as { aborted: boolean }).aborted = true; }
    });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('returns object with get/post/put/delete methods', () => {
    const api = createApiClient({ baseUrl: 'https://example.com' });
    expect(typeof api.get).toBe('function');
    expect(typeof api.post).toBe('function');
    expect(typeof api.put).toBe('function');
    expect(typeof api.delete).toBe('function');
  });

  it('makes GET request to correct URL', async () => {
    vi.mocked(fetch).mockResolvedValueOnce(mockResponse(200, { ok: true }));
    const api = createApiClient({ baseUrl: 'https://nodns.shop' });
    await api.get('/api/health');
    expect(fetch).toHaveBeenCalledWith(
      'https://nodns.shop/api/health',
      expect.objectContaining({ method: 'GET' }),
    );
  });

  it('makes POST with JSON body', async () => {
    vi.mocked(fetch).mockResolvedValueOnce(mockResponse(200, { id: '123' }));
    const api = createApiClient({ baseUrl: 'https://nodns.shop' });
    const result = await api.post('/api/records', { name: 'alice' });
    expect(result).toEqual({ id: '123' });
    const [, init] = vi.mocked(fetch).mock.calls[0];
    expect(init?.method).toBe('POST');
    expect(init?.body).toBe(JSON.stringify({ name: 'alice' }));
  });

  it('adds query params for GET', async () => {
    vi.mocked(fetch).mockResolvedValueOnce(mockResponse(200, {}));
    const api = createApiClient({ baseUrl: 'https://nodns.shop' });
    await api.get('/api/check', { params: { name: 'alice', zone: 'nodns.shop' } });
    const url = vi.mocked(fetch).mock.calls[0]?.[0] as string;
    expect(url).toContain('name=alice');
    expect(url).toContain('zone=nodns.shop');
  });

  it('injects X-Nostr-Npub from getNpub getter', async () => {
    vi.mocked(fetch).mockResolvedValueOnce(mockResponse(200, {}));
    const api = createApiClient({
      baseUrl: 'https://nodns.shop',
      getNpub: () => 'npub1test',
    });
    await api.get('/api/records');
    const init = vi.mocked(fetch).mock.calls[0]?.[1] as RequestInit;
    expect((init.headers as Record<string, string>)['X-Nostr-Npub']).toBe('npub1test');
  });

  it('allows per-request authHeader override', async () => {
    vi.mocked(fetch).mockResolvedValueOnce(mockResponse(200, {}));
    const api = createApiClient({
      baseUrl: 'https://nodns.shop',
      getNpub: () => 'default-npub',
    });
    await api.get('/api/premium', { authHeader: 'override-npub' });
    const init = vi.mocked(fetch).mock.calls[0]?.[1] as RequestInit;
    expect((init.headers as Record<string, string>)['X-Nostr-Npub']).toBe('override-npub');
  });

  it('throws ApiError on non-2xx response', async () => {
    vi.mocked(fetch).mockResolvedValueOnce(mockResponse(404, { error: 'not found' }));
    const api = createApiClient({ baseUrl: 'https://nodns.shop' });
    try {
      await api.get('/api/missing');
      expect.fail('should have thrown');
    } catch (e) {
      expect(e).toBeInstanceOf(ApiError);
      expect((e as ApiError).status).toBe(404);
    }
  });

  it('throws NetworkError on fetch TypeError', async () => {
    vi.mocked(fetch).mockRejectedValueOnce(new TypeError('Failed to fetch'));
    const api = createApiClient({ baseUrl: 'https://nodns.shop' });
    await expect(api.get('/api/health')).rejects.toThrow(NetworkError);
  });

  it('sanitizes error messages with HTML', async () => {
    vi.mocked(fetch).mockResolvedValueOnce(
      mockResponse(500, { error: '<html>Internal server error</html>' }),
    );
    const api = createApiClient({ baseUrl: 'https://nodns.shop' });
    await expect(api.get('/api/broken')).rejects.toThrow('unexpected error');
  });

  it('caps long error messages', async () => {
    vi.mocked(fetch).mockResolvedValueOnce(
      mockResponse(500, { error: 'x'.repeat(300) }),
    );
    const api = createApiClient({ baseUrl: 'https://nodns.shop' });
    await expect(api.get('/api/broken')).rejects.toThrow('unexpected error');
  });

  it('passes through safe error messages', async () => {
    vi.mocked(fetch).mockResolvedValueOnce(
      mockResponse(400, { error: 'Invalid name' }),
    );
    const api = createApiClient({ baseUrl: 'https://nodns.shop' });
    await expect(api.get('/api/check')).rejects.toThrow('Invalid name');
  });

  it('sets Content-Type to application/json', async () => {
    vi.mocked(fetch).mockResolvedValueOnce(mockResponse(200, {}));
    const api = createApiClient({ baseUrl: 'https://nodns.shop' });
    await api.get('/api/health');
    const init = vi.mocked(fetch).mock.calls[0]?.[1] as RequestInit;
    expect((init.headers as Record<string, string>)['Content-Type']).toBe('application/json');
  });

  it('allows custom headers per request', async () => {
    vi.mocked(fetch).mockResolvedValueOnce(mockResponse(200, {}));
    const api = createApiClient({ baseUrl: 'https://nodns.shop' });
    await api.post('/api/resolver/subscribe', {}, { headers: { 'X-Cashu': 'token123' } });
    const init = vi.mocked(fetch).mock.calls[0]?.[1] as RequestInit;
    expect((init.headers as Record<string, string>)['X-Cashu']).toBe('token123');
  });
});
