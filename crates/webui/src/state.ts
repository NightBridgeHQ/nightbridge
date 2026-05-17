import { get, writable } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";
import { loadSnapshot, setApiBase, streamEvents, type DaemonEvent, type DaemonSnapshot } from "./api";

const TOKEN_KEY = "lsi.apiToken";
const API_BASE_KEY = "lsi.apiBase";
const MODE_KEY = "lsi.guiMode";

export type GuiMode = "remote" | "standalone";

export type ConnectionState = "disconnected" | "connecting" | "connected" | "error";
export type StandaloneState = "stopped" | "starting" | "running" | "error";

export type AppState = {
  token: string;
  apiBase: string;
  guiMode: GuiMode;
  isDesktop: boolean;
  standalone: StandaloneState;
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

type EmbeddedDaemonStatus = {
  running: boolean;
  endpoint: string | null;
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
  standalone: "stopped",
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

function endpointError(endpoint: string): string | null {
  if (!endpoint) {
    return null;
  }
  try {
    const url = new URL(endpoint);
    if (url.protocol !== "http:" && url.protocol !== "https:") {
      return "Remote daemon endpoint must start with http:// or https://";
    }
  } catch {
    return "Remote daemon endpoint must be a valid URL";
  }
  return null;
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
    if (settings.mode === "standalone") {
      const status = await invoke<EmbeddedDaemonStatus>("gui_embedded_daemon_status");
      applyEmbeddedDaemonStatus(status);
    }
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
  const invalidEndpoint = mode === "remote" ? endpointError(trimmedEndpoint) : null;
  if (invalidEndpoint) {
    appState.update((state) => ({
      ...state,
      connection: "error",
      error: invalidEndpoint
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

export async function startStandaloneDaemon(): Promise<void> {
  if (!isTauri) {
    appState.update((state) => ({
      ...state,
      standalone: "error",
      connection: "error",
      error: "Standalone mode requires the desktop app"
    }));
    return;
  }

  appState.update((state) => ({ ...state, standalone: "starting", connection: "connecting", error: null }));
  try {
    const status = await invoke<EmbeddedDaemonStatus>("gui_start_embedded_daemon");
    applyEmbeddedDaemonStatus(status);
  } catch (error) {
    appState.update((state) => ({
      ...state,
      standalone: "error",
      connection: "error",
      error: error instanceof Error ? error.message : String(error)
    }));
  }
}

export async function stopStandaloneDaemon(): Promise<void> {
  if (!isTauri) {
    return;
  }

  try {
    await invoke("gui_stop_embedded_daemon");
    disconnectEvents();
    setDaemonApiBase("");
    setToken("");
    appState.update((state) => ({
      ...state,
      standalone: "stopped",
      connection: "disconnected",
      snapshot: null,
      lastEvent: null,
      error: null
    }));
  } catch (error) {
    appState.update((state) => ({
      ...state,
      standalone: "error",
      connection: "error",
      error: error instanceof Error ? error.message : String(error)
    }));
  }
}

function applyEmbeddedDaemonStatus(status: EmbeddedDaemonStatus): void {
  if (!status.running) {
    appState.update((state) => ({
      ...state,
      standalone: "stopped",
      connection: "disconnected",
      snapshot: null,
      error: null
    }));
    return;
  }

  setGuiMode("standalone");
  setDaemonApiBase(status.endpoint ?? "");
  setToken(status.api_token ?? "");
  appState.update((state) => ({ ...state, standalone: "running", error: null }));
}

export async function refreshSnapshot(): Promise<void> {
  const current = get(appState);
  const token = current.token;
  if (!token) {
    appState.update((state) => ({
      ...state,
      connection: "disconnected",
      snapshot: null,
      error: "API token required"
    }));
    return;
  }
  const invalidEndpoint = current.guiMode === "remote" ? endpointError(current.apiBase) : null;
  if (invalidEndpoint) {
    appState.update((state) => ({
      ...state,
      connection: "error",
      snapshot: null,
      error: invalidEndpoint
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
