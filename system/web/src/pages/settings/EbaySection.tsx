import { useEffect, useState } from "react";
import { toast } from "sonner";
import { Store, Eye, EyeOff, Lock, ExternalLink, ChevronDown, ChevronRight, Copy, Check } from "lucide-react";
import { copyText } from "@/lib/clipboard";
import { C } from "@/components/setup/shared";
import { getConnector, setEbay, removeConnector } from "@/lib/api";
import type { ConnectorInfo } from "@/lib/api";
import { FieldLabel, InfoCard, Step } from "@/pages/settings/connectorFormParts";

const CONNECTOR_CODE = "ebay";

type Environment = "production" | "sandbox";

const MARKETPLACES: { id: string; label: string }[] = [
  { id: "EBAY_US", label: "United States (ebay.com)" },
  { id: "EBAY_GB", label: "United Kingdom (ebay.co.uk)" },
  { id: "EBAY_DE", label: "Germany (ebay.de)" },
  { id: "EBAY_AU", label: "Australia (ebay.com.au)" },
  { id: "EBAY_CA", label: "Canada (ebay.ca)" },
  { id: "EBAY_FR", label: "France (ebay.fr)" },
  { id: "EBAY_IT", label: "Italy (ebay.it)" },
  { id: "EBAY_ES", label: "Spain (ebay.es)" },
];

const marketplaceLabel = (id: string) => MARKETPLACES.find((m) => m.id === id)?.label ?? id;

// What the robot may do in the seller's account: listings and stock, business
// policies (to publish) and orders. The same scope URLs apply to Sandbox.
const SCOPES = [
  "https://api.ebay.com/oauth/api_scope",
  "https://api.ebay.com/oauth/api_scope/sell.inventory",
  "https://api.ebay.com/oauth/api_scope/sell.account",
  "https://api.ebay.com/oauth/api_scope/sell.fulfillment",
];

/** eBay's consent page for this app; it sends the browser on to the RuName's accept URL with ?code=. */
function signInUrl(environment: Environment, clientId: string, ruName: string): string {
  const host = environment === "sandbox" ? "auth.sandbox.ebay.com" : "auth.ebay.com";
  const q = new URLSearchParams({
    client_id: clientId.trim(),
    response_type: "code",
    redirect_uri: ruName.trim(),
    scope: SCOPES.join(" "),
    prompt: "login",
  });
  return `https://${host}/oauth2/authorize?${q.toString()}`;
}

/**
 * Settings card for the eBay seller connector. The owner registers their own
 * eBay developer app and pastes its keys plus a refresh token; the connectors
 * skill mints short-lived access tokens from them on the robot.
 */
