package mqtthandler

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"errors"
	"io"
	"log/slog"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/gin-gonic/gin"

	"go.autonomous.ai/os/system/server/serializers"
)

// The eBay connector is linked with the customer's own eBay developer app: its
// client ID and secret plus a refresh token for their seller account. eBay's
// developer portal does not show a refresh token, so this page takes the
// authorization code eBay hands back after the seller signs in and exchanges it
// here, on the device, where the secret already is. A refresh token pasted
// directly is accepted too and checked once. The connectors skill helper mints
// the short-lived access tokens from the refresh token on demand.
const (
	ebayConnectorCode = "ebay"
	// ebayTokenCacheFile is the helper-owned access-token cache; dropped on
	// every save so a new link never reuses an old token.
	ebayTokenCacheFile = "ebay.token.json"

	ebayTokenTimeout     = 20 * time.Second
	ebayMaxTokenResponse = 64 << 10
	maxEbayFieldLen      = 512
	maxEbayCodeLen       = 4096

	ebayRejectedMessage  = "eBay rejected these values. Check that the App ID, Cert ID and RuName are from the same keyset as the environment you picked, and sign in again: a code works once and only for about five minutes."
	ebayUnreachableError = "Couldn't reach eBay to finish connecting. Check the device's internet connection and try again."
)

// ebayTokenHosts is the only place the secret, a code or a refresh token is sent.
var ebayTokenHosts = map[string]string{
	"production": "api.ebay.com",
	"sandbox":    "api.sandbox.ebay.com",
}

var (
	errEbayRejected    = errors.New("ebay rejected the credentials")
	errEbayUnreachable = errors.New("ebay unreachable")
)

// ebayConnectRequest is the body of PUT /api/device/connectors/ebay. Exactly
// one of Code (the code, or the whole address eBay redirected to) and
// RefreshToken is used; Code wins when both are sent.
type ebayConnectRequest struct {
	Environment   string `json:"environment"`
	MarketplaceID string `json:"marketplace_id"`
	ClientID      string `json:"client_id"`
	ClientSecret  string `json:"client_secret"`
	RuName        string `json:"ru_name"`
	Code          string `json:"code"`
	RefreshToken  string `json:"refresh_token"`
}

// ebayToken is the useful part of an eBay token response.
type ebayToken struct {
	RefreshToken          string
	RefreshTokenExpiresIn int
}

// ebayCodeFrom accepts a bare code or the full address eBay redirected the
// browser to, and returns the decoded code.
func ebayCodeFrom(raw string) string {
	s := strings.TrimSpace(raw)
	// An address (with or without code=) is read only through its query.
	if strings.Contains(s, "code=") || strings.Contains(s, "://") || strings.Contains(s, "?") {
		query := s
		if u, err := url.Parse(s); err == nil && u.RawQuery != "" {
			query = u.RawQuery
		} else if i := strings.Index(s, "?"); i >= 0 {
			query = s[i+1:]
		}
		if v, err := url.ParseQuery(query); err == nil {
			return strings.TrimSpace(v.Get("code"))
		}
		return ""
	}
	// A bare code may arrive still URL-encoded (copied out of the address bar).
	if d, err := url.QueryUnescape(s); err == nil {
		return d
	}
	return s
}

// checkEbayField validates a pasted value: present, bounded and free of
// whitespace or control characters.
func checkEbayField(label, v string, maxLen int) string {
	if v == "" {
		return label + " is required."
	}
	if len(v) > maxLen || strings.ContainsFunc(v, func(r rune) bool { return r <= ' ' || r == 0x7f }) {
		return label + " looks wrong: copy it again without spaces."
	}
	return ""
}

func newEbayHTTPClient() *http.Client {
	return &http.Client{
		Timeout: ebayTokenTimeout,
		CheckRedirect: func(*http.Request, []*http.Request) error {
			return http.ErrUseLastResponse
		},
	}
}

