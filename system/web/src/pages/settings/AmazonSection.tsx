import { useEffect, useState } from "react";
import { toast } from "sonner";
import { ShoppingCart } from "lucide-react";
import { C } from "@/components/setup/shared";
import { getConnector, removeConnector, getAmazonStatus, disconnectAmazon, getAmazonToolkit, setAmazonToolkit, getAmazonSP, setAmazonSP } from "@/lib/api";
import type { AmazonToolkitInfo } from "@/lib/api";
import { InfoCard, Step, FieldLabel } from "@/pages/settings/connectorFormParts";

const CONNECTOR_CODE = "amazon";

const buttonBase: React.CSSProperties = {
  padding: "8px 18px", borderRadius: 8, fontSize: 12, fontWeight: 500, background: "none",
  border: `1px solid ${C.border}`, color: C.text,
};

function Header({ connected, connectedAt }: { connected: boolean; connectedAt: number }) {
  return (
    <div style={{ display: "flex", alignItems: "flex-start", gap: 14, marginBottom: 18 }}>
      <div style={{
        flexShrink: 0, width: 44, height: 44, borderRadius: 12, background: "#E88B00",
        display: "flex", alignItems: "center", justifyContent: "center",
      }}>
        <ShoppingCart size={24} color="#fff" strokeWidth={2.2} />
      </div>
      <div style={{ minWidth: 0, flex: 1 }}>
        <div style={{ fontSize: 16, fontWeight: 700, color: C.text, marginBottom: 3 }}>Amazon seller account</div>
        <div style={{ fontSize: 12, color: C.textDim, lineHeight: 1.5 }}>
          The robot works with your Amazon seller account through Amazon&apos;s official Selling Partner connector.
        </div>
        <div style={{ marginTop: 8, fontSize: 12, color: connected ? C.green : C.textDim }}>
          {connected ? "✓ connected" : "not connected yet"}
          {connected && connectedAt > 0 && (
            <span style={{ color: C.textDim }}> · {new Date(connectedAt * 1000).toLocaleDateString()}</span>
          )}
        </div>
      </div>
    </div>
  );
}

const inputStyle: React.CSSProperties = {
  width: "100%", boxSizing: "border-box", padding: "9px 12px", borderRadius: 8, fontSize: 13,
  background: C.surface, border: `1px solid ${C.border}`, color: C.text, marginBottom: 12,
};

const linkStyle: React.CSSProperties = { color: C.amber, textDecoration: "underline", textUnderlineOffset: 2 };

const REGIONS = [
  { value: "na", label: "North America (US, Canada, Mexico, Brazil)" },
  { value: "eu", label: "Europe (UK, Germany, France, Italy, Spain, ...)" },
  { value: "fe", label: "Far East (Japan, Australia, Singapore)" },
];