export function EbaySection({ active }: { active: boolean }) {
  const [loading, setLoading] = useState(true);
  const [connected, setConnected] = useState(false);
  const [linked, setLinked] = useState<{ marketplace: string; environment: string; at: number }>({
    marketplace: "",
    environment: "",
    at: 0,
  });
  const [clientId, setClientId] = useState("");
  const [clientSecret, setClientSecret] = useState("");
  const [ruName, setRuName] = useState("");
  // The address eBay sent the browser to after sign-in (or just its code).
  const [callback, setCallback] = useState("");
  // Fallback for owners who already hold a refresh token.
  const [useToken, setUseToken] = useState(false);
  const [refreshToken, setRefreshToken] = useState("");
  const [marketplace, setMarketplace] = useState("EBAY_US");
  const [environment, setEnvironment] = useState<Environment>("production");
  const [showSecrets, setShowSecrets] = useState(false);
  const [guideOpen, setGuideOpen] = useState<boolean>(() => {
    try { return localStorage.getItem("ebay-guide-open") === "1"; } catch { return false; }
  });
  useEffect(() => {
    try { localStorage.setItem("ebay-guide-open", guideOpen ? "1" : "0"); } catch { /* private mode: silently skip */ }
  }, [guideOpen]);
  const [submitting, setSubmitting] = useState(false);
  const [disconnecting, setDisconnecting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = () => {
    setLoading(true);
    getConnector(CONNECTOR_CODE)
      .then((r: ConnectorInfo) => {
        setConnected(!!r.connected);
        setLinked({
          marketplace: r.credentials?.marketplace_id ?? "",
          environment: r.credentials?.environment ?? "",
          at: r.obtained_at ?? 0,
        });
        if (r.credentials?.client_id) setClientId(r.credentials.client_id);
        if (r.credentials?.ru_name) setRuName(r.credentials.ru_name);
        if (r.credentials?.marketplace_id) setMarketplace(r.credentials.marketplace_id);
        if (r.credentials?.environment === "sandbox") setEnvironment("sandbox");
      })
      .catch(() => {
        // A never-set connector returns connected:false; stay silent on 500.
      })
      .finally(() => setLoading(false));
  };
  useEffect(load, []);

  const reset = () => {
    setClientSecret("");
    setCallback("");
    setRefreshToken("");
    setError(null);
    setShowSecrets(false);
  };

  const canSignIn = !!clientId.trim() && !!ruName.trim();
  const ready = !!clientId.trim() && !!clientSecret.trim()
    && (useToken ? !!refreshToken.trim() : !!ruName.trim() && !!callback.trim());

  // eBay names keysets "<app>-PRD-…" or "<app>-SBX-…".
  const keysetMismatch = (): string | null => {
    const id = clientId.trim();
    const keyset = /-SBX-/i.test(id) ? "sandbox" : /-PRD-/i.test(id) ? "production" : null;
    if (keyset && keyset !== environment) {
      return `This App ID is a ${keyset} key. Switch the environment to ${keyset === "sandbox" ? "Sandbox" : "Production"} or use your ${environment} keys.`;
    }
    return null;
  };

  const openSignIn = () => {
    setError(null);
    const mismatch = keysetMismatch();
    if (mismatch) { setError(mismatch); return; }
    window.open(signInUrl(environment, clientId, ruName), "_blank", "noopener");
  };

  // Click handler, not a form submit: this renders inside SettingsPanel's form, and nested forms never submit.
  const onSubmit = async (e: React.MouseEvent | React.KeyboardEvent) => {
    e.preventDefault();
    setError(null);
    const id = clientId.trim();
    const secret = clientSecret.trim();
    const token = refreshToken.trim();
    if (!ready) {
      setError(useToken
        ? "App ID, Cert ID and refresh token are all required."
        : "Fill in the App ID, Cert ID and RuName, sign in with eBay, then paste the address it sent you to.");
      return;
    }
    const mismatch = keysetMismatch();
    if (mismatch) { setError(mismatch); return; }
    if (!useToken && !/code=/.test(callback) && /[?/]/.test(callback)) {
      setError("That address has no code= in it. Sign in with eBay again and copy the address of the page you land on.");
      return;
    }
    setSubmitting(true);
    try {
      await setEbay({
        environment,
        marketplace_id: marketplace,
        client_id: id,
        client_secret: secret,
        ru_name: ruName.trim(),
        ...(useToken ? { refresh_token: token } : { code: callback.trim() }),
      });
      toast.success("eBay connected.");
      reset();
      load();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Save failed.");
    } finally {
      setSubmitting(false);
    }
  };

  const onDisconnect = async () => {
    if (!connected) return;
    if (!window.confirm("Disconnect eBay? The robot will stop managing your listings and orders until you connect again.")) return;
    setDisconnecting(true);
    try {
      await removeConnector(CONNECTOR_CODE);
      toast.success("eBay disconnected.");
      setConnected(false);
      setLinked({ marketplace: "", environment: "", at: 0 });
      reset();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Disconnect failed.");
    } finally {
      setDisconnecting(false);
    }
  };

  const linkStyle: React.CSSProperties = { color: C.amber, display: "inline-flex", alignItems: "center", gap: 3 };

  return (
    <div
      id="section-ebay"
      className="lm-card lm-fade-in"
      style={{
        display: active ? "block" : "none",
        padding: "22px 24px",
        marginBottom: 16,
      }}
    >
      {loading ? (
        <div style={{ fontSize: 12, color: C.textDim }}>Loading…</div>
      ) : (
        <div
          onKeyDown={(e) => {
            const target = e.target as HTMLElement;
            if (e.key === "Enter" && target.tagName === "INPUT") {
              if (!submitting && ready) onSubmit(e);
            }
          }}
        >
          <div style={{ display: "flex", alignItems: "flex-start", gap: 14, marginBottom: 18 }}>
            <div style={{
              flexShrink: 0,
              width: 44, height: 44, borderRadius: 12,
              background: "#0064D2",
              display: "flex", alignItems: "center", justifyContent: "center",
            }}>
              <Store size={24} color="#fff" strokeWidth={2.2} />
            </div>
            <div style={{ minWidth: 0, flex: 1 }}>
              <div style={{ fontSize: 16, fontWeight: 700, color: C.text, marginBottom: 3 }}>
                Connect eBay
              </div>
              <div style={{ fontSize: 12, color: C.textDim, lineHeight: 1.5 }}>
                Let the robot check and update your eBay listings, stock, prices and orders.
              </div>
              {connected && (
                <div style={{ marginTop: 8, fontSize: 12, color: C.green, display: "inline-flex", flexWrap: "wrap", alignItems: "center", gap: 6 }}>
                  ✓ connected
                  {linked.marketplace && <span style={{ color: C.textDim }}>· {marketplaceLabel(linked.marketplace)}</span>}
                  {linked.environment === "sandbox" && <span style={{ color: C.amber }}>· Sandbox</span>}
                  {linked.at > 0 && (
                    <span style={{ color: C.textDim }}>
                      · {new Date(linked.at * 1000).toLocaleDateString()}
                    </span>
                  )}
                </div>
              )}
            </div>
          </div>

          <InfoCard tone="blue" title="BEFORE YOU START">
            You need an eBay <b>seller account</b> and a free <b>eBay developer account</b>{" "}
            to create your own app keys. The robot asks you to confirm before it
            changes a listing, a price or an order.
          </InfoCard>

          <FieldLabel htmlFor="ebay_environment">Environment</FieldLabel>
          <div id="ebay_environment" role="radiogroup" style={{ display: "flex", flexWrap: "wrap", gap: 8, marginBottom: 14 }}>
            {(["production", "sandbox"] as const).map((env) => (
              <button
                key={env}
                type="button"
                role="radio"
                aria-checked={environment === env}
                onClick={() => setEnvironment(env)}
                style={{
                  padding: "7px 14px", borderRadius: 8, fontSize: 12, fontWeight: 600,
                  background: environment === env ? C.amberDim : "none",
                  border: `1px solid ${environment === env ? C.amber : C.border}`,
                  color: environment === env ? C.amber : C.text,
                  cursor: "pointer",
                }}
              >
                {env === "production" ? "Production (my real shop)" : "Sandbox (test)"}
              </button>
            ))}
          </div>

          <FieldLabel htmlFor="ebay_marketplace">Marketplace</FieldLabel>
          <select
            id="ebay_marketplace"
            value={marketplace}
            onChange={(e) => setMarketplace(e.target.value)}
            style={{ ...inputStyle, marginBottom: 14, cursor: "pointer" }}
          >
            {MARKETPLACES.map((m) => (
              <option key={m.id} value={m.id}>{m.label}</option>
            ))}
          </select>

          <FieldLabel htmlFor="ebay_client_id">App ID (Client ID)</FieldLabel>
          <input
            id="ebay_client_id"
            type="text"
            value={clientId}
            onChange={(e) => setClientId(e.target.value)}
            placeholder="e.g. YourName-MyShop-PRD-1a2b3c4d5-6e7f8a9b"
            autoComplete="off"
            spellCheck={false}
            style={{ ...inputStyle, marginBottom: 14, fontFamily: "monospace" }}
          />

          <div style={{ display: "flex", alignItems: "flex-end", justifyContent: "space-between", marginBottom: 5 }}>
            <FieldLabel htmlFor="ebay_client_secret">Cert ID (Client Secret)</FieldLabel>
            <button
              type="button"
              onClick={() => setShowSecrets((v) => !v)}
              aria-label={showSecrets ? "Hide secrets" : "Show secrets"}
              style={{ background: "none", border: "none", padding: 0, color: C.textDim, cursor: "pointer", display: "flex", alignItems: "center" }}
            >
              {showSecrets ? <EyeOff size={14} /> : <Eye size={14} />}
            </button>
          </div>
          <input
            id="ebay_client_secret"
            type={showSecrets ? "text" : "password"}
            value={clientSecret}
            onChange={(e) => setClientSecret(e.target.value)}
            placeholder={connected ? "paste again to reconnect" : "e.g. PRD-1a2b3c4d5e6f-…"}
            autoComplete="off"
            spellCheck={false}
            style={{ ...inputStyle, marginBottom: 14, fontFamily: "monospace" }}
          />

          <FieldLabel htmlFor="ebay_ru_name">RuName (eBay redirect URL name)</FieldLabel>
          <input
            id="ebay_ru_name"
            type="text"
            value={ruName}
            onChange={(e) => setRuName(e.target.value)}
            placeholder="e.g. YourName-MyShop-MyShop-abcdefgh"
            autoComplete="off"
            spellCheck={false}
            style={{ ...inputStyle, marginBottom: 6, fontFamily: "monospace" }}
          />
          <div style={{ fontSize: 12, color: C.textDim, marginBottom: 14, lineHeight: 1.5 }}>
            Create it under <b>User Tokens</b> → <b>+ Add eBay Redirect URL</b>; step 4 of{" "}
            <b>How to get your eBay keys</b> below shows what to fill in.
          </div>

          {!useToken ? (
            <>
              <FieldLabel htmlFor="ebay_sign_in">Sign in to your eBay seller account</FieldLabel>
              <div style={{ display: "flex", flexWrap: "wrap", alignItems: "center", gap: 10, marginBottom: 6 }}>
                <button
                  id="ebay_sign_in"
                  type="button"
                  onClick={openSignIn}
                  disabled={!canSignIn}
                  style={{
                    padding: "8px 16px", borderRadius: 8, fontSize: 12, fontWeight: 600,
                    background: canSignIn ? C.amberDim : C.surface,
                    border: `1px solid ${canSignIn ? C.amber : C.border}`,
                    color: canSignIn ? C.amber : C.textMuted,
                    cursor: canSignIn ? "pointer" : "not-allowed",
                    display: "inline-flex", alignItems: "center", gap: 6,
                  }}
                >
                  Sign in with eBay <ExternalLink size={11} />
                </button>
                <span style={{ fontSize: 12, color: C.textDim }}>
                  {environment === "sandbox" ? "Use your Sandbox test user." : "Use the account that owns your shop."}
                </span>
              </div>
              <div style={{ fontSize: 12, color: C.textDim, marginBottom: 10, lineHeight: 1.5 }}>
                After you agree, eBay opens your accept page. Copy that page&apos;s whole address
                (it contains <code>code=</code>) and paste it below within five minutes.
              </div>
              <FieldLabel htmlFor="ebay_callback">Address after signing in</FieldLabel>
              <input
                id="ebay_callback"
                type="text"
                value={callback}
                onChange={(e) => setCallback(e.target.value)}
                placeholder="https://…?code=v%5E1.1…&expires_in=299"
                autoComplete="off"
                spellCheck={false}
                style={{ ...inputStyle, marginBottom: 8, fontFamily: "monospace" }}
              />
            </>
          ) : (
            <>
              <div style={{ display: "flex", alignItems: "flex-end", justifyContent: "space-between", marginBottom: 5 }}>
                <FieldLabel htmlFor="ebay_refresh_token">Refresh token</FieldLabel>
                <div style={{ fontSize: 12, color: C.textDim }}>
                  <TokenCounter length={refreshToken.length} />
                </div>
              </div>
              <input
                id="ebay_refresh_token"
                type={showSecrets ? "text" : "password"}
                value={refreshToken}
                onChange={(e) => setRefreshToken(e.target.value)}
                placeholder="starts with v^1.1#"
                autoComplete="off"
                spellCheck={false}
                style={{ ...inputStyle, marginBottom: 8, fontFamily: "monospace" }}
              />
            </>
          )}
          <button
            type="button"
            onClick={() => { setUseToken((v) => !v); setError(null); }}
            style={{
              background: "none", border: "none", padding: 0, marginBottom: 14,
              color: C.amber, cursor: "pointer", font: "inherit", fontSize: 12,
              textDecoration: "underline", textUnderlineOffset: 2,
            }}
          >
            {useToken ? "Sign in with eBay instead" : "I already have a refresh token"}
          </button>

          <div style={{
            display: "flex", alignItems: "center", gap: 6,
            fontSize: 12, color: C.amber, marginBottom: 16,
          }}>
            <Lock size={11} /> Stored on your robot only. Never uploaded to our servers.
          </div>

          <div style={{
            background: C.bg, border: `1px solid ${C.border}`, borderRadius: 10,
            padding: "10px 14px", marginBottom: 12,
          }}>
            <div style={{ display: "flex", flexWrap: "wrap", alignItems: "center", justifyContent: "space-between", gap: 8 }}>
              <button
                type="button"
                onClick={() => setGuideOpen((v) => !v)}
                style={{
                  background: "none", border: "none", padding: 0, cursor: "pointer",
                  display: "inline-flex", flexWrap: "wrap", alignItems: "center", gap: 6,
                  color: C.textDim, fontSize: 12, fontWeight: 700, letterSpacing: "0.05em",
                }}
                aria-expanded={guideOpen}
              >
                {guideOpen ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
                HOW TO GET YOUR EBAY KEYS
              </button>
              <a
                href="https://developer.ebay.com/my/keys"
                target="_blank"
                rel="noreferrer"
                style={{ color: C.amber, fontSize: 12.5, display: "inline-flex", alignItems: "center", gap: 4, textDecoration: "none" }}
              >
                Open eBay Developer Portal <ExternalLink size={11} />
              </a>
            </div>
            {guideOpen && (
              <div style={{ marginTop: 12, fontSize: 12.5, color: C.textDim, lineHeight: 1.6 }}>
                <Step n={1}>
                  Join the{" "}
                  <a href="https://developer.ebay.com/signin" target="_blank" rel="noreferrer" style={linkStyle}>
                    eBay Developers Program <ExternalLink size={10} />
                  </a>
                  . Activation can take up to one business day.
                </Step>
                <Step n={2}>
                  On <b>Application Keys</b>, create a keyset for{" "}
                  <b>{environment === "sandbox" ? "Sandbox" : "Production"}</b>. If eBay asks
                  about <b>marketplace account deletion</b> notifications, choose the
                  exemption: this robot does not keep eBay user data.
                </Step>
                <Step n={3}>
                  Copy the <b>App ID (Client ID)</b> and <b>Cert ID (Client Secret)</b>{" "}
                  into the fields above.
                </Step>
                <Step n={4}>
                  Next to the keyset, open <b>User Tokens</b>. At the bottom, under{" "}
                  <b>Get a Token from eBay via Your Application</b>, click{" "}
                  <b>+ Add eBay Redirect URL</b> (or <b>Click here to add one</b>). Open the new
                  row with its arrow and fill in:
                  <div style={{ display: "grid", gap: 6, margin: "8px 0" }}>
                    <GuideField label="Display Title" note="Any name; your sellers see it on eBay's consent page." value="Intern" />
                    <GuideField label="Your privacy policy URL" note="Any https page. It can't be eBay's own." value={PLACEHOLDER_URL} />
                    <GuideField label="Your auth accepted URL" note="Any https page. eBay sends you there after you agree, with code=… in the address." value={PLACEHOLDER_URL} />
                    <GuideField label="Your auth declined URL" note="Any https page." value={PLACEHOLDER_URL} />
                  </div>
                  At the bottom of the row, pick <b>OAuth (new security)</b>, not Auth&apos;n&apos;Auth,
                  then <b>Save</b>. Leave <i>Select OAuth Scopes</i> and the branding options as they are.
                  <div style={{ marginTop: 6 }}>
                    The <b>RuName</b> is the first column of that row (looks like{" "}
                    <code>Your_Name-YourApp-TestIn-abcde</code>). If it won&apos;t copy, type it into
                    the RuName field above exactly, with every <code>_</code> and <code>-</code>.
                  </div>
                  <div style={{ marginTop: 6, color: C.amber }}>
                    Don&apos;t use <b>Get OAuth Application Token</b>: that token belongs to the
                    app, can&apos;t see your shop and expires in two hours.
                  </div>
                </Step>
                <Step n={5}>
                  {environment === "sandbox" ? (
                    <>
                      Sandbox only: on the <b>User Tokens</b> page, click{" "}
                      <b>Register a new Sandbox user</b> and sign in with that user. Neither your
                      real eBay login nor your developer account works in Sandbox: if eBay shows
                      your developer name, click <b>Switch account</b>.
                    </>
                  ) : (
                    <>Sign in with the eBay account that owns your shop.</>
                  )}
                </Step>
                <Step n={6} last>
                  Click <b>Sign in with eBay</b>, agree, then copy the whole address of the
                  page you land on and paste it above. Click <b>Connect</b> within five
                  minutes: the code in it expires quickly. The robot then stays signed in
                  for about 18 months.
                </Step>
              </div>
            )}
          </div>

          {error && (
            <div style={{
              background: "var(--lm-red-dim)", border: "1px solid var(--lm-red-glow)",
              borderRadius: 8, padding: "8px 12px", fontSize: 12.5, color: C.red, marginBottom: 14,
            }}>
              {error}
            </div>
          )}

          <div style={{ display: "flex", flexWrap: "wrap", justifyContent: "space-between", alignItems: "center", gap: 10 }}>
            {connected ? (
              <button
                type="button"
                onClick={onDisconnect}
                disabled={disconnecting}
                style={{
                  padding: "8px 14px", borderRadius: 8, fontSize: 12, fontWeight: 500,
                  background: "none",
                  border: `1px solid ${C.border}`,
                  color: C.red,
                  cursor: disconnecting ? "not-allowed" : "pointer",
                  opacity: disconnecting ? 0.5 : 1,
                }}
              >
                {disconnecting ? "Disconnecting…" : "Disconnect"}
              </button>
            ) : <span />}

            <div style={{ display: "flex", flexWrap: "wrap", gap: 10 }}>
              <button
                type="button"
                onClick={reset}
                disabled={submitting || (!clientSecret && !refreshToken && !callback)}
                style={{
                  padding: "8px 18px", borderRadius: 8, fontSize: 12, fontWeight: 500,
                  background: "none",
                  border: `1px solid ${C.border}`,
                  color: C.text,
                  cursor: submitting || (!clientSecret && !refreshToken && !callback) ? "not-allowed" : "pointer",
                  opacity: submitting || (!clientSecret && !refreshToken && !callback) ? 0.5 : 1,
                }}
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={onSubmit}
                disabled={submitting || !ready}
                style={{
                  padding: "8px 22px", borderRadius: 8, fontSize: 12, fontWeight: 600,
                  background: submitting || !ready ? C.surface : C.amber,
                  color: submitting || !ready ? C.textMuted : "var(--lm-on-amber)",
                  border: "none",
                  cursor: submitting || !ready ? "not-allowed" : "pointer",
                  transition: "all 0.15s",
                }}
              >
                {submitting ? "Connecting…" : connected ? "Reconnect" : "Connect"}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

// Any https page works for eBay's redirect settings; the robot only needs the
// code eBay appends to the accepted URL, so suggest one owners can paste as is.
const PLACEHOLDER_URL = "https://autonomous.ai";

// One field of eBay's redirect-URL form: what to type, why, and a copy button.
function GuideField({ label, note, value }: { label: string; note: string; value: string }) {
  const [copied, setCopied] = useState(false);
  const onCopy = async () => {
    if (await copyText(value)) {
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    } else {
      toast.error("Couldn't copy. Select the text and copy it.");
    }
  };
  return (
    <div style={{ background: C.surface, border: `1px solid ${C.border}`, borderRadius: 8, padding: "6px 10px" }}>
      <div style={{ display: "flex", flexWrap: "wrap", alignItems: "center", justifyContent: "space-between", gap: 8 }}>
        <span style={{ color: C.text, fontWeight: 600 }}>{label}</span>
        <span style={{ display: "inline-flex", alignItems: "center", gap: 6, minWidth: 0 }}>
          <code style={{ color: C.text, userSelect: "all", overflowWrap: "anywhere" }}>{value}</code>
          <button
            type="button"
            onClick={onCopy}
            aria-label={`Copy ${label}`}
            style={{
              background: "none", border: `1px solid ${C.border}`, borderRadius: 6, padding: "2px 6px",
              color: copied ? C.green : C.textDim, cursor: "pointer", display: "inline-flex", alignItems: "center", gap: 4, fontSize: 11,
            }}
          >
            {copied ? <Check size={11} /> : <Copy size={11} />} {copied ? "Copied" : "Copy"}
          </button>
        </span>
      </div>
      <div style={{ fontSize: 11.5, color: C.textDim, marginTop: 2 }}>{note}</div>
    </div>
  );
}

// Check mark + character count once the token looks long enough (>= 40 chars).
function TokenCounter({ length }: { length: number }) {
  if (length === 0) return null;
  const ok = length >= 40;
  return (
    <span style={{ display: "inline-flex", alignItems: "center", gap: 4, color: ok ? C.green : C.textMuted }}>
      {ok && <span>✓</span>}
      <span>{length}</span>
    </span>
  );
}

const inputStyle: React.CSSProperties = {
  width: "100%",
  boxSizing: "border-box",
  background: C.surface,
  border: `1px solid ${C.border}`,
  borderRadius: 8,
  padding: "9px 12px",
  fontSize: 12.5,
  color: C.text,
  outline: "none",
};
