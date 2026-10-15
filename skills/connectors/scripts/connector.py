#!/usr/bin/env python3
"""Call a linked service with its stored credential, without the agent ever
handling the secret.

The agent names a connector, a method and a URL; this helper reads the token
from the device's connector config, attaches it, sends the request, and prints
the response body. The token never appears on a command line, in the process
list, or in anything the agent writes or reads.

Usage:
  connector.py list
  connector.py info <code>
  connector.py call <code> <METHOD> <url> [options]

Options for `call`:
  --query K=V        URL query parameter, URL-encoded for you (repeatable)
  --json BODY        JSON request body; `-` reads it from stdin
  --data K=V         form field, sent application/x-www-form-urlencoded (repeatable)
  --form K=V         multipart field; `K=@/path` uploads a file (repeatable)
  --header K:V       extra request header (repeatable; the credential header is refused)
  --token-param NAME send the credential as query parameter NAME instead of a
                     header (repeatable; only for endpoints that reject the header)

Exit codes: 0 success (2xx) · 1 HTTP error or network failure · 2 usage error ·
3 not connected / unusable credential · 4 host not allowed for this connector.
Connectors linked with a client ID and secret (Shopify) get a fresh short-lived
token minted for them when the stored one is missing or about to expire; that
token is cached in a helper-owned file and never printed.
On any failure nothing is written to stdout, so a following `| jq` sees empty
input instead of an error body it could mistake for an empty result.
"""
import base64
import hashlib
import json
import mimetypes
import os
import re
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid
from pathlib import Path

CONFIGS_DIR = Path(os.environ.get("CONNECTOR_CONFIGS_DIR", "/root/.openclaw/workspace/configs"))
TIMEOUT_SECONDS = 60
MAX_RESPONSE_BYTES = 8 * 1024 * 1024

# Official API hosts per connector. A credential is only ever sent to one of
# these (exact host or a subdomain of it). A connector that is not listed is
# refused: the helper never sends a credential to a host it cannot vouch for.
OFFICIAL_HOSTS = {
    "gmail": ("googleapis.com",),
    "google_calendar": ("googleapis.com",),
    "google_drive": ("googleapis.com",),
    "facebook": ("graph.facebook.com",),
    "figma": ("api.figma.com",),
    "figma-api": ("api.figma.com",),
    "github": ("api.github.com",),
    "ahrefs": ("api.ahrefs.com",),
    "notion": ("api.notion.com",),
    "linear": ("api.linear.app",),
    "asana": ("app.asana.com",),
    # Narrowed per call to the linked environment's hosts (EBAY_HOSTS).
    "ebay": ("api.ebay.com", "apiz.ebay.com", "api.sandbox.ebay.com", "apiz.sandbox.ebay.com"),
}
GOOGLE_CODES = ("gmail", "google_calendar", "google_drive")

# Connectors whose API host is per account, not per provider. The credential may
# only go to the one host stored when the account was linked, never to a family
# of hosts: code -> (credential key holding the host, pattern a valid host must
# match in full).
DYNAMIC_HOST_CODES = {
    "shopify": ("shop", re.compile(r"[a-z0-9][a-z0-9-]*\.myshopify\.com")),
    # Amazon's Selling Partner API has one host per region (and a sandbox).
    "amazon_sp": ("api_host", re.compile(r"(sandbox\.)?sellingpartnerapi-(na|eu|fe)\.amazon\.com")),
}

# Connectors linked with the customer's own app (client ID + client secret) and
# no refresh token: the access token is short-lived (about a day) and the helper
# mints a new one itself from the saved client credentials when needed. The token
# request goes to the one stored account host, like every other call.
CLIENT_CREDENTIAL_CODES = ("shopify", "ebay", "amazon_sp")

# Connectors whose token is minted from a saved refresh token as well as the client ID and secret
# (Login with Amazon): the token request goes to a fixed provider host, not the account host.
REFRESH_TOKEN_CODES = ("amazon_sp",)
LWA_TOKEN_URL = "https://api.amazon.com/auth/o2/token"
TOKEN_RENEW_SKEW_SECONDS = 300          # renew when less than this is left
MINT_TIMEOUT_SECONDS = 30
MAX_MINT_RESPONSE_BYTES = 64 * 1024
DEFAULT_MINTED_LIFETIME_SECONDS = 3600  # when the response omits expires_in
MAX_MINTED_LIFETIME_SECONDS = 7 * 24 * 3600
MINT_REJECTED_MESSAGE = (
    "{code}: the provider rejected the saved client ID/secret or the app is no longer installed — "
    "ask the user to reconnect {code} in the Autonomous app"
)