/** Option 1: the seller's own private Selling Partner API app (client ID, client secret, refresh token). */
function AmazonDirectCard({ onConnected }: { onConnected: (connected: boolean) => void }) {
  const [loading, setLoading] = useState(true);
  const [connected, setConnected] = useState(false);
  const [region, setRegion] = useState("na");
  const [connectedRegion, setConnectedRegion] = useState("");
  const [clientId, setClientId] = useState("");
  const [clientSecret, setClientSecret] = useState("");
  const [refreshToken, setRefreshToken] = useState("");
  const [busy, setBusy] = useState<"connect" | "disconnect" | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getAmazonSP()
      .then((r) => { setConnected(!!r.connected); setConnectedRegion(r.region ?? ""); onConnected(!!r.connected); })
      .catch(() => {})
      .finally(() => setLoading(false));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const canSubmit = !!clientId.trim() && !!clientSecret.trim() && !!refreshToken.trim();
  const reset = () => { setClientId(""); setClientSecret(""); setRefreshToken(""); setError(null); };

  const onSubmit = async () => {
    setError(null);
    setBusy("connect");
    try {
      const r = await setAmazonSP({ region, client_id: clientId.trim(), client_secret: clientSecret.trim(), refresh_token: refreshToken.trim() });
      setConnected(!!r.connected);
      setConnectedRegion(r.region ?? region);
      onConnected(!!r.connected);
      reset();
      toast.success("Amazon connected.");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Could not connect Amazon.");
    } finally {
      setBusy(null);
    }
  };

  const onDisconnect = async () => {
    if (!window.confirm("Disconnect Amazon? The robot forgets the saved client ID, secret and refresh token.")) return;
    setBusy("disconnect");
    try {
      await removeConnector("amazon_sp");
      setConnected(false);
      setConnectedRegion("");
      onConnected(false);
      toast.success("Amazon disconnected.");
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Disconnect failed.");
    } finally {
      setBusy(null);
    }
  };

  return (
    <div style={{ marginBottom: 18, padding: "14px 16px", background: C.bg, border: `1px solid ${C.border}`, borderRadius: 10 }}>
      <div style={{ fontSize: 13.5, fontWeight: 700, color: C.text, marginBottom: 4, display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
        Option 1: your own Amazon app
        {connected && (
          <span style={{ fontSize: 12, fontWeight: 400, color: C.green }}>
            ✓ connected{connectedRegion ? ` · ${connectedRegion.toUpperCase()}` : ""}
          </span>
        )}
      </div>
      <div style={{ fontSize: 12, color: C.textDim, lineHeight: 1.6, marginBottom: 12 }}>
        Link the robot to your seller account with a private app you create in Seller Central. You enter the app&apos;s
        client ID and secret and the refresh token Amazon gives when you authorize it. The robot renews its short-lived
        access tokens itself. Saved on this device only; the secret is never shown again.
      </div>

      <details style={{ marginBottom: 12 }}>
        <summary style={{ fontSize: 12, color: C.amber, cursor: "pointer" }}>How to create the app and get the three values</summary>
        <div style={{ marginTop: 10 }}>
          <Step n={1}>
            <b>Register as a developer.</b> Sign in to Seller Central as the <b>account owner or an administrator</b>. The
            account needs a <b>Professional</b> selling plan. Open <b>Apps and Services → Develop Apps</b> (
            <a href="https://sellercentral.amazon.com/sellingpartner/developerconsole/" target="_blank" rel="noopener noreferrer" style={linkStyle}>direct link</a>)
            and complete the developer profile, choosing the option for your <b>own account</b> (a private developer).
          </Step>
          <Step n={2}>
            <b>Create the app.</b> <b>Add new app client</b>, pick the <b>SP-API</b> API type, and select the roles the robot
            needs (for example product listings, inventory and orders). Leave it as a private app.
          </Step>
          <Step n={3}>
            <b>Authorize it on your own account.</b> Open the app&apos;s menu and choose <b>Authorize</b> → <b>Authorize app</b>.
            Amazon shows a <b>refresh token</b> (it starts with <code>Atzr|</code>). Copy it. An app can be authorized on at
            most 10 accounts. Amazon&apos;s own guide:{" "}
            <a href="https://developer-docs.amazon.com/sp-api/docs/self-authorization" target="_blank" rel="noopener noreferrer" style={linkStyle}>Self authorization</a>.
          </Step>
          <Step n={4}>
            <b>Copy the client ID and secret.</b> In the app, open <b>LWA credentials → View</b> and copy the
            <b> client identifier</b> (starts with <code>amzn1.application-oa2-client.</code>) and the <b>client secret</b>.
            Treat the secret and the refresh token like passwords; they are only saved on this device.
          </Step>
          <Step n={5} last>
            <b>Enter them here,</b> pick the region where your account sells, and press <b>Connect</b>. The device checks them
            with Amazon once before saving. Menu names can differ slightly in Seller Central; if a step looks different, look
            for the same item.
          </Step>
        </div>
      </details>

      {loading ? (
        <div style={{ fontSize: 12, color: C.textDim }}>Loading…</div>
      ) : (
        <div onKeyDown={(e) => { if (e.key === "Enter" && (e.target as HTMLElement).tagName === "INPUT" && canSubmit && !busy) onSubmit(); }}>
          <FieldLabel htmlFor="amazon_sp_region">Region</FieldLabel>
          <select id="amazon_sp_region" value={region} onChange={(e) => setRegion(e.target.value)} style={inputStyle}>
            {REGIONS.map((r) => <option key={r.value} value={r.value}>{r.label}</option>)}
          </select>
          <FieldLabel htmlFor="amazon_sp_client_id">Client ID</FieldLabel>
          <input id="amazon_sp_client_id" type="text" value={clientId} onChange={(e) => setClientId(e.target.value)}
            placeholder="amzn1.application-oa2-client.…" autoComplete="off" spellCheck={false}
            style={{ ...inputStyle, fontFamily: "monospace" }} />
          <FieldLabel htmlFor="amazon_sp_client_secret">Client secret</FieldLabel>
          <input id="amazon_sp_client_secret" type="password" value={clientSecret} onChange={(e) => setClientSecret(e.target.value)}
            placeholder="paste the client secret" autoComplete="new-password" spellCheck={false}
            style={{ ...inputStyle, fontFamily: "monospace" }} />
          <FieldLabel htmlFor="amazon_sp_refresh_token">Refresh token</FieldLabel>
          <input id="amazon_sp_refresh_token" type="password" value={refreshToken} onChange={(e) => setRefreshToken(e.target.value)}
            placeholder="Atzr|…" autoComplete="new-password" spellCheck={false}
            style={{ ...inputStyle, fontFamily: "monospace" }} />

          {error && (
            <div style={{
              background: "var(--lm-red-dim)", border: "1px solid var(--lm-red-glow)", borderRadius: 8,
              padding: "8px 12px", fontSize: 12.5, color: C.red, marginBottom: 12,
            }}>{error}</div>
          )}

          <div style={{ display: "flex", flexWrap: "wrap", justifyContent: "space-between", gap: 10 }}>
            {connected ? (
              <button type="button" onClick={onDisconnect} disabled={busy !== null}
                style={{ ...buttonBase, color: C.red, cursor: busy ? "not-allowed" : "pointer", opacity: busy ? 0.5 : 1 }}>
                {busy === "disconnect" ? "Disconnecting…" : "Disconnect"}
              </button>
            ) : <span />}
            <button type="button" onClick={onSubmit} disabled={busy !== null || !canSubmit}
              style={{
                padding: "8px 22px", borderRadius: 8, fontSize: 12, fontWeight: 600, border: "none",
                background: busy || !canSubmit ? C.surface : C.amber,
                color: busy || !canSubmit ? C.textMuted : "var(--lm-on-amber)",
                cursor: busy || !canSubmit ? "not-allowed" : "pointer",
              }}>
              {busy === "connect" ? "Connecting…" : connected ? "Reconnect" : "Connect"}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

export function AmazonSection({ active }: { active: boolean }) {
  const [loading, setLoading] = useState(true);
  const [connected, setConnected] = useState(false);
  const [connectedAt, setConnectedAt] = useState(0);
  const [viaDevice, setViaDevice] = useState(false);
  const [directConnected, setDirectConnected] = useState(false);
  const [toolkit, setToolkit] = useState<AmazonToolkitInfo | null>(null);
  const [busy, setBusy] = useState<"install" | "remove" | "disconnect" | null>(null);

  const load = () => {
    setLoading(true);
    Promise.allSettled([getConnector(CONNECTOR_CODE), getAmazonToolkit(), getAmazonStatus()])
      .then(([conn, kit, dev]) => {
        const device = dev.status === "fulfilled" && dev.value.connected;
        setViaDevice(device);
        if (conn.status === "fulfilled") setConnectedAt(conn.value.obtained_at ?? 0);
        setConnected(device || (conn.status === "fulfilled" && !!conn.value.connected));
        if (kit.status === "fulfilled") setToolkit(kit.value);
      })
      .finally(() => setLoading(false));
  };
  useEffect(load, []);

  const onToolkit = async (action: "install" | "remove") => {
    if (action === "install" && !window.confirm(
      "Install Amazon's published skills? The robot downloads them from Amazon's open-source repository on GitHub (markdown files only, nothing is run).",
    )) return;
    if (action === "remove" && !window.confirm("Remove the Amazon skills the robot installed? The Amazon connection itself is not affected.")) return;
    setBusy(action);
    try {
      setToolkit(await setAmazonToolkit(action));
      toast.success(action === "install" ? "Amazon's skills installed." : "Amazon's skills removed.");
    } catch (err) {
      toast.error(err instanceof Error ? err.message : `Could not ${action} the skills.`);
    } finally {
      setBusy(null);
    }
  };

  const onDisconnect = async () => {
    if (!window.confirm("Disconnect Amazon? The robot forgets the saved link until you connect again.")) return;
    setBusy("disconnect");
    try {
      if (viaDevice) await disconnectAmazon();
      else await removeConnector(CONNECTOR_CODE);
      toast.success("Amazon disconnected.");
      setViaDevice(false);
      setConnected(false);
      setConnectedAt(0);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Disconnect failed.");
    } finally {
      setBusy(null);
    }
  };

  const installed = toolkit?.installed ?? [];

  return (
    <div id="section-amazon" className="lm-card lm-fade-in"
      style={{ display: active ? "block" : "none", padding: "22px 24px", marginBottom: 16 }}>
      {loading ? (
        <div style={{ fontSize: 12, color: C.textDim }}>Loading…</div>
      ) : (
        <div>
          <Header connected={connected || directConnected} connectedAt={connectedAt} />

          <AmazonDirectCard onConnected={setDirectConnected} />

          {!connected && (
            <InfoCard tone="blue" title="OPTION 2: AMAZON'S OWN CONNECTOR (BETA)">
              Connect it from <a href="/setting#mcp" style={{ color: C.amber, textDecoration: "underline" }}>Settings → MCP Tools</a>:
              press <b>Connect Amazon</b>, sign in to <b>Seller Central</b> on Amazon&apos;s own page, then paste back the
              address your browser ends on. The card there has the full step-by-step guide. Linking it from your{" "}
              <b>autonomous.ai</b> account also works.
              <div style={{ marginTop: 10 }}>
                <b style={{ color: C.text }}>If Amazon shows &quot;Forbidden Error&quot;:</b>
                <img src="https://cdn.autonomous.ai/production/ecm/261015/amazon-seller-error.png" alt="Amazon Forbidden Error: You don't have permission to access this resource"
                  style={{ display: "block", maxWidth: "100%", margin: "8px 0", borderRadius: 6, border: `1px solid ${C.border}` }} />
                Either the Seller Central account is not allowed to authorize agents (the account owner enables them under{" "}
                <b>Manage Your Permissions → Manage Agents</b>, then press Connect Amazon again), or Amazon&apos;s beta
                connector does not accept this robot yet. If an owner or administrator gets the same error, use{" "}
                <b>Option 1</b> above.
              </div>
            </InfoCard>
          )}
          <InfoCard tone="warning" title="GOOD TO KNOW">
            US stores only for now (Amazon&apos;s beta). The robot reads freely, but before it changes a listing,
            a price, stock or a shipment it tells you exactly what will change and waits for your yes. Amazon may
            also hold a change as a draft for you to approve in Seller Central.
          </InfoCard>

          <div style={{ marginTop: 18, paddingTop: 16, borderTop: `1px solid ${C.border}` }}>
            <div style={{ fontSize: 13, fontWeight: 700, color: C.text, marginBottom: 4 }}>Amazon&apos;s skills (optional)</div>
            <div style={{ fontSize: 12, color: C.textDim, lineHeight: 1.6, marginBottom: 12 }}>
              Amazon publishes ready-made skills for its connector: sales and traffic analytics, listing fixes,
              stockout checks, FBA inbound plans and more. They are optional; the connection works without them.
            </div>

            {toolkit && !toolkit.available ? (
              <div style={{ fontSize: 12, color: C.red }}>
                Not available here: the connectors skill that installs them is missing on this device.
              </div>
            ) : (
              <>
                <div style={{ fontSize: 12, color: C.textDim, marginBottom: 12 }}>
                  {installed.length > 0
                    ? `Installed: ${installed.length} skills (${installed.map((s) => s.skill.replace(/^amazon-/, "")).join(", ")})`
                    : "Not installed."}
                </div>
                <div style={{ display: "flex", flexWrap: "wrap", gap: 10 }}>
                  <button type="button" onClick={() => onToolkit("install")} disabled={busy !== null}
                    style={{
                      padding: "8px 20px", borderRadius: 8, fontSize: 12, fontWeight: 600, border: "none",
                      background: busy ? C.surface : C.amber, color: busy ? C.textMuted : "var(--lm-on-amber)",
                      cursor: busy ? "not-allowed" : "pointer",
                    }}>
                    {busy === "install" ? "Installing…" : installed.length > 0 ? "Reinstall" : "Install Amazon's skills"}
                  </button>
                  {installed.length > 0 && (
                    <button type="button" onClick={() => onToolkit("remove")} disabled={busy !== null}
                      style={{ ...buttonBase, cursor: busy ? "not-allowed" : "pointer", opacity: busy ? 0.5 : 1 }}>
                      {busy === "remove" ? "Removing…" : "Remove"}
                    </button>
                  )}
                </div>
              </>
            )}
          </div>

          {connected && (
            <div style={{ marginTop: 18, paddingTop: 16, borderTop: `1px solid ${C.border}` }}>
              <button type="button" onClick={onDisconnect} disabled={busy !== null}
                style={{ ...buttonBase, color: C.red, cursor: busy ? "not-allowed" : "pointer", opacity: busy ? 0.5 : 1 }}>
                {busy === "disconnect" ? "Disconnecting…" : "Disconnect"}
              </button>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
