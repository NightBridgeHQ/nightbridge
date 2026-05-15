import { get, writable } from "svelte/store";
import { loadSnapshot, streamEvents, type DaemonEvent, type DaemonSnapshot } from "./api";

const TOKEN_KEY = "lsi.apiToken";

export type ConnectionState = "disconnected" | "connecting" | "connected" | "error";

export type AppState = {
  token: string;
  connection: ConnectionState;
  snapshot: DaemonSnapshot | null;
  lastEvent: DaemonEvent | null;
  error: string | null;
};

const initialToken = typeof localStorage === "undefined" ? "" : localStorage.getItem(TOKEN_KEY) ?? "";

export const appState = writable<AppState>({
  token: initialToken,
  connection: initialToken ? "disconnected" : "disconnected",
  snapshot: null,
  lastEvent: null,
  error: null
});

let eventsAbort: AbortController | null = null;

export function setToken(token: string): void {
  const trimmed = token.trim();
  if (trimmed) {
    localStorage.setItem(TOKEN_KEY, trimmed);
  } else {
    localStorage.removeItem(TOKEN_KEY);
  }
  appState.update((state) => ({ ...state, token: trimmed, error: null }));
}

export async function refreshSnapshot(): Promise<void> {
  const token = get(appState).token;
  if (!token) {
    appState.update((state) => ({
      ...state,
      connection: "disconnected",
      snapshot: null,
      error: "API token required"
    }));
    return;
  }

  appState.update((state) => ({ ...state, connection: "connecting", error: null }));
  try {
    const snapshot = await loadSnapshot(token);
    appState.update((state) => ({ ...state, connection: "connected", snapshot, error: null }));
  } catch (error) {
    appState.update((state) => ({
      ...state,
      connection: "error",
      error: error instanceof Error ? error.message : String(error)
    }));
  }
}

export function connectEvents(): void {
  const token = get(appState).token;
  eventsAbort?.abort();
  eventsAbort = null;
  if (!token) {
    return;
  }

  const abort = new AbortController();
  eventsAbort = abort;
  void streamEvents(token, abort.signal, (event) => {
    appState.update((state) => ({ ...state, lastEvent: event }));
    void refreshSnapshot();
  }).catch((error) => {
    if (abort.signal.aborted) {
      return;
    }
    appState.update((state) => ({
      ...state,
      connection: "error",
      error: error instanceof Error ? error.message : String(error)
    }));
  });
}

export function disconnectEvents(): void {
  eventsAbort?.abort();
  eventsAbort = null;
}
