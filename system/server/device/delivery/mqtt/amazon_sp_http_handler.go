package mqtthandler

import (
	"context"
	"encoding/json"
	"errors"
	"io"
	"log/slog"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"time"

	"github.com/gin-gonic/gin"

	"go.autonomous.ai/os/system/server/serializers"
)

// The direct Amazon connector is linked with the seller's own private Selling Partner API app: the app's
// client ID and client secret, plus the refresh token Amazon gives when the seller authorizes that app on
// their account. The device checks the three once with Login with Amazon, saves them, and the connectors
// skill helper mints short-lived access tokens from them on demand.
const (
	amazonSPConnectorCode  = "amazon_sp"
	amazonSPTokenCacheFile = "amazon_sp.token.json"
	// amazonLWATokenURL is Login with Amazon's token endpoint: the only host a token request is sent to.
	amazonLWATokenURL = "https://api.amazon.com/auth/o2/token"

	amazonSPMintTimeout     = 20 * time.Second
	amazonSPMaxMintResponse = 64 << 10
	amazonSPDefaultLifetime = 3600
	amazonSPMinLifetime     = 60
	amazonSPMaxLifetime     = 24 * 3600
	maxAmazonSPFieldLen     = 2048

	amazonSPRejectedMessage  = "Amazon rejected the client ID, client secret or refresh token. Check all three and that the app is authorized on this account."
	amazonSPUnreachableError = "Couldn't reach Amazon to check the credentials. Check the device's internet connection and try again."
	amazonSPRegionHint       = "Choose North America, Europe or Far East."
	amazonSPSellerIDHint     = "The Seller ID looks like A1B2C3D4E5F6G7 (letters and numbers only). Find it in Seller Central under Account Info → Merchant Token."
)

// maskAmazonSecret redacts a saved credential for display: dots, plus the last 4 characters when the value is
// long enough that those reveal nothing useful. A short or empty value shows dots only (or nothing).
func maskAmazonSecret(v string) string {
	if v == "" {
		return ""
	}
	if len(v) < 24 {
		return "••••••••"
	}
	return "••••••••" + v[len(v)-4:]
}

// amazonSPSellerIDRE is the shape of a seller (merchant) ID: 8 to 20 letters and digits.
var amazonSPSellerIDRE = regexp.MustCompile(`^[A-Z0-9]{8,20}$`)

// normalizeAmazonSellerID uppercases and trims a pasted Seller ID; "" is allowed (it is optional).
func normalizeAmazonSellerID(raw string) (string, error) {
	s := strings.ToUpper(strings.TrimSpace(raw))
	if s == "" {
		return "", nil
	}
	if !amazonSPSellerIDRE.MatchString(s) {
		return "", errors.New(amazonSPSellerIDHint)
	}
	return s, nil
}

// amazonSPHosts maps a region to the one Selling Partner API host calls for it go to.
var amazonSPHosts = map[string]string{
	"na":         "sellingpartnerapi-na.amazon.com",
	"eu":         "sellingpartnerapi-eu.amazon.com",
	"fe":         "sellingpartnerapi-fe.amazon.com",
	"sandbox_na": "sandbox.sellingpartnerapi-na.amazon.com",
}

var (
	errAmazonSPRejected    = errors.New("amazon rejected the credentials")
	errAmazonSPUnreachable = errors.New("amazon unreachable")
)

// amazonSPConnectRequest is the body of PUT /api/device/connectors/amazon_sp.
type amazonSPConnectRequest struct {
	Region       string `json:"region"`
	ClientID     string `json:"client_id"`
	ClientSecret string `json:"client_secret"`
	RefreshToken string `json:"refresh_token"`
	// SellerID is Amazon's seller (merchant) ID. It is not a secret and cannot be discovered through the API,
	// but listing calls need it. Optional on connect; may also be sent alone to update a saved connection.
	SellerID string `json:"seller_id"`
}

// amazonSPInfoResponse is the read side. It never carries the secret, the refresh token or an access token.
type amazonSPInfoResponse struct {
	Connector       string `json:"connector"`
	Connected       bool   `json:"connected"`
	Region          string `json:"region,omitempty"`
	ClientID        string `json:"client_id,omitempty"`
	SellerID        string `json:"seller_id,omitempty"`
	HasSecret       bool   `json:"has_secret"`
	HasRefreshToken bool   `json:"has_refresh_token"`
	// SecretHint and RefreshTokenHint are masked: dots plus the last 4 characters of a long value, so the page
	// can show which saved value is which. The full values never leave the device.
	SecretHint       string `json:"client_secret_hint,omitempty"`
	RefreshTokenHint string `json:"refresh_token_hint,omitempty"`
	ObtainedAt       int64  `json:"obtained_at,omitempty"`
}