# eBay: the customer's own developer app (client ID + secret) plus a refresh
# token for their seller account. The helper mints the short-lived access token
# from the refresh token on the linked environment's API host; the stored
# `api_key` is that refresh token and is never sent to an API endpoint.
EBAY_HOSTS = {
    "production": ("api.ebay.com", "apiz.ebay.com"),
    "sandbox": ("api.sandbox.ebay.com", "apiz.sandbox.ebay.com"),
}
# Content-Language eBay expects on inventory/offer writes, per marketplace.
EBAY_CONTENT_LANGUAGE = {
    "EBAY_US": "en-US", "EBAY_MOTORS_US": "en-US", "EBAY_CA": "en-CA", "EBAY_GB": "en-GB",
    "EBAY_AU": "en-AU", "EBAY_IE": "en-IE", "EBAY_DE": "de-DE", "EBAY_AT": "de-AT",
    "EBAY_CH": "de-CH", "EBAY_FR": "fr-FR", "EBAY_IT": "it-IT", "EBAY_ES": "es-ES",
    "EBAY_NL": "nl-NL", "EBAY_BE": "nl-BE", "EBAY_PL": "pl-PL",
}
EBAY_REFRESH_REJECTED_MESSAGE = (
    "ebay: eBay rejected the saved refresh token (expired or revoked) — "
    "ask the user to reconnect eBay in the Autonomous app"
)

# Connectors that present the token in their own header instead of
# `Authorization: Bearer`. The raw token is sent, without a scheme prefix.
AUTH_HEADERS = {
    "shopify": "X-Shopify-Access-Token",
    "amazon_sp": "x-amz-access-token",
}

# Connectors whose API only accepts the credential as a query parameter on
# some endpoints (Facebook feed-post DELETE, debug_token).
TOKEN_PARAM_CODES = ("facebook",)

# Files that may be uploaded with `--form K=@path`: media and documents only,
# never from a directory that holds credentials or device configuration.
UPLOAD_SUFFIXES = (
    ".jpg", ".jpeg", ".png", ".gif", ".webp", ".heic",
    ".mp4", ".mov", ".webm", ".mp3", ".m4a", ".wav", ".ogg",
    ".pdf", ".txt", ".md", ".csv",
)
UPLOAD_FORBIDDEN_DIRS = ("/root/.openclaw", "/root/config", "/root/.ssh", "/etc", "/proc", "/sys")
UPLOAD_ALLOWED_DIRS = ("/root/.openclaw/workspace/media", "/root/.openclaw/media")


class Failure(Exception):
    def __init__(self, code, message):
        super().__init__(message)
        self.code = code


def fmt_time(ts):
    """Local time plus the zone, so nobody reads a UTC stamp as local."""
    if not ts:
        return "never"
    return time.strftime("%Y-%m-%d %H:%M:%S %Z", time.localtime(int(ts)))


def load_json(path):
    try:
        with open(path) as f:
            return json.load(f)
    except FileNotFoundError:
        return None
    except (OSError, ValueError) as e:
        raise Failure(3, f"cannot read {path.name}: {type(e).__name__}")


def load_entry(code):
    """Return the stored entry for a connector, or None when it is not linked."""
    data = load_json(CONFIGS_DIR / f"{code}_access_tokens.json")
    entry = ((data or {}).get("connectors") or {}).get(code)
    if entry:
        return entry
    data = load_json(CONFIGS_DIR / "connectors.json")
    return ((data or {}).get("connectors") or {}).get(code)


