export class ApiError extends Error {
  readonly status: number;
  constructor(status: number, message: string) {
    super(sanitizeErrorMessage(message));
    this.name = 'ApiError';
    this.status = status;
  }
}

export class NetworkError extends Error {
  constructor(message = 'Unable to connect. Please check your network.') {
    super(message);
    this.name = 'NetworkError';
  }
}

export class TimeoutError extends Error {
  constructor(message = 'Request timed out. Please try again.') {
    super(message);
    this.name = 'TimeoutError';
  }
}

function sanitizeErrorMessage(msg: string): string {
  if (/^HTTP \d{3}/.test(msg)) return msg;
  if (msg.length < 200 && !msg.includes('<') && !msg.includes('stack')) return msg;
  return 'An unexpected error occurred. Please try again.';
}

export interface ApiClientConfig {
  baseUrl: string;
  timeout?: number;
  getNpub?: () => string | null;
}

export interface RequestOptions {
  params?: Record<string, string | number | boolean | undefined>;
  body?: unknown;
  headers?: Record<string, string>;
  authHeader?: string;
}

function buildUrl(baseUrl: string, path: string, params?: RequestOptions['params']): string {
  const url = new URL(path.startsWith('http') ? path : `${baseUrl}${path}`, baseUrl);
  if (params) {
    for (const [key, value] of Object.entries(params)) {
      if (value !== undefined) url.searchParams.set(key, String(value));
    }
  }
  return url.toString();
}

export function createApiClient(config: ApiClientConfig) {
  const { baseUrl, timeout = 30_000, getNpub } = config;

  async function request<T>(
    method: string,
    path: string,
    options: RequestOptions = {},
  ): Promise<T> {
    const { params, body, headers: reqHeaders, authHeader } = options;
    const url = buildUrl(baseUrl, path, params);

    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
      ...reqHeaders,
    };

    const npub = authHeader ?? getNpub?.();
    if (npub) headers['X-Nostr-Npub'] = npub;

    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), timeout);

    let response: Response;
    try {
      response = await fetch(url, {
        method,
        headers,
        body: body !== undefined ? JSON.stringify(body) : undefined,
        signal: controller.signal,
      });
    } catch (err) {
      clearTimeout(timeoutId);
      if (err instanceof DOMException && err.name === 'AbortError') {
        throw new TimeoutError();
      }
      throw new NetworkError();
    }
    clearTimeout(timeoutId);

    if (!response.ok) {
      let userMessage: string;
      try {
        const errorBody = await response.json();
        const raw: string =
          typeof errorBody === 'object' && errorBody !== null && typeof errorBody.error === 'string'
            ? errorBody.error
            : '';
        userMessage = raw || `HTTP ${response.status}`;
      } catch {
        userMessage = `HTTP ${response.status}`;
      }
      throw new ApiError(response.status, userMessage);
    }

    if (response.status === 204) return undefined as T;
    return response.json() as Promise<T>;
  }

  return {
    get<T>(path: string, options?: RequestOptions): Promise<T> {
      return request<T>('GET', path, options);
    },
    post<T>(path: string, body?: unknown, options?: RequestOptions): Promise<T> {
      return request<T>('POST', path, { ...options, body });
    },
    put<T>(path: string, body?: unknown, options?: RequestOptions): Promise<T> {
      return request<T>('PUT', path, { ...options, body });
    },
    delete<T>(path: string, options?: RequestOptions): Promise<T> {
      return request<T>('DELETE', path, options);
    },
  };
}

export type ApiClient = ReturnType<typeof createApiClient>;