type amazonSPToken struct {
	AccessToken string
	ExpiresIn   int
}

func newAmazonSPHTTPClient() *http.Client {
	return &http.Client{
		Timeout: amazonSPMintTimeout,
		CheckRedirect: func(*http.Request, []*http.Request) error {
			return http.ErrUseLastResponse
		},
	}
}

func (h *DeviceMQTTHandler) amazonSPClient() *http.Client {
	if h.amazonSPHTTP != nil {
		return h.amazonSPHTTP
	}
	return newAmazonSPHTTPClient()
}

// checkAmazonSPField validates a pasted value: present, bounded, no whitespace or control characters.
func checkAmazonSPField(label, v string) string {
	if v == "" {
		return label + " is required."
	}
	if len(v) > maxAmazonSPFieldLen || strings.ContainsFunc(v, func(r rune) bool { return r <= ' ' || r == 0x7f }) {
		return label + " looks wrong: copy it again without spaces or line breaks."
	}
	return ""
}

// mintAmazonSPToken asks Login with Amazon for an access token from the refresh token. Errors are fixed
// values: they never carry any credential or the response body.
func mintAmazonSPToken(ctx context.Context, client *http.Client, clientID, clientSecret, refreshToken string) (amazonSPToken, error) {
	form := url.Values{}
	form.Set("grant_type", "refresh_token")
	form.Set("refresh_token", refreshToken)
	form.Set("client_id", clientID)
	form.Set("client_secret", clientSecret)

	req, err := http.NewRequestWithContext(ctx, http.MethodPost, amazonLWATokenURL, strings.NewReader(form.Encode()))
	if err != nil {
		return amazonSPToken{}, errAmazonSPUnreachable
	}
	req.Header.Set("Content-Type", "application/x-www-form-urlencoded")
	req.Header.Set("Accept", "application/json")

	resp, err := client.Do(req)
	if err != nil {
		return amazonSPToken{}, errAmazonSPUnreachable
	}
	defer resp.Body.Close()
	body, err := io.ReadAll(io.LimitReader(resp.Body, amazonSPMaxMintResponse))
	if err != nil {
		return amazonSPToken{}, errAmazonSPUnreachable
	}
	switch {
	case resp.StatusCode == http.StatusBadRequest || resp.StatusCode == http.StatusUnauthorized || resp.StatusCode == http.StatusForbidden:
		return amazonSPToken{}, errAmazonSPRejected
	case resp.StatusCode != http.StatusOK:
		return amazonSPToken{}, errAmazonSPUnreachable
	}
	var out struct {
		AccessToken string `json:"access_token"`
		ExpiresIn   int    `json:"expires_in"`
	}
	if err := json.Unmarshal(body, &out); err != nil || out.AccessToken == "" {
		return amazonSPToken{}, errAmazonSPUnreachable
	}
	life := out.ExpiresIn
	if life <= 0 {
		life = amazonSPDefaultLifetime
	}
	life = min(max(life, amazonSPMinLifetime), amazonSPMaxLifetime)
	return amazonSPToken{AccessToken: out.AccessToken, ExpiresIn: life}, nil
}

// dropAmazonSPTokenCache removes the helper's cached token so changed or removed credentials never reuse one.
func (h *DeviceMQTTHandler) dropAmazonSPTokenCache() {
	if h.connectorWriter == nil {
		return
	}
	if err := os.Remove(filepath.Join(h.connectorWriter.dir, amazonSPTokenCacheFile)); err != nil && !errors.Is(err, os.ErrNotExist) {
		slog.Warn("amazon: could not remove the token cache", "component", "mqtt", "error", err)
	}
}

func (h *DeviceMQTTHandler) publishAmazonSPResult(kind string, data map[string]any) {
	data["initiator"] = "device_local"
	if h.publishConnectorResult != nil {
		_ = h.publishConnectorResult(kind, "success", "", data)
		return
	}
	_ = h.publishDataResult(kind, "success", "", data)
}