def stored_host(code, entry):
    """The single API host a per-account connector may be called on, taken from
    the entry stored at link time. Raises when it is missing or malformed so the
    credential is never sent to a host the helper cannot vouch for."""
    key, pattern = DYNAMIC_HOST_CODES[code]
    creds = entry.get("credentials") or {}
    host = creds.get(key)
    if not host:
        try:
            host = (json.loads(creds.get("connector_vars") or "{}") or {}).get(key)
        except (ValueError, AttributeError):
            host = None
    host = str(host or "").strip().lower()
    if not pattern.fullmatch(host):
        raise Failure(3, f"{code}: the linked account has no valid {key} stored — ask the user to reconnect it in the Autonomous app")
    return host


def account_of(entry):
    creds = entry.get("credentials") or {}
    if creds.get("shop"):
        return creds["shop"]
    if entry.get("auth_type") == "pat" and creds.get("email"):
        return creds["email"]
    return entry.get("user_email") or creds.get("email") or creds.get("shop") or ""


def cmd_list():
    """Print every linked connector from all three credential shapes."""
    found = 0
    problems = []
    for path in sorted(CONFIGS_DIR.glob("*_access_tokens.json")):
        code = path.name[: -len("_access_tokens.json")]
        try:
            entry = ((load_json(path) or {}).get("connectors") or {}).get(code)
        except Failure as e:
            problems.append(str(e))
            continue
        if entry:
            found += 1
            print(describe(code, entry))
    for name, key, label in (("connectors.json", "connectors", ""), ("access_tokens.json", "providers", "provider ")):
        try:
            data = load_json(CONFIGS_DIR / name) or {}
        except Failure as e:
            problems.append(str(e))
            continue
        for code, entry in (data.get(key) or {}).items():
            found += 1
            if label:
                email = entry.get("user_email")
                print(f"provider {code}: oauth token present" + (f" ({email})" if email else ""))
            else:
                print(describe(code, entry))
    for p in problems:
        print(f"verification failed: {p}", file=sys.stderr)
    if problems:
        return 3
    if not found:
        print("no connectors linked")
    return 0


def describe(code, entry):
    parts = [account_of(entry), entry.get("auth_type") or "oauth"]
    return f"{code}: connected (" + ", ".join(p for p in parts if p) + ")"


def cmd_info(code):
    entry = load_entry(code)
    if not entry:
        drop_token_cache(code)
        raise Failure(3, f"{code}: not connected")
    expires = int(entry.get("expires_at") or 0)
    info = {
        "connector": code,
        "auth_type": entry.get("auth_type") or "oauth",
        "account": account_of(entry),
        "scopes": entry.get("scopes") or [],
        "auto_refresh": bool(entry.get("refresh_token")) and bool(entry.get("refresh")),
        "expires": "never" if not expires else fmt_time(expires),
        "expired": bool(expires) and expires < time.time(),
        "obtained": fmt_time(entry.get("obtained_at")),
    }
    page_id = (entry.get("credentials") or {}).get("page_id")
    if page_id:
        info["page_id"] = page_id
    if code in DYNAMIC_HOST_CODES:
        try:
            info[DYNAMIC_HOST_CODES[code][0]] = stored_host(code, entry)
        except Failure as e:
            info["problem"] = str(e)
    if code in CLIENT_CREDENTIAL_CODES:
        saved = can_mint_token(code, entry)
        info["client_credentials"] = "saved" if saved else "missing"
        info["token_renewal"] = (
            "automatic: the helper mints a new token when needed" if saved
            else "none: reconnect when the token expires"
        )
        if saved:
            info["auto_refresh"] = True
            info["expired"] = False  # an expired stored token is replaced on the next call
    if code == "ebay":
        if not ebay_refresh_token(entry):
            info["client_credentials"] = "missing"
            info["token_renewal"] = "none: reconnect eBay"
            info["auto_refresh"] = False
        else:
            info["expires"] = "access tokens are minted when needed"
        info["environment"] = ebay_environment(entry)
        info["marketplace_id"] = ebay_marketplace(entry)
    print(json.dumps(info, indent=2))
    return 0


def client_credentials_of(entry):
    """The saved client ID and secret of a connector linked with its own app."""
    creds = entry.get("credentials") or {}
    client_id = str(creds.get("client_id") or entry.get("client_id") or "").strip()
    client_secret = str(creds.get("client_secret") or "").strip()
    return client_id, client_secret


