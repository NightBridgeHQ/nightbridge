import { get, writable } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";
import { loadSnapshot, setApiBase, streamEvents, type DaemonEvent, type DaemonSnapshot } from "./api";

const TOKEN_KEY = "lsi.apiToken";
const API_BASE_KEY = "lsi.apiBase";
const MODE_KEY = "lsi.guiMode";

export type GuiMode = "remote" | "standalone";

export type ConnectionState = "disconnected" | "connecting" | "connected" | "error";

export type AppState = {
  token: string;
  apiBase: string;
  guiMode: GuiMode;
  isDesktop: boolean;
  connection: ConnectionState;
  snapshot: DaemonSnapshot | null;
  lastEvent: DaemonEvent | null;
  error: string | null;
};

type DesktopSettings = {
  mode: GuiMode;
  remote_endpoint: string | null;
  api_token: string | null;
};

const initialToken = typeof localStorage === "undefined" ? "" : localStorage.getItem(TOKEN_KEY) ?? "";
const initialApiBase = typeof localStorage === "undefined" ? "" : localStorage.getItem(API_BASE_KEY) ?? "";
const initialGuiMode =
  typeof localStorage === "undefined" ? "remote" : ((localStorage.getItem(MODE_KEY) as GuiMode | null) ?? "remote");
const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

setApiBase(initialApiBase);

export const appState = writable<AppState>({
  token: initialToken,
  apiBase: initialApiBase,
  guiMode: initialGuiMode,
  isDesktop: isTauri,
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

export function setDaemonApiBase(base: string): void {
  const trimmed = base.trim().replace(/\/+$/, "");
  if (trimmed) {
    localStorage.setItem(API_BASE_KEY, trimmed);
  } else {
    localStorage.removeItem(API_BASE_KEY);
  }
  setApiBase(trimmed);
  appState.update((state) => ({ ...state, apiBase: trimmed, error: null }));
}

function setGuiMode(mode: GuiMode): void {
  if (typeof localStorage !== "undefined") {
    localStorage.setItem(MODE_KEY, mode);
  }
  appState.update((state) => ({ ...state, guiMode: mode, error: null }));
}

export async function loadDesktopSettings(): Promise<void> {
  if (!isTauri) {
    return;
  }

  try {
    const settings = await invoke<DesktopSettings>("gui_load_settings");
    setGuiMode(settings.mode);
    setDaemonApiBase(settings.remote_endpoint ?? "");
    setToken(settings.api_token ?? "");
  } catch (error) {
    appState.update((state) => ({
      ...state,
      error: error instanceof Error ? error.message : String(error)
    }));
  }
}

export async function saveDesktopSettings(mode: GuiMode, endpoint: string, token: string): Promise<void> {
  const trimmedEndpoint = endpoint.trim().replace(/\/+$/, "");
  const trimmedToken = token.trim();
  if (mode === "remote" && trimmedEndpoint && !/^https?:\/\//.test(trimmedEndpoint)) {
    appState.update((state) => ({
      ...state,
      connection: "error",
      error: "Remote daemon endpoint must start with http:// or https://"
    }));
    return;
  }

  if (isTauri) {
    await invoke("gui_save_settings", {
      settings: {
        mode,
        remote_endpoint: trimmedEndpoint || null,
        api_token: trimmedToken || null
      }
    });
  }

  setGuiMode(mode);
  setDaemonApiBase(mode === "remote" ? trimmedEndpoint : "");
  setToken(trimmedToken);
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
