import { useEffect, useState } from "react";
import { toast } from "sonner";
import { ShoppingBag, Eye, EyeOff, Lock, ExternalLink, ChevronDown, ChevronRight } from "lucide-react";
import { C } from "@/components/setup/shared";
import { getShopify, setShopify, removeConnector } from "@/lib/api";
import { InfoCard, Step, FieldLabel } from "@/pages/settings/connectorFormParts";

const CONNECTOR_CODE = "shopify";

// Same cleanup the autonomous.ai form does: trim, lowercase, drop a pasted
// scheme and path, complete a bare handle. The device validates again.
function normalizeShop(raw: string): string {
  let s = raw.trim().toLowerCase().replace(/^https?:\/\//, "");
  s = s.replace(/[/?#].*$/, "").trim();
  if (/^[a-z0-9][a-z0-9-]*$/.test(s)) s += ".myshopify.com";
  return s;
}

const SHOP_RE = /^[a-z0-9][a-z0-9-]*\.myshopify\.com$/;

/** Screenshot with the store domain and Client ID blurred (the secret is masked by Shopify). */
function GuideShot({ file, alt, caption }: { file: string; alt: string; caption: string }) {
  return (
    <figure style={{ margin: "8px 0 0" }}>
      <img src={`/shopify-guide/${file}`} alt={alt}
        style={{ maxWidth: "100%", borderRadius: 8, border: `1px solid ${C.border}` }} />
      <figcaption style={{ fontSize: 12, color: C.textDim, marginTop: 4 }}>{caption}</figcaption>
    </figure>
  )
}

export function ShopifySection({ active }: { active: boolean }) {
  const [loading, setLoading] = useState(true);
  const [connected, setConnected] = useState(false);
  const [connectedShop, setConnectedShop] = useState("");
  const [connectedAt, setConnectedAt] = useState(0);
  // Non-secret values of the saved link, shown again after a reload.
  const [savedClientId, setSavedClientId] = useState("");
  const [hasSecret, setHasSecret] = useState(false);
  const [shop, setShop] = useState("");
  const [clientId, setClientId] = useState("");
  const [clientSecret, setClientSecret] = useState("");
  const [showSecret, setShowSecret] = useState(false);
  const [guideOpen, setGuideOpen] = useState<boolean>(() => {
    try { return localStorage.getItem("shopify-guide-open") === "1"; } catch { return false; }
  });
  useEffect(() => {
    try { localStorage.setItem("shopify-guide-open", guideOpen ? "1" : "0"); } catch { /* private mode: silently skip */ }
  }, [guideOpen]);
  const [submitting, setSubmitting] = useState(false);
  const [disconnecting, setDisconnecting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = () => {
    setLoading(true);
    getShopify()
      .then((r) => {
        setConnected(!!r.connected);
        setConnectedShop(r.shop ?? "");
        setConnectedAt(r.obtained_at ?? 0);
        setSavedClientId(r.client_id ?? "");
        setHasSecret(!!r.connected && !!r.has_secret);
        setShop(r.shop ?? "");
        setClientId(r.client_id ?? "");
      })
      .catch(() => {
        // A never-linked connector reports connected:false; stay silent on errors.
      })
      .finally(() => setLoading(false));
  };
  useEffect(load, []);

  const reset = () => {
    setShop(connectedShop);
    setClientId(savedClientId);
    setClientSecret("");
    setError(null);
    setShowSecret(false);
  };

  const canSubmit = !!shop.trim() && !!clientId.trim() && !!clientSecret.trim();
  // Anything typed that differs from the saved link (Cancel restores it).
  const dirty = shop !== connectedShop || clientId !== savedClientId || !!clientSecret;

  // Click handler, not a form submit: this renders inside SettingsPanel's form, and nested forms never submit.
  const onSubmit = async (e: React.MouseEvent | React.KeyboardEvent) => {
    e.preventDefault();
    setError(null);
    const domain = normalizeShop(shop);
    if (!SHOP_RE.test(domain)) {
      setError("Enter your store's .myshopify.com address, for example your-store.myshopify.com.");
      return;
    }
    const id = clientId.trim();
    const secret = clientSecret.trim();
    if (!id) { setError("Client ID is required."); return; }
    if (!secret) { setError("Client secret is required."); return; }
    if (/\s/.test(id) || /\s/.test(secret)) {
      setError("The Client ID and secret can't contain spaces: copy them again from the app's Overview page.");
      return;
    }
    setShop(domain);
    setSubmitting(true);
    try {
      await setShopify({ shop: domain, client_id: id, client_secret: secret });
      toast.success("Shopify connected.");
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
    if (!window.confirm("Disconnect Shopify? The robot will forget the saved store and credentials until you connect again.")) return;
    setDisconnecting(true);
    try {
      await removeConnector(CONNECTOR_CODE);
      toast.success("Shopify disconnected.");
      setConnected(false);
      setConnectedShop("");
      setConnectedAt(0);
      setSavedClientId("");
      setHasSecret(false);
      setShop("");
      setClientId("");
      setClientSecret("");
      setError(null);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Disconnect failed.");
    } finally {
      setDisconnecting(false);
    }
  };

  return (
    <div
      id="section-shopify"
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
              if (!submitting && canSubmit) onSubmit(e);
            }
          }}
        >
          <ModalHeader connected={connected} connectedAt={connectedAt} connectedShop={connectedShop} />

          <InfoCard tone="blue" title="BEFORE YOU START">
            You must be the <b>owner or an admin</b> of the Shopify store, and
            sign in to the <b>Shopify Dev Dashboard</b> with the <b>same Shopify
            account (organization)</b> that owns that store. Shopify only lets
            an app be installed on stores in its own organization. It is free
            and takes about 5 minutes.
          </InfoCard>

          <FieldLabel htmlFor="shopify_shop">Store domain</FieldLabel>
          <input
            id="shopify_shop"
            type="text"
            value={shop}
            onChange={(e) => setShop(e.target.value)}
            placeholder="your-store.myshopify.com"
            autoComplete="off"
            spellCheck={false}
            style={{ ...inputStyle, marginBottom: 6, fontFamily: "monospace" }}
          />
          <div style={{ fontSize: 12, color: C.textDim, marginBottom: 14, lineHeight: 1.5 }}>
            The permanent address ending in <b>.myshopify.com</b> (Shopify admin → Settings → Domains).{" "}
            <a
              href="https://admin.shopify.com/"
              target="_blank"
              rel="noreferrer"
              style={{
                color: C.amber, fontWeight: 500, textDecoration: "underline",
                textUnderlineOffset: 2,
              }}
            >
              Open your Shopify admin
            </a>{" "}
            (the address bar shows your store handle) or{" "}
            <button
              type="button"
              onClick={() => {
                setGuideOpen(true);
                document
                  .getElementById("shopify-guide-anchor")
                  ?.scrollIntoView({ behavior: "smooth", block: "nearest" });
              }}
              style={{
                background: "none", border: "none", padding: 0,
                color: C.amber, cursor: "pointer",
                font: "inherit", fontWeight: 500, textDecoration: "underline",
                textUnderlineOffset: 2,
              }}
            >
              see how to find it
            </button>
            .
          </div>

          <FieldLabel htmlFor="shopify_client_id">Client ID</FieldLabel>
          <input
            id="shopify_client_id"
            type="text"
            value={clientId}
            onChange={(e) => setClientId(e.target.value)}
            placeholder="paste the Client ID from your app's Overview"
            autoComplete="off"
            spellCheck={false}
            style={{ ...inputStyle, marginBottom: 14, fontFamily: "monospace" }}
          />

          <div style={{ display: "flex", alignItems: "flex-end", justifyContent: "space-between", marginBottom: 5 }}>
            <FieldLabel htmlFor="shopify_client_secret">Client secret</FieldLabel>
            <button
              type="button"
              onClick={() => setShowSecret((v) => !v)}
              aria-label={showSecret ? "Hide" : "Show"}
              style={{ background: "none", border: "none", padding: 0, color: C.textDim, cursor: "pointer", display: "flex", alignItems: "center" }}
            >
              {showSecret ? <EyeOff size={14} /> : <Eye size={14} />}
            </button>
          </div>
          <input
            id="shopify_client_secret"
            type={showSecret ? "text" : "password"}
            value={clientSecret}
            onChange={(e) => setClientSecret(e.target.value)}
            placeholder={hasSecret ? "saved; paste it again to reconnect or change it" : "paste the Client secret"}
            autoComplete="off"
            spellCheck={false}
            style={{ ...inputStyle, marginBottom: 12, fontFamily: "monospace" }}
          />

          <div style={{
            display: "flex", alignItems: "center", gap: 6,
            fontSize: 12, color: C.amber, marginBottom: 16,
          }}>
            <Lock size={11} /> Saved on this device. Only the connected status is shared with your account.
          </div>

          <div id="shopify-guide-anchor" style={{
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
                HOW TO CREATE YOUR SHOPIFY APP
              </button>
              <a
                href="https://dev.shopify.com"
                target="_blank"
                rel="noreferrer"
                style={{ color: C.amber, fontSize: 12.5, display: "inline-flex", alignItems: "center", gap: 4, textDecoration: "none" }}
              >
                Open Shopify Dev Dashboard <ExternalLink size={11} />
              </a>
            </div>
            {guideOpen && (
              <div style={{ marginTop: 12, fontSize: 12.5, color: C.textDim, lineHeight: 1.6 }}>
                <Step n={1}>
                  <b>Open the Shopify Dev Dashboard.</b> Go to{" "}
                  <a href="https://dev.shopify.com" target="_blank" rel="noreferrer" style={{ color: C.amber }}>dev.shopify.com</a>{" "}
                  and sign in with the same account you use for your store admin.
                  If asked to choose an organization, pick the one that owns your store.
                </Step>
                <Step n={2}>
                  <b>Create the app.</b> Click <b>Create app</b>, start from the
                  Dev Dashboard, name it "Autonomous" and click <b>Create</b>.
                </Step>
                <Step n={3}>
                  <b>Set up a version.</b> Open the app's <b>Versions</b> page and
                  create a version with:
                  <ul style={{ margin: "4px 0 4px 16px", padding: 0 }}>
                    <li><b>App URL</b> = <code>https://autonomous.ai</code> (any https address works)</li>
                    <li><b>Embed app in Shopify admin</b> = off</li>
                    <li>
                      <b>Scopes</b> = <code>read_products, write_products, read_content, write_content, read_publications, write_publications, read_inventory, write_inventory, read_locations, read_files, write_files</code>{" "}
                      (the publications scopes let the agent publish products to your Online Store; the inventory scopes let it set stock; the files scopes let it upload images)
                    </li>
                    <li><b>Redirect URLs</b> can stay empty</li>
                  </ul>
                  Click <b>Release</b> and confirm.
                </Step>
                <Step n={4}>
                  <b>Install the app on your store.</b> On the app's <b>Home</b> page
                  open <b>Distribution → Select distribution method → Custom distribution</b>,
                  enter your store domain (for example <code>your-store.myshopify.com</code>),
                  generate the install link, open it and click <b>Install</b>. You can
                  confirm in Shopify admin → Settings → Apps and sales channels.
                </Step>
                <Step n={5}>
                  <b>Copy the credentials.</b> Open the app's <b>Overview</b> page, find the{" "}
                  <b>Credentials</b> card on the right and copy the <b>Client ID</b> and the{" "}
                  <b>Client secret</b> with the copy buttons. Treat the secret like a password.
                  <GuideShot file="shopify-credentials.png"
                    alt="Dev Dashboard Credentials card with Client ID and Client secret and their copy buttons"
                    caption="Dev Dashboard → your app → Overview → Credentials (Client ID blurred here)." />
                </Step>
                <Step n={6} last>
                  <b>Enter them here.</b> Store domain (the permanent{" "}
                  <code>.myshopify.com</code> address: in Shopify admin the URL looks
                  like <code>admin.shopify.com/store/&lt;handle&gt;</code>, so the domain is{" "}
                  <code>&lt;handle&gt;.myshopify.com</code>; or Settings → Domains),
                  Client ID and Client secret, then click <b>Connect</b>.
                  <GuideShot file="shopify-store-domain.png"
                    alt="Dev Dashboard Stores list showing the store name and its .myshopify.com address"
                    caption="Dev Dashboard → Stores: the address under the store name ends in .myshopify.com (blurred here)." />
                </Step>
                <div style={{
                  marginTop: 12, paddingTop: 10, borderTop: `1px solid ${C.border}`,
                  fontSize: 12, color: C.textDim, lineHeight: 1.6,
                }}>
                  <div style={{ fontWeight: 700, letterSpacing: "0.05em", marginBottom: 4 }}>TROUBLESHOOTING</div>
                  <ul style={{ margin: "0 0 0 16px", padding: 0 }}>
                    <li><b>"App not installed"</b> → finish step 4 on the same store.</li>
                    <li><b>"Missing or invalid client secret"</b> → copy the secret again (no spaces); if you regenerated it in Shopify, use the new one.</li>
                    <li>Rejected even though both look right → the app and the store must be in the same Shopify organization; create the app from the account that owns the store.</li>
                    <li>Domain rejected → it must end in <code>.myshopify.com</code> (not a custom domain, not an admin.shopify.com address).</li>
                    <li>Can't publish products or upload images → add the <code>write_publications</code> and <code>write_files</code> scopes, release a new version, approve the update.</li>
                    <li>Tokens last 24 hours and your device renews them automatically; if you regenerate the secret in Shopify, connect again here with the new one.</li>
                  </ul>
                </div>
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
                disabled={submitting || !dirty}
                style={{
                  padding: "8px 18px", borderRadius: 8, fontSize: 12, fontWeight: 500,
                  background: "none",
                  border: `1px solid ${C.border}`,
                  color: C.text,
                  cursor: submitting || !dirty ? "not-allowed" : "pointer",
                  opacity: submitting || !dirty ? 0.5 : 1,
                }}
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={onSubmit}
                disabled={submitting || !canSubmit}
                style={{
                  padding: "8px 22px", borderRadius: 8, fontSize: 12, fontWeight: 600,
                  background: submitting || !canSubmit ? C.surface : C.amber,
                  color: submitting || !canSubmit ? C.textMuted : "var(--lm-on-amber)",
                  border: "none",
                  cursor: submitting || !canSubmit ? "not-allowed" : "pointer",
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

// Logo + title + description row.
function ModalHeader({
  connected,
  connectedAt,
  connectedShop,
}: {
  connected: boolean;
  connectedAt: number;
  connectedShop: string;
}) {
  return (
    <div style={{ display: "flex", alignItems: "flex-start", gap: 14, marginBottom: 18 }}>
      <div style={{
        flexShrink: 0,
        width: 44, height: 44, borderRadius: 12,
        background: "#5E8E3E",
        display: "flex", alignItems: "center", justifyContent: "center",
      }}>
        <ShoppingBag size={24} color="#fff" strokeWidth={2.2} />
      </div>
      <div style={{ minWidth: 0, flex: 1 }}>
        <div style={{ fontSize: 16, fontWeight: 700, color: C.text, marginBottom: 3 }}>
          Connect Shopify
        </div>
        <div style={{ fontSize: 12, color: C.textDim, lineHeight: 1.5 }}>
          Link your Shopify store with your own app so the robot can create draft products and articles.
        </div>
        {connected && (
          <div style={{ marginTop: 8, fontSize: 12, color: C.green, display: "inline-flex", flexWrap: "wrap", alignItems: "center", gap: 6 }}>
            ✓ connected
            {connectedShop && <span style={{ color: C.textDim }}>· {connectedShop}</span>}
            {connectedAt > 0 && (
              <span style={{ color: C.textDim }}>
                · {new Date(connectedAt * 1000).toLocaleDateString()}
              </span>
            )}
          </div>
        )}
      </div>
    </div>
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
