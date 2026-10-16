import { useEffect, useState } from "react";
import { toast } from "sonner";
import { C, SectionCard, LABEL_STYLE, INPUT_STYLE } from "@/components/setup/shared";
import { listMCPTools, addMCPTool, removeMCPTool, startFoConnect, finishFoConnect, getFoStatus, getAmazonStatus, startAmazonConnect, finishAmazonConnect, cancelAmazonConnect, disconnectAmazon } from "@/lib/api";
import type { MCPTool, FoStatus, AmazonStatus } from "@/lib/api";

import "./settings-lists.css";

type HeaderRow = { key: string; value: string };

export function MCPToolsSection({ active }: { active: boolean }) {
  const [tools, setTools] = useState<MCPTool[]>([]);
  const [loading, setLoading] = useState(true);
  const [adding, setAdding] = useState(false);
  const [removing, setRemoving] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [url, setUrl] = useState("");
  const [headers, setHeaders] = useState<HeaderRow[]>([]);
  const [foAuthURL, setFoAuthURL] = useState("");
  const [foCallback, setFoCallback] = useState("");
  const [foBusy, setFoBusy] = useState(false);
  const [foStatus, setFoStatus] = useState<FoStatus>({ connected: false, needs_reconnect: false });
  const [amazon, setAmazon] = useState<AmazonStatus>({ supported: true, connected: false, pending: false });
  const [amazonAuthURL, setAmazonAuthURL] = useState("");
  const [amazonCallback, setAmazonCallback] = useState("");
  const [amazonBusy, setAmazonBusy] = useState(false);

  useEffect(() => {
    listMCPTools()
      .then(setTools)
      .catch(() => {})
      .finally(() => setLoading(false));
    getFoStatus().then(setFoStatus).catch(() => {});
    getAmazonStatus().then(setAmazon).catch(() => {});
  }, []);

  function addHeaderRow() { setHeaders((prev) => [...prev, { key: "", value: "" }]); }
  function removeHeaderRow(i: number) { setHeaders((prev) => prev.filter((_, idx) => idx !== i)); }
  function updateHeader(i: number, field: "key" | "value", val: string) {
    setHeaders((prev) => prev.map((h, idx) => idx === i ? { ...h, [field]: val } : h));
  }

  function buildHeaders(): Record<string, string> | undefined {
    const h: Record<string, string> = {};
    for (const r of headers) {
      const k = r.key.trim();
      const v = r.value.trim();
      if (k && v) h[k] = v;
    }
    return Object.keys(h).length > 0 ? h : undefined;
  }

  async function handleAdd() {
    const n = name.trim();
    const u = url.trim();
    if (!n || !u) { toast.error("Name and URL are required."); return; }
    setAdding(true);
    const hdrs = buildHeaders();
    try {
      await addMCPTool({ name: n, url: u, headers: hdrs });
      setTools((prev) => [...prev, { name: n, url: u, headers: hdrs }]);
      setName("");
      setUrl("");
      setHeaders([]);
      toast.success(`Added MCP tool "${n}".`);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to add tool.");
    } finally {
      setAdding(false);
    }
  }

  async function handleRemove(toolName: string) {
    setRemoving(toolName);
    try {
      await removeMCPTool(toolName);
      setTools((prev) => prev.filter((t) => t.name !== toolName));
      if (toolName === "fo") setFoStatus({ connected: false, needs_reconnect: false });
      toast.success(`Removed "${toolName}".`);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to remove tool.");
    } finally {
      setRemoving(null);
    }
  }

  async function handleFoStart() {
    setFoBusy(true);
    try {
      const { auth_url } = await startFoConnect();
      setFoAuthURL(auth_url);
      window.open(auth_url, "_blank", "noopener");
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to start Fo sign-in.");
    } finally {
      setFoBusy(false);
    }
  }

  async function handleFoFinish() {
    setFoBusy(true);
    try {
      await finishFoConnect(foCallback.trim());
      setTools(await listMCPTools());
      setFoStatus(await getFoStatus());
      setFoAuthURL("");
      setFoCallback("");
      toast.success("Fo connected.");
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to connect Fo.");
    } finally {
      setFoBusy(false);
    }
  }

  async function handleAmazonStart() {
    setAmazonBusy(true);
    try {
      const { auth_url } = await startAmazonConnect();
      setAmazonAuthURL(auth_url);
      window.open(auth_url, "_blank", "noopener");
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to start the Amazon sign-in.");
    } finally {
      setAmazonBusy(false);
    }
  }

  async function handleAmazonFinish() {
    setAmazonBusy(true);
    try {
      await finishAmazonConnect(amazonCallback.trim());
      setAmazon(await getAmazonStatus());
      setAmazonAuthURL("");
      setAmazonCallback("");
      toast.success("Amazon connected.");
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to connect Amazon.");
    } finally {
      setAmazonBusy(false);
    }
  }

  async function handleAmazonDisconnect() {
    if (!window.confirm("Disconnect Amazon? The robot forgets the saved sign-in until you connect again.")) return;
    setAmazonBusy(true);
    try {
      await disconnectAmazon();
      setAmazon(await getAmazonStatus());
      toast.success("Amazon disconnected.");
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to disconnect Amazon.");
    } finally {
      setAmazonBusy(false);
    }
  }

  const headerCount = (t: MCPTool) => t.headers ? Object.keys(t.headers).length : 0;

  const BTN: React.CSSProperties = {
    padding: "6px 14px", borderRadius: 8, fontSize: 13, fontWeight: 500,
    cursor: "pointer", border: `1px solid ${C.border}`, background: C.surface,
    color: C.text,
  };

  const SMALL_BTN: React.CSSProperties = {
    padding: "3px 8px", borderRadius: 6, fontSize: 12, fontWeight: 500,
    cursor: "pointer", border: `1px solid ${C.border}`, background: C.surface,
    color: C.textDim,
  };

  return (
    <SectionCard id="mcp" title="MCP Tools" active={active}>
      <div style={{ fontSize: 12.5, color: C.textDim, marginBottom: 14, lineHeight: 1.6 }}>
        Remote MCP tool endpoints the agent can call over HTTPS. Add public
        tools (HF Spaces, community servers) or authenticated tools (with custom headers).
        Each tool is synced to the active runtime and takes effect immediately.
      </div>

      {loading ? (
        <div style={{ fontSize: 12, color: C.textDim }}>Loading…</div>
      ) : (
        <>
          {tools.length > 0 && (
            <div style={{ marginBottom: 16 }}>
              {tools.map((t) => (
                <div
                  key={t.name}
                  className="lm-settings-list-row"
                  style={{
                    display: "flex", alignItems: "center", justifyContent: "space-between",
                    gap: 8, padding: "10px 12px", marginBottom: 6,
                    background: C.surface, border: `1px solid ${C.border}`, borderRadius: 8,
                  }}
                >
                  <div style={{ minWidth: 0, flex: 1 }}>
                    <div style={{ fontSize: 13, fontWeight: 600, color: C.text, display: "flex", flexWrap: "wrap", alignItems: "center", gap: 6 }}>
                      <span className="lm-settings-list-name">{t.name}</span>
                      {headerCount(t) > 0 && (
                        <span style={{ fontSize: 12, color: C.amber, fontWeight: 400 }}>
                          {headerCount(t)} header{headerCount(t) > 1 ? "s" : ""}
                        </span>
                      )}
                    </div>
                    <div style={{
                      fontSize: 12, color: C.textDim,
                      overflowWrap: "anywhere",
                    }}>
                      {t.url}
                    </div>
                  </div>
                  <button
                    type="button"
                    aria-label={`Remove MCP tool ${t.name}`}
                    onClick={() => handleRemove(t.name)}
                    disabled={removing === t.name}
                    style={{ ...BTN, color: "#ef4444", flexShrink: 0, opacity: removing === t.name ? 0.5 : 1, cursor: removing === t.name ? "not-allowed" : "pointer" }}
                  >
                    {removing === t.name ? "Removing…" : "Remove"}
                  </button>
                </div>
              ))}
            </div>
          )}

          <div style={{
            marginBottom: 16, padding: "10px 12px", background: C.surface,
            border: `1px solid ${C.border}`, borderRadius: 8,
          }}>
            <div style={{ fontSize: 13, fontWeight: 600, color: C.text, marginBottom: 4, display: "flex", gap: 8, alignItems: "center" }}>
              Fo (Wajo)
              {foStatus.connected && !foStatus.needs_reconnect && (
                <span style={{ fontSize: 12, fontWeight: 400, color: "#22c55e" }}>Connected</span>
              )}
              {foStatus.needs_reconnect && (
                <span style={{ fontSize: 12, fontWeight: 400, color: C.amber }}>Sign-in expired</span>
              )}
            </div>
            {foStatus.needs_reconnect && (
              <div role="alert" style={{ fontSize: 12, color: C.amber, lineHeight: 1.6, marginBottom: 8 }}>
                Wajo no longer accepts this device&apos;s sign-in, so the agent cannot reach Fo.
                Press Reconnect and sign in again; the agent keeps running meanwhile.
              </div>
            )}
            <div style={{ fontSize: 12, color: C.textDim, lineHeight: 1.6, marginBottom: 8 }}>
              Let the agent hand errands (email, calendar, calls, purchases) to your Fo
              assistant. Needs a Wajo account with Fo Pro.
            </div>
            {!foAuthURL ? (
              <button type="button" onClick={handleFoStart} disabled={foBusy} style={{ ...BTN, opacity: foBusy ? 0.5 : 1 }}>
                {foBusy ? "Opening…" : foStatus.connected ? "Reconnect" : "Connect Fo"}
              </button>
            ) : (
              <>
                <div style={{ fontSize: 12, color: C.textDim, lineHeight: 1.6, marginBottom: 6 }}>
                  1. <a href={foAuthURL} target="_blank" rel="noopener noreferrer" style={{ color: C.amber }}>Open the Wajo sign-in</a> and allow access.<br />
                  2. Your browser then lands on a <b>localhost</b> page that does not load. That is expected:
                  copy the full address from the address bar and paste it below.
                </div>
                <label htmlFor="fo-callback" style={LABEL_STYLE}>Address after sign-in</label>
                <input
                  id="fo-callback"
                  type="text"
                  value={foCallback}
                  onChange={(e) => setFoCallback(e.target.value)}
                  placeholder="http://localhost:5000/oauth/fo/callback?code=…"
                  style={INPUT_STYLE}
                />
                <button
                  type="button"
                  onClick={handleFoFinish}
                  disabled={foBusy || !foCallback.trim()}
                  style={{ ...BTN, marginTop: 8, opacity: foBusy || !foCallback.trim() ? 0.5 : 1 }}
                >
                  {foBusy ? "Connecting…" : "Finish"}
                </button>
              </>
            )}
          </div>

          <div style={{
            marginBottom: 16, padding: "10px 12px", background: C.surface,
            border: `1px solid ${C.border}`, borderRadius: 8,
          }}>
            <div style={{ fontSize: 13, fontWeight: 600, color: C.text, marginBottom: 4, display: "flex", gap: 8, alignItems: "center" }}>
              Amazon (Selling Partner)
              <span style={{ fontSize: 11, fontWeight: 600, color: C.amber, border: `1px solid ${C.amber}`, borderRadius: 6, padding: "0 6px" }}>Beta</span>
              {amazon.connected && <span style={{ fontSize: 12, fontWeight: 400, color: "#22c55e" }}>Connected</span>}
            </div>
            <div style={{ fontSize: 12, color: C.textDim, lineHeight: 1.6, marginBottom: 8 }}>
              Let the agent read and manage your Amazon seller account: sales, inventory, listings and FBA.
              US stores only for now. It asks you before it changes a listing, a price, stock or a shipment.
            </div>
            {!amazon.supported ? (
              <div style={{ fontSize: 12, color: C.amber, lineHeight: 1.6 }}>{amazon.reason}</div>
            ) : (
              <>
                <details style={{ marginBottom: 10 }}>
                  <summary style={{ fontSize: 12, color: C.amber, cursor: "pointer" }}>How to connect Amazon (step by step)</summary>
                  <div style={{ fontSize: 12, color: C.textDim, lineHeight: 1.7, marginTop: 8 }}>
                    <b style={{ color: C.text }}>Once, in Seller Central:</b> the account owner, an administrator or a
                    permissions manager must allow agents: <b>Manage Your Permissions → Manage Agents</b>, then enable the
                    Selling Partner agent. Owners and administrators can always authorize it; any other user needs to be
                    granted access there first.
                    <ol style={{ margin: "8px 0 8px 18px", padding: 0, listStyle: "decimal" }}>
                      <li>Press <b>Connect Amazon</b>. A sign-in link opens (it also stays on this page).</li>
                      <li>Sign in to <b>Seller Central</b> with the account you want the robot to use and approve access.</li>
                      <li>
                        Your browser then lands on an address that starts with <b>http://127.0.0.1</b> and says it can&apos;t
                        be reached. That is expected: copy the <b>whole address</b> from the address bar.
                      </li>
                      <li>Paste it in the box on this card and press <b>Finish</b>. The agent restarts for a few seconds to load Amazon.</li>
                    </ol>
                    The link works once and for about 10 minutes; if it expires, press Connect Amazon again. If Amazon
                    says to ask an administrator, the Manage Agents step above is missing. Press <b>Disconnect</b> any
                    time to remove the link.
                    <div style={{ marginTop: 10, paddingTop: 8, borderTop: `1px solid ${C.border}` }}>
                      <b style={{ color: C.text }}>If Amazon shows &quot;Forbidden Error&quot;</b> after you sign in:
                      <img src="/amazon-guide/amazon-seller-error.png" alt="Amazon Forbidden Error: You don't have permission to access this resource"
                        style={{ display: "block", maxWidth: "100%", margin: "8px 0", borderRadius: 6, border: `1px solid ${C.border}` }} />
                      Amazon refused the sign-in. Two things cause it. <b>1)</b> The Seller Central account is not allowed to
                      authorize agents: ask the account owner to enable them under <b>Manage Your Permissions → Manage
                      Agents</b> (and grant your user there), wait a few minutes, then press Connect Amazon again for a new link.
                      <b> 2)</b> Amazon&apos;s connector is in beta and may not accept this robot yet. If the owner or an
                      administrator also gets this error, use <b>Option 1</b> (your own Amazon app) on Settings →
                      Connectors → Amazon instead.
                    </div>
                  </div>
                </details>
                {!amazonAuthURL ? (
                  <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
                    <button type="button" onClick={handleAmazonStart} disabled={amazonBusy} style={{ ...BTN, opacity: amazonBusy ? 0.5 : 1 }}>
                      {amazonBusy ? "Opening…" : amazon.connected ? "Reconnect" : "Connect Amazon"}
                    </button>
                    {amazon.connected && (
                      <button type="button" onClick={handleAmazonDisconnect} disabled={amazonBusy}
                        style={{ ...BTN, color: "#ef4444", opacity: amazonBusy ? 0.5 : 1 }}>
                        Disconnect
                      </button>
                    )}
                  </div>
                ) : (
                  <>
                    <div style={{ fontSize: 12, color: C.textDim, lineHeight: 1.6, marginBottom: 6 }}>
                      1. <a href={amazonAuthURL} target="_blank" rel="noopener noreferrer" style={{ color: C.amber }}>Open the Amazon sign-in</a> and approve access.<br />
                      2. Your browser then lands on a <b>127.0.0.1</b> page that does not load. That is expected:
                      copy the full address from the address bar and paste it below.
                    </div>
                    <label htmlFor="amazon-callback" style={LABEL_STYLE}>Address after sign-in</label>
                    <input
                      id="amazon-callback"
                      type="text"
                      value={amazonCallback}
                      onChange={(e) => setAmazonCallback(e.target.value)}
                      placeholder="http://127.0.0.1:27890/callback?code=…"
                      style={INPUT_STYLE}
                    />
                    <div style={{ display: "flex", gap: 8, marginTop: 8 }}>
                      <button type="button" onClick={handleAmazonFinish} disabled={amazonBusy || !amazonCallback.trim()}
                        style={{ ...BTN, opacity: amazonBusy || !amazonCallback.trim() ? 0.5 : 1 }}>
                        {amazonBusy ? "Connecting…" : "Finish"}
                      </button>
                      <button type="button" onClick={() => { setAmazonAuthURL(""); setAmazonCallback(""); cancelAmazonConnect().catch(() => {}); }} disabled={amazonBusy} style={BTN}>
                        Cancel
                      </button>
                    </div>
                  </>
                )}
              </>
            )}
          </div>

          {tools.length === 0 && (
            <div style={{ fontSize: 12, color: C.textDim, marginBottom: 14 }}>
              No MCP tools configured yet.
            </div>
          )}

          <div style={{ marginBottom: 6 }}>
            <label htmlFor="mcp-name" style={LABEL_STYLE}>Name</label>
            <input
              id="mcp-name"
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="search"
              style={INPUT_STYLE}
            />
          </div>
          <div style={{ marginBottom: 6 }}>
            <label htmlFor="mcp-url" style={LABEL_STYLE}>URL</label>
            <input
              id="mcp-url"
              type="text"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              placeholder="https://owner-space.hf.space/gradio_api/mcp/"
              style={INPUT_STYLE}
            />
          </div>

          <div style={{ marginBottom: 10 }}>
            <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 6 }}>
              <span style={LABEL_STYLE}>Headers <span style={{ color: C.textDim, fontWeight: 400 }}>(optional)</span></span>
              <button type="button" onClick={addHeaderRow} style={SMALL_BTN}>+ Add</button>
            </div>
            {headers.map((h, i) => (
              <div key={i} className="lm-mcp-header-row">
                <div>
                  <label htmlFor={`mcp-header-key-${i}`} style={LABEL_STYLE}>Header name</label>
                  <input
                    id={`mcp-header-key-${i}`}
                    type="text"
                    value={h.key}
                    onChange={(e) => updateHeader(i, "key", e.target.value)}
                    placeholder="Authorization"
                    style={INPUT_STYLE}
                  />
                </div>
                <div>
                  <label htmlFor={`mcp-header-value-${i}`} style={LABEL_STYLE}>Header value</label>
                  <input
                    id={`mcp-header-value-${i}`}
                    type="password"
                    value={h.value}
                    onChange={(e) => updateHeader(i, "value", e.target.value)}
                    placeholder="Bearer sk-..."
                    style={INPUT_STYLE}
                  />
                </div>
                <button
                  type="button"
                  aria-label={`Remove header ${i + 1}`}
                  onClick={() => removeHeaderRow(i)}
                  style={{ ...BTN, color: "#ef4444" }}
                >Remove</button>
              </div>
            ))}
          </div>

          <button
            type="button"
            onClick={handleAdd}
            disabled={adding || !name.trim() || !url.trim()}
            style={{
              ...BTN,
              background: C.amber, color: "#000", borderColor: C.amber,
              opacity: adding || !name.trim() || !url.trim() ? 0.5 : 1,
              cursor: adding ? "not-allowed" : "pointer",
            }}
          >
            {adding ? "Adding…" : "Add Tool"}
          </button>
        </>
      )}
    </SectionCard>
  );
}
