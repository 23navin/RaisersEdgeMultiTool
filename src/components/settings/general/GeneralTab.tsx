// GeneralTab.tsx
//
// General settings tab. Currently hosts the Raiser's Edge NXT connection
// card — the OAuth setup for the Data Requests / Reports features.
//
// Follows the ImportTab convention: this component owns its state and is the
// only place here that calls invoke(). The backend (sky_auth.rs) runs the
// actual OAuth handshake; this UI just collects the three credentials, kicks
// off connect_re_nxt, and reflects connection status.

import { useEffect, useRef, useState } from "react";
import {
  AlertCircleIcon,
  CheckCircle2Icon,
  ExternalLinkIcon,
  Loader2Icon,
} from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { cn } from "../../../lib/utils";
import type { ReNxtConnectionStatus } from "../../../types";

// Must match REDIRECT_URI in sky_auth.rs and the Redirect URI registered on
// the application in the Blackbaud developer portal.
const REDIRECT_URI = "http://localhost:13631/callback";

export function GeneralTab() {
  const [status, setStatus] = useState<ReNxtConnectionStatus | null>(null);
  const [clientId, setClientId] = useState("");
  const [clientSecret, setClientSecret] = useState("");
  const [subscriptionKey, setSubscriptionKey] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Guard against a status response landing after the component unmounts
  // (the panel can close mid-request).
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const refreshStatus = async () => {
    try {
      const s = await invoke<ReNxtConnectionStatus>("re_nxt_status");
      if (mounted.current) setStatus(s);
    } catch (e) {
      if (mounted.current) setError(String(e));
    }
  };

  useEffect(() => {
    refreshStatus();
  }, []);

  const canConnect =
    clientId.trim() !== "" &&
    clientSecret.trim() !== "" &&
    subscriptionKey.trim() !== "" &&
    !busy;

  const handleConnect = async () => {
    if (!canConnect) return;
    setBusy(true);
    setError(null);
    try {
      // Resolves once the user finishes the browser handshake against their
      // RE NXT environment. The backend persists the connection; the secret
      // never comes back to the frontend.
      const s = await invoke<ReNxtConnectionStatus>("connect_re_nxt", {
        clientId: clientId.trim(),
        clientSecret: clientSecret.trim(),
        subscriptionKey: subscriptionKey.trim(),
      });
      if (!mounted.current) return;
      setStatus(s);
      // Clear the secret from the form once it's safely stored backend-side.
      setClientSecret("");
    } catch (e) {
      if (mounted.current) setError(String(e));
    } finally {
      if (mounted.current) setBusy(false);
    }
  };

  const handleDisconnect = async () => {
    setBusy(true);
    setError(null);
    try {
      await invoke("disconnect_re_nxt");
      if (!mounted.current) return;
      setStatus({ connected: false });
    } catch (e) {
      if (mounted.current) setError(String(e));
    } finally {
      if (mounted.current) setBusy(false);
    }
  };

  return (
    <div className="px-[24px] py-[18px] max-w-[620px]">
      <SectionHeader
        title="Raiser's Edge NXT"
        subtitle="Connect to a RE NXT environment to use Data Requests and Reports."
      />

      {error && (
        <div className="mb-[14px] rounded-[8px] border border-red-200 bg-red-50 px-[12px] py-[9px] text-[12px] text-red-700 flex items-start gap-[7px]">
          <AlertCircleIcon size={14} className="shrink-0 mt-[1px]" />
          <span className="break-words">{error}</span>
        </div>
      )}

      {status === null ? (
        <div className="flex items-center gap-[8px] text-[13px] text-neutral-500 py-[8px]">
          <Loader2Icon size={14} className="animate-spin" />
          Checking connection…
        </div>
      ) : status.connected ? (
        <ConnectedCard
          status={status}
          busy={busy}
          onDisconnect={handleDisconnect}
        />
      ) : (
        <ConnectForm
          clientId={clientId}
          clientSecret={clientSecret}
          subscriptionKey={subscriptionKey}
          busy={busy}
          canConnect={canConnect}
          onClientId={setClientId}
          onClientSecret={setClientSecret}
          onSubscriptionKey={setSubscriptionKey}
          onConnect={handleConnect}
        />
      )}
    </div>
  );
}

// ── Connected state ─────────────────────────────────────────────────────────────

function ConnectedCard({
  status,
  busy,
  onDisconnect,
}: {
  status: ReNxtConnectionStatus;
  busy: boolean;
  onDisconnect: () => void;
}) {
  return (
    <div className="rounded-[10px] border border-neutral-200 bg-neutral-50/60 px-[16px] py-[14px]">
      <div className="flex items-start justify-between gap-[12px]">
        <div className="flex items-start gap-[9px] min-w-0">
          <CheckCircle2Icon
            size={18}
            className="shrink-0 mt-[1px] text-emerald-600"
          />
          <div className="min-w-0">
            <div className="text-[13px] font-medium text-neutral-900">
              Connected
              {status.environment_name ? ` · ${status.environment_name}` : ""}
            </div>
            {status.environment_id && (
              <div className="text-[11px] text-neutral-500 mt-[2px]">
                Environment ID: {status.environment_id}
              </div>
            )}
            <div className="text-[11px] text-neutral-500 mt-[1px]">
              {expiryLabel(status.expires_at)}
            </div>
          </div>
        </div>
        <button
          type="button"
          onClick={onDisconnect}
          disabled={busy}
          className="shrink-0 text-[12px] px-[10px] py-[5px] rounded-[6px] border border-neutral-200 bg-white text-red-700 hover:bg-red-50 cursor-pointer disabled:cursor-wait disabled:opacity-60 transition-colors"
        >
          Disconnect
        </button>
      </div>
    </div>
  );
}