def refresh_token_of(entry):
    """The saved refresh token of a connector that mints from one (never printed)."""
    return str((entry.get("credentials") or {}).get("refresh_token") or entry.get("refresh_token") or "").strip()


def can_mint_token(code, entry):
    """Whether the helper holds everything it needs to mint a new token for this connector."""
    client_id, client_secret = client_credentials_of(entry)
    if not (client_id and client_secret):
        return False
    if code == "ebay":
        return bool(ebay_refresh_token(entry))
    return bool(refresh_token_of(entry)) if code in REFRESH_TOKEN_CODES else True


def ebay_environment(entry):
    env = str((entry.get("credentials") or {}).get("environment") or "production").strip().lower()
    return env if env in EBAY_HOSTS else "production"


def ebay_marketplace(entry):
    return str((entry.get("credentials") or {}).get("marketplace_id") or "EBAY_US").strip().upper()


def ebay_refresh_token(entry):
    return str(entry.get("refresh_token") or entry.get("api_key") or "").strip()


def credential_fingerprint(host, client_id):
    """Identifies which account and app a cached token belongs to. It covers the
    host and client ID only; the secret is never hashed or stored by the helper."""
    return hashlib.sha256(f"{host}\n{client_id}".encode()).hexdigest()


def token_cache_path(code):
    return CONFIGS_DIR / f"{code}.token.json"


def read_token_cache(code, fingerprint, now):
    """A still-valid cached token for exactly this account and app, else None."""
    try:
        data = load_json(token_cache_path(code))
    except Failure:
        return None  # a damaged cache is just a miss
    if not isinstance(data, dict) or data.get("fingerprint") != fingerprint:
        return None
    token = data.get("access_token")
    try:
        expires_at = int(data.get("expires_at") or 0)
    except (TypeError, ValueError):
        return None
    if not isinstance(token, str) or not token or expires_at - now <= TOKEN_RENEW_SKEW_SECONDS:
        return None
    return token, expires_at