// SetAmazonSP handles PUT /api/device/connectors/amazon_sp: checks the credentials with Login with Amazon
// once, saves them through the same writer every connector uses, and reports the connector as connected
// (status only; nothing secret is published).
func (h *DeviceMQTTHandler) SetAmazonSP(c *gin.Context) {
	var req amazonSPConnectRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, serializers.ResponseError("invalid body"))
		return
	}
	sellerID, err := normalizeAmazonSellerID(req.SellerID)
	if err != nil {
		c.JSON(http.StatusBadRequest, serializers.ResponseError(err.Error()))
		return
	}
	// Only the Seller ID was sent: update the saved connection without asking for the secrets again.
	if strings.TrimSpace(req.ClientID) == "" && strings.TrimSpace(req.ClientSecret) == "" && strings.TrimSpace(req.RefreshToken) == "" && sellerID != "" {
		h.updateAmazonSPSellerID(c, sellerID)
		return
	}
	region := strings.ToLower(strings.TrimSpace(req.Region))
	host, ok := amazonSPHosts[region]
	if !ok {
		c.JSON(http.StatusBadRequest, serializers.ResponseError(amazonSPRegionHint))
		return
	}
	clientID := strings.TrimSpace(req.ClientID)
	clientSecret := strings.TrimSpace(req.ClientSecret)
	refresh := strings.TrimSpace(req.RefreshToken)
	for _, f := range []struct{ label, v string }{{"Client ID", clientID}, {"Client secret", clientSecret}, {"Refresh token", refresh}} {
		if msg := checkAmazonSPField(f.label, f.v); msg != "" {
			c.JSON(http.StatusBadRequest, serializers.ResponseError(msg))
			return
		}
	}
	writer := h.connectorWriterFor(amazonSPConnectorCode)
	if writer == nil || h.connectorWriter == nil {
		c.JSON(http.StatusServiceUnavailable, serializers.ResponseError("no writer available"))
		return
	}

	ctx, cancel := context.WithTimeout(c.Request.Context(), connectorHTTPTimeout)
	defer cancel()
	tok, err := mintAmazonSPToken(ctx, h.amazonSPClient(), clientID, clientSecret, refresh)
	switch {
	case errors.Is(err, errAmazonSPRejected):
		c.JSON(http.StatusBadRequest, serializers.ResponseError(amazonSPRejectedMessage))
		return
	case err != nil:
		c.JSON(http.StatusBadGateway, serializers.ResponseError(amazonSPUnreachableError))
		return
	}

	now := time.Now()
	creds := ConnectorCreds{
		Connector:   amazonSPConnectorCode,
		AuthType:    "pat",
		AccessToken: tok.AccessToken,
		ExpiresAt:   now.Add(time.Duration(tok.ExpiresIn) * time.Second).Unix(),
		ClientID:    clientID,
		Credentials: map[string]string{
			"api_host": host, "region": region,
			"client_id": clientID, "client_secret": clientSecret, "refresh_token": refresh,
			"seller_id": sellerID,
		},
		// The helper mints the next token itself; the device's own refresh loop must not touch this entry.
		Refresh:    false,
		ObtainedAt: now.Unix(),
	}
	if err := writer.Write(ctx, creds); err != nil {
		slog.Warn("amazon: could not save the connection", "component", "mqtt", "error", err)
		c.JSON(http.StatusInternalServerError, serializers.ResponseError("could not save the connection"))
		return
	}
	h.dropAmazonSPTokenCache()
	h.publishAmazonSPResult("connector.set."+amazonSPConnectorCode, map[string]any{
		"connector": amazonSPConnectorCode,
		"auth_type": "pat",
	})
	c.JSON(http.StatusOK, serializers.ResponseSuccess(amazonSPInfoResponse{
		Connector: amazonSPConnectorCode, Connected: true, Region: region, ClientID: clientID, SellerID: sellerID,
		HasSecret: true, HasRefreshToken: true, ObtainedAt: creds.ObtainedAt,
		SecretHint: maskAmazonSecret(clientSecret), RefreshTokenHint: maskAmazonSecret(refresh),
	}))
}

