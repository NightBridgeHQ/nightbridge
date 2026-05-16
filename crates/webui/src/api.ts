export type DaemonStatus = {
  alias: string;
  fingerprint: string;
  version: string;
  inbox_dir: string;
  localsend_port: number;
  native_port: number;
};

export type TrustedPeer = {
  fingerprint: string;
  pubkey_hex: string;
  label: string;
  trusted_at_unix_seconds: number;
  last_seen_unix_seconds: number | null;
  policy: string;
};

export type LanPeer = {
  alias: string;
  address: string;
  port: number;
  protocol: string;
  fingerprint: string;
  device_model?: string | null;
  device_type?: string | null;
  download: boolean;
};

export type InboxEntry = {
  file_name: string;
  path: string;
  size: number;
  modified_unix_seconds: number;
};

export type Transfer = Record<string, unknown>;

export type DaemonSnapshot = {
  status: DaemonStatus;
  trustedPeers: TrustedPeer[];
  lanPeers: LanPeer[];
  inbox: InboxEntry[];
  transfers: Transfer[];
};

export type DaemonEvent = {
  event_id?: string;
  occurred_at_unix_seconds?: number;
  type?: string;
  [key: string]: unknown;
};

let apiBase = "";

export function setApiBase(base: string): void {
  apiBase = base.replace(/\/+$/, "");
}

export function resolveApiPath(path: string): string {
  if (!path.startsWith("/")) {
    throw new Error("API path must start with /");
  }
  return `${apiBase}${path}`;
}

const jsonHeaders = (token: string): HeadersInit => ({
  Authorization: `Bearer ${token}`,
  Accept: "application/json"
});

async function requestJson<T>(path: string, token: string): Promise<T> {
  let response: Response;
  try {
    response = await fetch(resolveApiPath(path), { headers: jsonHeaders(token) });
  } catch (error) {
    throw new Error(`Cannot reach daemon endpoint: ${error instanceof Error ? error.message : String(error)}`);
  }
  if (!response.ok) {
    if (response.status === 401 || response.status === 403) {
      throw new Error("Bad API token or token missing");
    }
    throw new Error(`${response.status} ${response.statusText || "API request failed"}`);
  }
  return (await response.json()) as T;
}

export async function loadSnapshot(token: string): Promise<DaemonSnapshot> {
  const [status, trusted, lan, inbox, active] = await Promise.all([
    requestJson<DaemonStatus>("/api/v1/status", token),
    requestJson<{ peers: TrustedPeer[] }>("/api/v1/peers/trusted", token),
    requestJson<{ peers: LanPeer[] }>("/api/v1/peers/lan", token),
    requestJson<{ entries: InboxEntry[] }>("/api/v1/inbox", token),
    requestJson<{ transfers: Transfer[] }>("/api/v1/transfers/active", token)
  ]);

  return {
    status,
    trustedPeers: trusted.peers,
    lanPeers: lan.peers,
    inbox: inbox.entries,
    transfers: active.transfers
  };
}

export async function streamEvents(
  token: string,
  signal: AbortSignal,
  onEvent: (event: DaemonEvent) => void
): Promise<void> {
  let response: Response;
  try {
    response = await fetch(resolveApiPath("/api/v1/events"), {
      headers: { Authorization: `Bearer ${token}`, Accept: "text/event-stream" },
      signal
    });
  } catch (error) {
    if (signal.aborted) {
      return;
    }
    throw new Error(`Event stream disconnected: ${error instanceof Error ? error.message : String(error)}`);
  }
  if (!response.ok || !response.body) {
    if (response.status === 401 || response.status === 403) {
      throw new Error("Bad API token or token missing");
    }
    throw new Error(`${response.status} ${response.statusText || "event stream failed"}`);
  }

  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";

  while (!signal.aborted) {
    const { value, done } = await reader.read();
    if (done) {
      break;
    }
    buffer += decoder.decode(value, { stream: true });
    let boundary = buffer.indexOf("\n\n");
    while (boundary !== -1) {
      const frame = buffer.slice(0, boundary);
      buffer = buffer.slice(boundary + 2);
      const data = frame
        .split("\n")
        .filter((line) => line.startsWith("data:"))
        .map((line) => line.slice(5).trimStart())
        .join("\n");
      if (data) {
        onEvent(JSON.parse(data) as DaemonEvent);
      }
      boundary = buffer.indexOf("\n\n");
    }
  }
}