def write_token_cache(code, fingerprint, token, expires_at):
    """Atomically write the helper-owned token cache (mode 0600). The entry file
    the device software manages is never touched. A failure here only costs an
    extra token request next time, so it is swallowed."""
    path = token_cache_path(code)
    tmp = path.with_name(f".{path.name}.{uuid.uuid4().hex}.tmp")
    payload = json.dumps({
        "fingerprint": fingerprint, "access_token": token,
        "expires_at": int(expires_at), "obtained_at": int(time.time()),
    })
    try:
        fd = os.open(tmp, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(fd, "w") as f:
            f.write(payload)
        os.replace(tmp, path)
    except OSError:
        try:
            os.unlink(tmp)
        except OSError:
            pass


def drop_token_cache(code):
    """Forget a cached token once its connector is gone."""
    if code in CLIENT_CREDENTIAL_CODES:
        try:
            token_cache_path(code).unlink()
        except OSError:
            pass


def mint_token(code, host, client_id, client_secret, refresh_token=""):
    """Ask the provider for a new short-lived token with the saved credentials.
    Shopify: the account's own host with client credentials. Amazon: Login with
    Amazon with the saved refresh token. Returns (token, expires_at). Error
    messages are fixed text: they never carry the client ID, the secret, the
    refresh token or the response body."""
    if code in REFRESH_TOKEN_CODES:
        url = LWA_TOKEN_URL
        body = urllib.parse.urlencode({
            "grant_type": "refresh_token", "refresh_token": refresh_token,
            "client_id": client_id, "client_secret": client_secret,
        }).encode()
    else:
        url = f"https://{host}/admin/oauth/access_token"
        body = urllib.parse.urlencode({
            "grant_type": "client_credentials", "client_id": client_id, "client_secret": client_secret,
        }).encode()
    req = urllib.request.Request(
        url, data=body, method="POST",
        headers={"Content-Type": "application/x-www-form-urlencoded", "Accept": "application/json"},
    )
    try:
        with _OPENER.open(req, timeout=MINT_TIMEOUT_SECONDS) as resp:
            raw = resp.read(MAX_MINT_RESPONSE_BYTES)
    except urllib.error.HTTPError as e:
        if e.code in (400, 401, 403):
            raise Failure(3, MINT_REJECTED_MESSAGE.format(code=code))
        if 300 <= e.code < 400:
            raise Failure(1, f"{code}: the token request was redirected; not followed. Ask the user to reconnect {code}")
        raise Failure(1, f"{code}: the token request failed (HTTP {e.code}); try again in a moment")
    except (urllib.error.URLError, OSError):
        raise Failure(1, f"{code}: could not reach the provider to renew the token (network problem); try again in a moment")
    try:
        data = json.loads(raw.decode("utf-8", "replace"))
        token = data["access_token"]
        lifetime = int(data.get("expires_in") or DEFAULT_MINTED_LIFETIME_SECONDS)
    except (ValueError, KeyError, TypeError, AttributeError):
        raise Failure(1, f"{code}: the provider answered the token request with something unusable; try again in a moment")
    if not isinstance(token, str) or not token:
        raise Failure(1, f"{code}: the provider answered the token request with something unusable; try again in a moment")
    lifetime = max(60, min(lifetime, MAX_MINTED_LIFETIME_SECONDS))
    return token, int(time.time()) + lifetime


def mint_ebay_token(host, client_id, client_secret, refresh_token):
    """Exchange the saved eBay refresh token for a short-lived access token on
    the linked environment's host. Returns (token, expires_at). Error messages
    are fixed text: they never carry the client ID, a secret or the response body."""
    basic = base64.b64encode(f"{client_id}:{client_secret}".encode()).decode()
    req = urllib.request.Request(
        f"https://{host}/identity/v1/oauth2/token",
        data=urllib.parse.urlencode({"grant_type": "refresh_token", "refresh_token": refresh_token}).encode(),
        method="POST",
        headers={
            "Content-Type": "application/x-www-form-urlencoded", "Accept": "application/json",
            "Authorization": " ".join(("Basic", basic)),
        },
    )
    try:
        with _OPENER.open(req, timeout=MINT_TIMEOUT_SECONDS) as resp:
            raw = resp.read(MAX_MINT_RESPONSE_BYTES)
    except urllib.error.HTTPError as e:
        try:
            error = json.loads(e.read(MAX_MINT_RESPONSE_BYTES).decode("utf-8", "replace")).get("error")
        except (ValueError, AttributeError):
            error = None
        if error == "invalid_grant":
            raise Failure(3, EBAY_REFRESH_REJECTED_MESSAGE)
        if e.code in (400, 401, 403):
            raise Failure(3, MINT_REJECTED_MESSAGE.format(code="ebay"))
        if 300 <= e.code < 400:
            raise Failure(1, "ebay: the token request was redirected; not followed. Ask the user to reconnect ebay")
        raise Failure(1, f"ebay: the token request failed (HTTP {e.code}); try again in a moment")
    except (urllib.error.URLError, OSError):
        raise Failure(1, "ebay: could not reach eBay to renew the token (network problem); try again in a moment")
    try:
        data = json.loads(raw.decode("utf-8", "replace"))
        token = data["access_token"]
        lifetime = int(data.get("expires_in") or DEFAULT_MINTED_LIFETIME_SECONDS)
    except (ValueError, KeyError, TypeError, AttributeError):
        raise Failure(1, "ebay: eBay answered the token request with something unusable; try again in a moment")
    if not isinstance(token, str) or not token:
        raise Failure(1, "ebay: eBay answered the token request with something unusable; try again in a moment")
    lifetime = max(60, min(lifetime, MAX_MINTED_LIFETIME_SECONDS))
    return token, int(time.time()) + lifetime


def obtain_token(code, entry, host, force=False):
    """The token to use for a call, and whether it was minted just now.

    Connectors with saved client credentials use, in order: a valid cached
    token, a valid stored token, else a freshly minted one (always minted when
    `force` is set, after the provider rejected the one in use). Every other
    connector just uses its stored credential."""
    if code not in CLIENT_CREDENTIAL_CODES:
        token = entry.get("access_token") or entry.get("api_key")
        if not token:
            raise Failure(3, f"{code}: linked but no usable credential stored")
        return token, False

    now = int(time.time())
    client_id, client_secret = client_credentials_of(entry)
    ebay = code == "ebay"
    can_mint = can_mint_token(code, entry)
    fingerprint = None
    if client_id:
        # eBay: a relink (another seller on the same app) must not reuse a
        # cached token, so the link time is part of what the cache belongs to.
        fingerprint = credential_fingerprint(host, f"{client_id}\n{entry.get('obtained_at') or ''}" if ebay else client_id)
    if not force:
        candidates = []
        cached = read_token_cache(code, fingerprint, now) if fingerprint else None
        if cached:
            candidates.append(cached)
        # eBay's stored credential is the refresh token, never an access token.
        stored = None if ebay else entry.get("access_token") or entry.get("api_key")
        stored_expiry = int(entry.get("expires_at") or 0)
        if stored and (stored_expiry == 0 or stored_expiry - now > TOKEN_RENEW_SKEW_SECONDS):
            candidates.append((stored, stored_expiry or float("inf")))
        if candidates:
            return max(candidates, key=lambda c: c[1])[0], False
    if not can_mint:
        raise Failure(3, f"{code}: the saved token has expired and no client ID/secret is saved — ask the user to reconnect {code} in the Autonomous app")
    if ebay:
        token, expires_at = mint_ebay_token(host, client_id, client_secret, ebay_refresh_token(entry))
    else:
        token, expires_at = mint_token(code, host, client_id, client_secret, refresh_token_of(entry))
    write_token_cache(code, fingerprint, token, expires_at)
    return token, True


def host_allowed(code, host):
    allowed = OFFICIAL_HOSTS.get(code)
    if not allowed:
        return False
    return any(host == h or host.endswith("." + h) for h in allowed)


class _NoRedirect(urllib.request.HTTPRedirectHandler):
    """Never follow a redirect: urllib would re-send the Authorization header
    (and a credential in the query) to wherever the Location points."""

    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


_OPENER = urllib.request.build_opener(_NoRedirect)


def upload_path(raw):
    """Resolve an `@path` upload and refuse anything that is not plain media or
    a document outside the device's credential and configuration folders."""
    path = Path(raw).expanduser()
    try:
        real = path.resolve(strict=True)
    except (OSError, RuntimeError):
        raise Failure(2, f"cannot read upload {raw}")
    text = str(real)
    allowed_dir = any(text == d or text.startswith(d + "/") for d in UPLOAD_ALLOWED_DIRS)
    forbidden_dir = any(text == d or text.startswith(d + "/") for d in UPLOAD_FORBIDDEN_DIRS)
    if (forbidden_dir and not allowed_dir) or not real.is_file():
        raise Failure(2, f"refusing to upload {raw}: not a media file the helper may send")
    if real.suffix.lower() not in UPLOAD_SUFFIXES or path.suffix.lower() not in UPLOAD_SUFFIXES:
        raise Failure(2, f"refusing to upload {raw}: only {', '.join(UPLOAD_SUFFIXES)} files")
    return real


def pair(value, sep, flag):
    if sep not in value:
        raise Failure(2, f"{flag} expects K{sep}V, got {value!r}")
    k, v = value.split(sep, 1)
    return k.strip(), v if sep == "=" else v.strip()


def multipart(fields):
    boundary = uuid.uuid4().hex
    chunks = []
    for key, value in fields:
        head = f'--{boundary}\r\nContent-Disposition: form-data; name="{key}"'
        if value.startswith("@"):
            path = upload_path(value[1:])
            try:
                content = path.read_bytes()
            except OSError as e:
                raise Failure(2, f"cannot read upload {path}: {type(e).__name__}")
            ctype = mimetypes.guess_type(path.name)[0] or "application/octet-stream"
            head += f'; filename="{path.name}"\r\nContent-Type: {ctype}'
        else:
            content = value.encode()
        chunks += [head.encode() + b"\r\n\r\n", content, b"\r\n"]
    chunks.append(f"--{boundary}--\r\n".encode())
    return b"".join(chunks), f"multipart/form-data; boundary={boundary}"


def parse_call_args(args):
    opts = {"query": [], "json": None, "data": [], "form": [], "header": [], "token_param": []}
    i = 0
    while i < len(args):
        flag = args[i]
        if i + 1 >= len(args):
            raise Failure(2, f"{flag} needs a value")
        value = args[i + 1]
        if flag == "--query":
            opts["query"].append(pair(value, "=", flag))
        elif flag == "--data":
            opts["data"].append(pair(value, "=", flag))
        elif flag == "--form":
            opts["form"].append(pair(value, "=", flag))
        elif flag == "--header":
            opts["header"].append(pair(value, ":", flag))
        elif flag == "--json":
            opts["json"] = sys.stdin.read() if value == "-" else value
        elif flag == "--token-param":
            opts["token_param"].append(value)
        else:
            raise Failure(2, f"unknown option {flag}")
        i += 2
    if sum(bool(x) for x in (opts["json"] is not None, opts["data"], opts["form"])) > 1:
        raise Failure(2, "use only one of --json, --data, --form")
    return opts


def cmd_call(code, method, url, args):
    opts = parse_call_args(args)
    method = method.upper()

    parsed = urllib.parse.urlsplit(url)
    if parsed.scheme != "https" or not parsed.hostname:
        raise Failure(4, f"refusing {url!r}: only https:// URLs are allowed")
    if parsed.username is not None or parsed.password is not None or parsed.port not in (None, 443):
        raise Failure(4, f"refusing {url!r}: no user info or custom port in the URL")
    if code not in OFFICIAL_HOSTS and code not in DYNAMIC_HOST_CODES:
        raise Failure(4, f"{code}: the helper has no official API host for this connector — use its MCP tools if it has them")
    if code in OFFICIAL_HOSTS and not host_allowed(code, parsed.hostname):
        allowed = ", ".join(OFFICIAL_HOSTS[code])
        raise Failure(4, f"refusing to send the {code} credential to {parsed.hostname}; allowed: {allowed}")
    if opts["token_param"] and code not in TOKEN_PARAM_CODES:
        raise Failure(2, f"--token-param is only for {', '.join(TOKEN_PARAM_CODES)}")

    entry = load_entry(code)
    if not entry:
        drop_token_cache(code)
        raise Failure(3, f"{code}: not connected — ask the user to link it in the Autonomous app")
    host = None
    if code in DYNAMIC_HOST_CODES:
        host = stored_host(code, entry)
        if parsed.hostname != host:
            raise Failure(4, f"refusing to send the {code} credential to {parsed.hostname}; it only goes to the account it was linked to")
    if code == "ebay":
        env = ebay_environment(entry)
        if parsed.hostname not in EBAY_HOSTS[env]:
            raise Failure(4, f"refusing {parsed.hostname}: this eBay link is {env}; allowed: {', '.join(EBAY_HOSTS[env])}")
        host = EBAY_HOSTS[env][0]  # where tokens are minted
    if code in GOOGLE_CODES and entry.get("auth_type") == "pat":
        raise Failure(3, f"{code}: linked with an app password; Google REST APIs reject it — use IMAP/SMTP (see SKILL.md)")
    token, fresh = obtain_token(code, entry, host)
    client_id, client_secret = client_credentials_of(entry) if code in CLIENT_CREDENTIAL_CODES else ("", "")
    can_mint = code in CLIENT_CREDENTIAL_CODES and can_mint_token(code, entry)
    # Every secret that must never reach the output.
    secrets = [client_secret]
    if code == "ebay":
        secrets.append(ebay_refresh_token(entry))
    if code in REFRESH_TOKEN_CODES:
        secrets.append(refresh_token_of(entry))

    body = None
    content_type = None
    if opts["json"] is not None:
        body = opts["json"].encode()
        content_type = "application/json"
    elif opts["data"]:
        body = urllib.parse.urlencode(opts["data"]).encode()
        content_type = "application/x-www-form-urlencoded"
    forbidden = {"authorization", "host", "proxy-authorization", "cookie"} | {h.lower() for h in AUTH_HEADERS.values()}
    for k, v in opts["header"]:
        if any(c in k + v for c in "\r\n\0"):
            raise Failure(2, f"refusing header {k!r}: line breaks are not allowed in a header")
        if k.strip().lower() in forbidden:
            raise Failure(2, f"do not pass {k}; the helper sets it (or it is not allowed)")

    # One extra attempt is allowed, only for connectors whose token the helper
    # can mint, and only when a token it did not just mint is rejected (401).
    for attempt in range(2):
        query = urllib.parse.parse_qsl(parsed.query, keep_blank_values=True) + opts["query"]
        for name in opts["token_param"]:
            query.append((name, token))
        full_url = urllib.parse.urlunsplit(
            (parsed.scheme, parsed.netloc, parsed.path, urllib.parse.urlencode(query), "")
        )
        headers = {"Accept": "application/json"}
        for k, v in opts["header"]:
            headers[k] = v
        if not opts["token_param"]:
            if code in AUTH_HEADERS:
                headers[AUTH_HEADERS[code]] = token
            else:
                headers["Authorization"] = " ".join(("Bearer", token))
        if code == "ebay":
            present = {k.strip().lower() for k in headers}
            marketplace = ebay_marketplace(entry)
            if "x-ebay-c-marketplace-id" not in present:
                headers["X-EBAY-C-MARKETPLACE-ID"] = marketplace
            if (body is not None or opts["form"]) and "content-language" not in present:
                headers["Content-Language"] = EBAY_CONTENT_LANGUAGE.get(marketplace, "en-US")
        request_body = body
        if content_type:
            headers.setdefault("Content-Type", content_type)
        elif opts["form"]:
            request_body, headers["Content-Type"] = multipart(opts["form"])

        req = urllib.request.Request(full_url, data=request_body, method=method, headers=headers)
        try:
            with _OPENER.open(req, timeout=TIMEOUT_SECONDS) as resp:
                out = resp.read(MAX_RESPONSE_BYTES)
                status = resp.status
        except urllib.error.HTTPError as e:
            detail = e.read(MAX_RESPONSE_BYTES).decode("utf-8", "replace")
            if e.code == 401 and can_mint and not fresh and attempt == 0:
                token, fresh = obtain_token(code, entry, host, force=True)
                continue
            print(f"HTTP {e.code} from {parsed.hostname}", file=sys.stderr)
            if 300 <= e.code < 400:
                print("redirect not followed: the credential is only sent to the official host", file=sys.stderr)
            elif detail.strip():
                print(scrub(detail, token, *secrets), file=sys.stderr)
            if e.code == 401:
                expires = int(entry.get("expires_at") or 0)
                print(
                    f"credential: expires {fmt_time(expires) if expires else 'never'}, "
                    f"auto_refresh={'yes' if (entry.get('refresh_token') and entry.get('refresh')) or can_mint else 'no'}",
                    file=sys.stderr,
                )
            return 1
        except (urllib.error.URLError, OSError) as e:
            reason = getattr(e, "reason", e)
            print(f"request to {parsed.hostname} failed: {type(reason).__name__}", file=sys.stderr)
            return 1
        break

    text = out.decode("utf-8", "replace")
    if text:
        sys.stdout.write(scrub(text, token, *secrets))
        if not text.endswith("\n"):
            sys.stdout.write("\n")
    elif status == 204:
        print("{}")
    return 0


def scrub(text, *secrets):
    """Replace every secret (and its URL-encoded forms) with a placeholder."""
    for secret in secrets:
        if not secret:
            continue
        for form in {secret, urllib.parse.quote(secret, safe=""), urllib.parse.quote_plus(secret)}:
            text = text.replace(form, "[credential]")
    return text


def main(argv):
    try:
        if len(argv) >= 1 and argv[0] == "list":
            return cmd_list()
        if len(argv) == 2 and argv[0] == "info":
            return cmd_info(argv[1])
        if len(argv) >= 4 and argv[0] == "call":
            return cmd_call(argv[1], argv[2], argv[3], argv[4:])
        print(__doc__.split("\n\n", 2)[2], file=sys.stderr)
        return 2
    except Failure as e:
        print(str(e), file=sys.stderr)
        return e.code
    except BrokenPipeError:
        raise
    except Exception as e:  # never let a traceback echo request details
        print(f"connector helper failed: {type(e).__name__}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    try:
        code = main(sys.argv[1:])
        sys.stdout.flush()
    except BrokenPipeError:
        # The reader (e.g. a failing `| jq`) closed the pipe; its own error is
        # the one worth showing, not a Python traceback.
        os.dup2(os.open(os.devnull, os.O_WRONLY), sys.stdout.fileno())
        code = 1
    sys.exit(code)