// GetAmazonSP handles GET /api/device/connectors/amazon_sp.
func (h *DeviceMQTTHandler) GetAmazonSP(c *gin.Context) {
	empty := amazonSPInfoResponse{Connector: amazonSPConnectorCode}
	if h.connectorWriter == nil {
		c.JSON(http.StatusOK, serializers.ResponseSuccess(empty))
		return
	}
	creds, ok, err := h.connectorWriter.loadEntry(amazonSPConnectorCode)
	if err != nil {
		c.JSON(http.StatusInternalServerError, serializers.ResponseError("could not read the connection"))
		return
	}
	if !ok {
		c.JSON(http.StatusOK, serializers.ResponseSuccess(empty))
		return
	}
	clientID := creds.Credentials["client_id"]
	if clientID == "" {
		clientID = creds.ClientID
	}
	c.JSON(http.StatusOK, serializers.ResponseSuccess(amazonSPInfoResponse{
		Connector: amazonSPConnectorCode, Connected: true, Region: creds.Credentials["region"], ClientID: clientID,
		SellerID:         creds.Credentials["seller_id"],
		HasSecret:        creds.Credentials["client_secret"] != "",
		HasRefreshToken:  creds.Credentials["refresh_token"] != "",
		SecretHint:       maskAmazonSecret(creds.Credentials["client_secret"]),
		RefreshTokenHint: maskAmazonSecret(creds.Credentials["refresh_token"]),
		ObtainedAt:       creds.ObtainedAt,
	}))
}

// RemoveAmazonSP handles DELETE /api/device/connectors/amazon_sp.
func (h *DeviceMQTTHandler) RemoveAmazonSP(c *gin.Context) {
	writer := h.connectorWriterFor(amazonSPConnectorCode)
	if writer == nil {
		c.JSON(http.StatusServiceUnavailable, serializers.ResponseError("no writer available"))
		return
	}
	ctx, cancel := context.WithTimeout(c.Request.Context(), connectorHTTPTimeout)
	defer cancel()
	removed, err := writer.Remove(ctx, amazonSPConnectorCode)
	if err != nil {
		slog.Warn("amazon: could not remove the connection", "component", "mqtt", "error", err)
		c.JSON(http.StatusInternalServerError, serializers.ResponseError("could not remove the connection"))
		return
	}
	h.dropAmazonSPTokenCache()
	h.publishAmazonSPResult("connector.remove."+amazonSPConnectorCode, map[string]any{"connector": amazonSPConnectorCode, "removed": removed})
	c.JSON(http.StatusOK, serializers.ResponseSuccess(map[string]any{"connector": amazonSPConnectorCode, "removed": removed}))
}

// updateAmazonSPSellerID saves a new Seller ID on an existing connection and leaves every other value as it is.
func (h *DeviceMQTTHandler) updateAmazonSPSellerID(c *gin.Context, sellerID string) {
	writer := h.connectorWriterFor(amazonSPConnectorCode)
	if writer == nil || h.connectorWriter == nil {
		c.JSON(http.StatusServiceUnavailable, serializers.ResponseError("no writer available"))
		return
	}
	creds, ok, err := h.connectorWriter.loadEntry(amazonSPConnectorCode)
	if err != nil {
		c.JSON(http.StatusInternalServerError, serializers.ResponseError("could not read the connection"))
		return
	}
	if !ok {
		c.JSON(http.StatusBadRequest, serializers.ResponseError("Connect Amazon first: enter the client ID, client secret and refresh token."))
		return
	}
	if creds.Credentials == nil {
		creds.Credentials = map[string]string{}
	}
	creds.Credentials["seller_id"] = sellerID
	ctx, cancel := context.WithTimeout(c.Request.Context(), connectorHTTPTimeout)
	defer cancel()
	if err := writer.Write(ctx, creds); err != nil {
		slog.Warn("amazon: could not save the seller id", "component", "mqtt", "error", err)
		c.JSON(http.StatusInternalServerError, serializers.ResponseError("could not save the Seller ID"))
		return
	}
	c.JSON(http.StatusOK, serializers.ResponseSuccess(amazonSPInfoResponse{
		Connector: amazonSPConnectorCode, Connected: true, Region: creds.Credentials["region"],
		ClientID: creds.Credentials["client_id"], SellerID: sellerID,
		HasSecret: creds.Credentials["client_secret"] != "", HasRefreshToken: creds.Credentials["refresh_token"] != "",
		SecretHint:       maskAmazonSecret(creds.Credentials["client_secret"]),
		RefreshTokenHint: maskAmazonSecret(creds.Credentials["refresh_token"]),
		ObtainedAt:       creds.ObtainedAt,
	}))
}
