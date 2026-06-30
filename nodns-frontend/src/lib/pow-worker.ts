import { minePow } from 'nostr-tools/nip13';

interface PowRequest {
  template: {
    kind: number;
    tags: string[][];
    content: string;
    created_at: number;
  };
  difficulty: number;
  pubkey: string;
}

interface PowDoneResponse {
  type: 'done';
  mined: {
    kind: number;
    tags: string[][];
    content: string;
    created_at: number;
    pubkey: string;
    id: string;
  };
}

interface PowErrorResponse {
  type: 'error';
  error: string;
}

self.onmessage = (e: MessageEvent<PowRequest>) => {
  const { template, difficulty, pubkey } = e.data;
  try {
    const mined = minePow({ ...template, pubkey }, difficulty);
    const response: PowDoneResponse = {
      type: 'done',
      mined: {
        kind: mined.kind,
        tags: mined.tags,
        content: mined.content,
        created_at: mined.created_at,
        pubkey: mined.pubkey,
        id: mined.id,
      },
    };
    (self as unknown as Worker).postMessage(response);
  } catch (err) {
    const response: PowErrorResponse = {
      type: 'error',
      error: err instanceof Error ? err.message : String(err),
    };
    (self as unknown as Worker).postMessage(response);
  }
};
