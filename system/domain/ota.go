package domain

// OTAComponent describes version and download URL for a single component.
// Auto-update triggers only below MinVersion (empty = Version); manual software-update always installs Version.
type OTAComponent struct {
	Service    string   `json:"service,omitempty"`
	Replaces   []string `json:"replaces,omitempty"`
	Version    string   `json:"version"`
	MinVersion string   `json:"min_version,omitempty"`
	// MinVersionByDevice overrides MinVersion per device_type; types not listed fall back to MinVersion.
	MinVersionByDevice map[string]string `json:"min_version_by_device,omitempty"`
	// ActivateByDevice authorizes first installation of a service for an exact device type.
	ActivateByDevice map[string]bool `json:"activate_by_device,omitempty"`
	URL              string          `json:"url"`
	SHA256           string          `json:"sha256,omitempty"`
	// Commit pins a git-installed component (hermes) to an upstream commit; empty = unpinned (HEAD).
	Commit string `json:"commit,omitempty"`
}

const (
	OTAKeyOSServer  = "os-server"
	OTAKeyBootstrap = "bootstrap"
	OTAKeyOpenClaw  = "openclaw"
	OTAKeyWeb       = "web"
	OTAKeyHal       = "hal"
	OTAKeyBuddy     = "claude-desktop-buddy"
	// OTAKeyDevice is not a flat metadata key; the profile lives at metadata.devices.<device_type>.
	OTAKeyDevice = "device"

	// Agent-runtime CLI keys must equal the config.json `agent_runtime` name; bootstrap updates only the active one.
	// Hermes auto-updates only when its entry carries a Commit (unpinned min_version is unreachable).
	OTAKeyHermes     = "hermes"
	OTAKeyCodex      = "codex"
	OTAKeyClaudeCode = "claudecode"
	OTAKeyOpenCode   = "opencode"
	// OTAKeyPicoClaw's version is a release tag, read from PicoClawVersionStamp.
	OTAKeyPicoClaw = "picoclaw"
)

// PicoClawVersionStamp records the installed PicoClaw release tag (the binary cannot report it).
const PicoClawVersionStamp = "/usr/local/lib/os-runtimes/picoclaw/installed-version"

// OTAMetadata is the JSON shape returned by the OTA metadata URL, keyed by component.
type OTAMetadata map[string]OTAComponent