func (h *DeviceMQTTHandler) ebayClient() *http.Client {
	if h.ebayHTTP != nil {
		return h.ebayHTTP
	}
	return newEbayHTTPClient()
}

// requestEbayToken posts one grant to the environment's token endpoint. Errors
// are fixed values: they never carry a secret, the code or the response body.
func requestEbayToken(ctx context.Context, client *http.Client, host, clientID, clientSecret string, form url.Values) (ebayToken, error) {
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, "https://"+host+"/identity/v1/oauth2/token", strings.NewReader(form.Encode()))
	if err != nil {
		return ebayToken{}, errEbayUnreachable
	}
	req.Header.Set("Content-Type", "application/x-www-form-urlencoded")
	req.Header.Set("Accept", "application/json")
	req.Header.Set("Authorization", "Basic "+base64.StdEncoding.EncodeToString([]byte(clientID+":"+clientSecret)))

	resp, err := client.Do(req)
	if err != nil {
		return ebayToken{}, errEbayUnreachable
	}
	defer resp.Body.Close()
	body, err := io.ReadAll(io.LimitReader(resp.Body, ebayMaxTokenResponse))
	if err != nil {
		return ebayToken{}, errEbayUnreachable
	}
	switch {
	case resp.StatusCode == http.StatusBadRequest || resp.StatusCode == http.StatusUnauthorized || resp.StatusCode == http.StatusForbidden:
		return ebayToken{}, errEbayRejected
	case resp.StatusCode != http.StatusOK:
		return ebayToken{}, errEbayUnreachable
	}
	var out struct {
		AccessToken           string `json:"access_token"`
		RefreshToken          string `json:"refresh_token"`
		RefreshTokenExpiresIn int    `json:"refresh_token_expires_in"`
	}
	if err := json.Unmarshal(body, &out); err != nil || out.AccessToken == "" {
		return ebayToken{}, errEbayUnreachable
	}
	return ebayToken{RefreshToken: out.RefreshToken, RefreshTokenExpiresIn: out.RefreshTokenExpiresIn}, nil
}

func (h *DeviceMQTTHandler) dropEbayTokenCache() {
	if h.connectorWriter == nil {
		return
	}
	if err := os.Remove(filepath.Join(h.connectorWriter.dir, ebayTokenCacheFile)); err != nil && !errors.Is(err, os.ErrNotExist) {
		slog.Warn("ebay: could not remove the token cache", "component", "mqtt", "error", err)
	}
}

