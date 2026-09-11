// api.ts
//
// The one place that knows how the backend is reached. Inside the Tauri
// webview every call goes through invoke(); in a plain browser the same call
// becomes a POST to /api/<command> on the hosting server (the Axum shell).
// Components import these typed functions and never touch the transport.
//
// The wire shapes are identical on both paths: flat JSON args in, the types
// from types.ts out, errors as strings (Rust AppError stringified). Every
// caller already wraps calls in try/catch — a rejected promise carries the
// backend's error message either way.
//
// Three operations are inherently platform-shaped and live here too, each
// with a desktop and a web branch: picking an input file, saving an output,
// and resolving instruction-image URLs.

import type {
  ActionResult,
  LoadedProfile,
  ProfileFileEntry,
  ProfileMutation,
  ProfileSummary,
  QueryStepResult,
  ReNxtConnectionStatus,
  ReportRunResult,
  ResultSet,
  SyncResult,
  TransformResult,
  UserInputResult,
  ValidationReport,
  ValidationResult,
} from "../types";

// True inside the Tauri webview; false in a plain browser hitting the server.
export const isTauri = "__TAURI_INTERNALS__" in window;

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (isTauri) {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<T>(command, args);
  }
  const resp = await fetch(`/api/${command}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(args ?? {}),
  });
  if (!resp.ok) {
    // The server returns the AppError string as the body — surface it the
    // same way a rejected invoke() does.
    throw await resp.text();
  }
  return (await resp.json()) as T;
}

// ── profiles ──────────────────────────────────────────────────────────────────

export const listProfiles = () => call<ProfileSummary[]>("list_profiles");

export const loadProfile = (zipPath: string) =>
  call<LoadedProfile>("load_profile", { zipPath });

// ── import pipeline ───────────────────────────────────────────────────────────

export const validateFile = (filePath: string, inputLabel: string, sessionId: string) =>
  call<ValidationResult>("validate_file", { filePath, inputLabel, sessionId });

export const runProfile = (args: {
  filePaths: Record<string, string>;
  queryIds: Record<string, string>;
  syncIds: Record<string, string>;
  formIds: Record<string, string>;
  sqlFile: string;
  sessionId: string;
  outputLabels: string[];
}) => call<TransformResult>("run_profile", args);

export const runReQuery = (
  filePaths: Record<string, string>,
  stepLabel: string,
  sessionId: string,
) => call<QueryStepResult>("run_re_query", { filePaths, stepLabel, sessionId });

export const runCodeTableSync = (
  filePaths: Record<string, string>,
  stepLabel: string,
  sessionId: string,
) => call<SyncResult>("run_code_table_sync", { filePaths, stepLabel, sessionId });

export const runVisualization = (args: {
  filePaths: Record<string, string>;
  queryIds: Record<string, string>;
  syncIds: Record<string, string>;
  formIds: Record<string, string>;
  stepLabel: string;
  sessionId: string;
}) => call<ResultSet>("run_visualization", args);

// Recomputes a user_input step's rows and republishes the values held in
// `values` (row key → field id → value). Called on the step's first readiness
// and again after every edit — the backend never caches the row set.
export const runUserInput = (args: {
  filePaths: Record<string, string>;
  queryIds: Record<string, string>;
  syncIds: Record<string, string>;
  formIds: Record<string, string>;
  stepLabel: string;
  sessionId: string;
  values: Record<string, Record<string, string>>;
}) => call<UserInputResult>("run_user_input", args);

// ── reports ───────────────────────────────────────────────────────────────────

export const runReport = (sessionId: string, paramValues: Record<string, unknown>) =>
  call<ReportRunResult>("run_report", { sessionId, paramValues });

export const runReportAction = (
  sessionId: string,
  actionId: string,
  paramValues: Record<string, unknown>,
) => call<ActionResult>("run_report_action", { sessionId, actionId, paramValues });

// ── profile editor ────────────────────────────────────────────────────────────

export const saveProfile = (zipPath: string, files: ProfileFileEntry[]) =>
  call<ProfileMutation>("save_profile", { zipPath, files });

export const newProfile = () => call<ProfileMutation>("new_profile");

export const duplicateProfile = (sourceZipPath: string) =>
  call<ProfileMutation>("duplicate_profile", { sourceZipPath });

export const deleteProfile = (zipPath: string) =>
  call<void>("delete_profile", { zipPath });

export const validateProfile = (files: ProfileFileEntry[]) =>
  call<ValidationReport>("validate_profile", { files });

export const scaffoldMissing = (files: ProfileFileEntry[]) =>
  call<ProfileFileEntry[]>("scaffold_missing", { files });

// ── RE NXT connection ─────────────────────────────────────────────────────────
// Same three operations on both platforms; only how the browser reaches
// Blackbaud differs.
//
// Desktop: the Rust shell binds a loopback listener, opens the system browser,
// and resolves once the handshake finishes — connectReNxt returns the final
// status.
//
// Web: the server has a real URL, so it just hands back an authorization URL
// and we navigate there. The page unloads, Blackbaud redirects to
// /api/oauth/callback, the server persists the connection and bounces back to
// the app with ?connected=1 (or ?connect_error=…). So on web this call never
// returns a status — it returns null and the caller reads the result from the
// query string on the next load.

export const reNxtStatus = () => call<ReNxtConnectionStatus>("re_nxt_status");

export async function connectReNxt(
  clientId: string,
  clientSecret: string,
  subscriptionKey: string,
): Promise<ReNxtConnectionStatus | null> {
  if (isTauri) {
    return call<ReNxtConnectionStatus>("connect_re_nxt", {
      clientId,
      clientSecret,
      subscriptionKey,
    });
  }
  const { authorizeUrl } = await call<{ authorizeUrl: string; redirectUri: string }>(
    "connect_re_nxt",
    { clientId, clientSecret, subscriptionKey },
  );
  window.location.href = authorizeUrl;
  return null; // navigating away; the callback finishes the job
}

export const disconnectReNxt = () => call<void>("disconnect_re_nxt");

// The redirect URI that must be registered on the Blackbaud application.
// Desktop uses the fixed loopback port; web uses this deployment's own origin.
export const redirectUri = () =>
  isTauri
    ? "http://localhost:13631/callback"
    : `${window.location.origin}/api/oauth/callback`;

// True when this page load is the return leg of a web OAuth handshake. Does
// NOT consume the params — App.tsx uses it to reveal the settings panel so the
// outcome is actually seen; GeneralTab consumes them with takeConnectResult().
export function hasConnectResult(): boolean {
  if (isTauri) return false;
  const params = new URLSearchParams(window.location.search);
  return params.has("connected") || params.has("connect_error");
}

// Result of a web connect attempt, handed back through the query string by
// the server's OAuth callback. Clears the params so a refresh doesn't re-show
// a stale banner.
export function takeConnectResult(): { ok: boolean; error?: string } | null {
  if (isTauri) return null;
  const params = new URLSearchParams(window.location.search);
  const connected = params.has("connected");
  const error = params.get("connect_error");
  if (!connected && !error) return null;
  params.delete("connected");
  params.delete("connect_error");
  const qs = params.toString();
  window.history.replaceState({}, "", window.location.pathname + (qs ? `?${qs}` : ""));
  return error ? { ok: false, error } : { ok: true };
}

// ── platform-shaped operations ────────────────────────────────────────────────

export type PickedFile = { path: string; name: string };

// Let the user choose an input file. Desktop: the native open dialog returns
// a local path the backend reads directly. Web: an <input type=file> picker
// uploads the bytes into the session's inputs dir and the returned server-side
// path takes the local path's place — everything downstream is unchanged.
export async function pickInputFile(
  inputLabel: string,
  extensions: string[],
  sessionId: string,
): Promise<PickedFile | null> {
  if (isTauri) {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const { basename } = await import("@tauri-apps/api/path");
    const selected = await open({
      multiple: false,
      filters: [{ name: inputLabel, extensions }],
    });
    if (typeof selected !== "string") return null; // user cancelled
    return { path: selected, name: await basename(selected) };
  }

  // Browser: programmatic file input → multipart upload.
  const file = await new Promise<File | null>((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = extensions.map((e) => `.${e}`).join(",");
    input.onchange = () => resolve(input.files?.[0] ?? null);
    // Cancel fires no event we can rely on cross-browser; focus return is the
    // usual proxy. A stale resolve(null) after a real pick is harmless.
    window.addEventListener("focus", () => setTimeout(() => resolve(null), 500), {
      once: true,
    });
    input.click();
  });
  if (!file) return null;

  const form = new FormData();
  form.append("file", file);
  const resp = await fetch(
    `/api/sessions/${encodeURIComponent(sessionId)}/inputs`,
    { method: "POST", body: form },
  );
  if (!resp.ok) throw await resp.text();
  const { path } = (await resp.json()) as { path: string };
  return { path, name: file.name };
}

// Save a generated output. Desktop: native Save As dialog + backend copy.
// Web: navigate to the download endpoint — the server streams the artifact
// with a Content-Disposition filename.
export async function saveOutputFile(
  sessionId: string,
  artifactId: string,
  suggestedName: string,
): Promise<void> {
  if (isTauri) {
    const { save } = await import("@tauri-apps/plugin-dialog");
    const dest = await save({
      defaultPath: suggestedName,
      filters: [{ name: "CSV", extensions: ["csv"] }],
    });
    if (typeof dest !== "string") return; // user cancelled
    await call<void>("save_output", { sessionId, artifactId, destPath: dest });
    return;
  }
  const url =
    `/api/sessions/${encodeURIComponent(sessionId)}` +
    `/artifacts/${encodeURIComponent(artifactId)}?name=${encodeURIComponent(suggestedName)}`;
  const a = document.createElement("a");
  a.href = url;
  a.download = suggestedName;
  a.click();
}

// URL for an instruction image. `assetBase` comes from LoadedProfile: on
// desktop it's the extracted profile dir (served via the asset protocol); on
// the web the server serves the same file out of the session.
export async function assetUrl(assetBase: string, rel: string): Promise<string> {
  const full = `${assetBase}/${rel}`.replace(/\\/g, "/");
  if (isTauri) {
    const { convertFileSrc } = await import("@tauri-apps/api/core");
    return convertFileSrc(full);
  }
  return full;
}