// ── Disconnected / connect form ─────────────────────────────────────────────────

function ConnectForm({
  clientId,
  clientSecret,
  subscriptionKey,
  busy,
  canConnect,
  onClientId,
  onClientSecret,
  onSubscriptionKey,
  onConnect,
}: {
  clientId: string;
  clientSecret: string;
  subscriptionKey: string;
  busy: boolean;
  canConnect: boolean;
  onClientId: (v: string) => void;
  onClientSecret: (v: string) => void;
  onSubscriptionKey: (v: string) => void;
  onConnect: () => void;
}) {
  return (
    <div className="space-y-[14px]">
      <Field
        label="Application ID"
        hint="The client ID from your application in the Blackbaud developer portal."
        value={clientId}
        onChange={onClientId}
        disabled={busy}
        placeholder="00000000-0000-0000-0000-000000000000"
      />
      <Field
        label="Application secret"
        hint="The client secret paired with the Application ID. Stored locally; never shown again."
        value={clientSecret}
        onChange={onClientSecret}
        disabled={busy}
        type="password"
        placeholder="••••••••••••••••"
      />
      <Field
        label="Subscription key"
        hint="Your Primary access key from My subscriptions. Sent as Bb-Api-Subscription-Key on every request."
        value={subscriptionKey}
        onChange={onSubscriptionKey}
        disabled={busy}
        type="password"
        placeholder="••••••••••••••••"
      />

      <div className="flex items-center gap-[12px] pt-[2px]">
        <button
          type="button"
          onClick={onConnect}
          disabled={!canConnect}
          className={cn(
            "inline-flex items-center gap-[6px] text-[12px] px-[12px] py-[6px] rounded-[6px] border-0 cursor-pointer transition-colors",
            canConnect
              ? "bg-neutral-900 text-white hover:bg-neutral-800"
              : "bg-neutral-200 text-neutral-400 cursor-not-allowed",
          )}
        >
          {busy ? (
            <>
              <Loader2Icon size={13} className="animate-spin" />
              Waiting for browser…
            </>
          ) : (
            <>
              <ExternalLinkIcon size={13} />
              Connect to Raiser's Edge NXT
            </>
          )}
        </button>
        {busy && (
          <span className="text-[11px] text-neutral-500">
            Complete sign-in in the browser window that opened.
          </span>
        )}
      </div>

      <p className="text-[11px] leading-[1.5] text-neutral-500 border-t border-neutral-100 pt-[12px]">
        Register{" "}
        <code className="text-[10px] bg-neutral-100 rounded-[4px] px-[4px] py-[1px]">
          {REDIRECT_URI}
        </code>{" "}
        as a Redirect URI on your application in the developer portal, or the
        sign-in will fail with a redirect mismatch.
      </p>
    </div>
  );
}

// ── Bits ────────────────────────────────────────────────────────────────────────

function SectionHeader({
  title,
  subtitle,
}: {
  title: string;
  subtitle: string;
}) {
  return (
    <div className="mb-[16px]">
      <h2 className="text-[15px] font-semibold text-neutral-900">{title}</h2>
      <p className="text-[12px] text-neutral-500 mt-[2px]">{subtitle}</p>
    </div>
  );
}

function Field({
  label,
  hint,
  value,
  onChange,
  disabled,
  type = "text",
  placeholder,
}: {
  label: string;
  hint: string;
  value: string;
  onChange: (v: string) => void;
  disabled: boolean;
  type?: "text" | "password";
  placeholder?: string;
}) {
  return (
    <label className="block">
      <span className="block text-[12px] font-medium text-neutral-800">
        {label}
      </span>
      <span className="block text-[11px] text-neutral-500 mt-[1px] mb-[5px]">
        {hint}
      </span>
      <input
        type={type}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        disabled={disabled}
        placeholder={placeholder}
        autoComplete="off"
        spellCheck={false}
        className={cn(
          "w-full text-[12px] font-mono px-[10px] py-[7px] rounded-[6px] border border-neutral-200 bg-white text-neutral-900",
          "outline-none focus:border-neutral-400 focus:ring-2 focus:ring-neutral-900/5",
          "disabled:bg-neutral-50 disabled:text-neutral-400",
        )}
      />
    </label>
  );
}

function expiryLabel(expiresAt?: number | null): string {
  if (!expiresAt) return "Access token active.";
  const secondsLeft = expiresAt - Math.floor(Date.now() / 1000);
  if (secondsLeft <= 0) return "Access token expired — refreshes on next request.";
  const minutes = Math.round(secondsLeft / 60);
  return `Access token valid for ~${minutes} min (auto-refreshes).`;
}