// SetEbay handles PUT /api/device/connectors/ebay: exchanges the sign-in code
// for a refresh token (or checks a pasted refresh token), saves the link through
// the shared connector writer and reports the connected status (never a secret).
func (h *DeviceMQTTHandler) SetEbay(c *gin.Context) {
	var req ebayConnectRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, serializers.ResponseError("invalid body"))
		return
	}
	env := strings.ToLower(strings.TrimSpace(req.Environment))
	if env == "" {
		env = "production"
	}
	host, ok := ebayTokenHosts[env]
	if !ok {
		c.JSON(http.StatusBadRequest, serializers.ResponseError("Environment must be production or sandbox."))
		return
	}
	marketplace := strings.ToUpper(strings.TrimSpace(req.MarketplaceID))
	if marketplace == "" {
		marketplace = "EBAY_US"
	}
	clientID := strings.TrimSpace(req.ClientID)
	clientSecret := strings.TrimSpace(req.ClientSecret)
	ruName := strings.TrimSpace(req.RuName)
	code := ebayCodeFrom(req.Code)
	refresh := strings.TrimSpace(req.RefreshToken)
	for _, f := range []struct{ label, v string }{
		{"Marketplace", marketplace}, {"App ID (Client ID)", clientID}, {"Cert ID (Client Secret)", clientSecret},
	} {
		if msg := checkEbayField(f.label, f.v, maxEbayFieldLen); msg != "" {
			c.JSON(http.StatusBadRequest, serializers.ResponseError(msg))
			return
		}
	}

	form := url.Values{}
	switch {
	case strings.TrimSpace(req.Code) != "":
		if msg := checkEbayField("RuName", ruName, maxEbayFieldLen); msg != "" {
			c.JSON(http.StatusBadRequest, serializers.ResponseError(msg))
			return
		}
		if code == "" || len(code) > maxEbayCodeLen {
			c.JSON(http.StatusBadRequest, serializers.ResponseError("Paste the whole address eBay sent you to after signing in (it contains code=)."))
			return
		}
		form.Set("grant_type", "authorization_code")
		form.Set("code", code)
		form.Set("redirect_uri", ruName)
	case refresh != "":
		if msg := checkEbayField("Refresh token", refresh, maxEbayCodeLen); msg != "" {
			c.JSON(http.StatusBadRequest, serializers.ResponseError(msg))
			return
		}
		form.Set("grant_type", "refresh_token")
		form.Set("refresh_token", refresh)
	default:
		c.JSON(http.StatusBadRequest, serializers.ResponseError("Sign in with eBay first, then paste the address it sent you to."))
		return
	}

	writer := h.connectorWriterFor(ebayConnectorCode)
	if writer == nil || h.connectorWriter == nil {
		c.JSON(http.StatusServiceUnavailable, serializers.ResponseError("no writer available"))
		return
	}

	ctx, cancel := context.WithTimeout(c.Request.Context(), connectorHTTPTimeout)
	defer cancel()
	tok, err := requestEbayToken(ctx, h.ebayClient(), host, clientID, clientSecret, form)
	switch {
	case errors.Is(err, errEbayRejected):
		c.JSON(http.StatusBadRequest, serializers.ResponseError(ebayRejectedMessage))
		return
	case err != nil:
		c.JSON(http.StatusBadGateway, serializers.ResponseError(ebayUnreachableError))
		return
	}
	if tok.RefreshToken == "" {
		// A refresh grant answers without a new refresh token; keep the pasted one.
		tok.RefreshToken = refresh
	}
	if tok.RefreshToken == "" {
		c.JSON(http.StatusBadGateway, serializers.ResponseError("eBay didn't return a refresh token. Sign in again and paste the new address."))
		return
	}

	now := time.Now()
	credentials := map[string]string{
		"client_id": clientID, "client_secret": clientSecret,
		"marketplace_id": marketplace, "environment": env,
	}
	if ruName != "" {
		credentials["ru_name"] = ruName
	}
	var refreshExpiresAt int64
	if tok.RefreshTokenExpiresIn > 0 {
		refreshExpiresAt = now.Add(time.Duration(tok.RefreshTokenExpiresIn) * time.Second).Unix()
	}
	creds := ConnectorCreds{
		Connector:   ebayConnectorCode,
		AuthType:    "pat",
		APIKey:      tok.RefreshToken,
		ClientID:    clientID,
		Credentials: credentials,
		// The helper mints access tokens from the refresh token itself.
		Refresh:    false,
		ObtainedAt: now.Unix(),
	}
	if err := writer.Write(ctx, creds); err != nil {
		slog.Warn("ebay: could not save the connection", "component", "mqtt", "error", err)
		c.JSON(http.StatusInternalServerError, serializers.ResponseError("could not save the connection"))
		return
	}
	h.dropEbayTokenCache()
	h.publishShopifyResult("connector.set."+ebayConnectorCode, map[string]any{
		"connector": ebayConnectorCode,
		"auth_type": "pat",
	})

	c.JSON(http.StatusOK, serializers.ResponseSuccess(map[string]any{
		"connector":                ebayConnectorCode,
		"connected":                true,
		"environment":              env,
		"marketplace_id":           marketplace,
		"client_id":                clientID,
		"refresh_token_expires_at": refreshExpiresAt,
		"obtained_at":              creds.ObtainedAt,
	}))
}
