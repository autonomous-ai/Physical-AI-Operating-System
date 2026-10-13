package device

import (
	"log/slog"
	"math"
	"slices"

	"go.autonomous.ai/os/system/domain"
	"go.autonomous.ai/os/system/lib/hal"
	"go.autonomous.ai/os/system/lib/i18n"
	"go.autonomous.ai/os/system/server/config"
)

// WakeSettings is the wake section of web Settings in one reply (MQTT wakeword.settings.*).
// A listening window the owner never set is asked from HAL, whose image default varies by device.
func (s *Service) WakeSettings() domain.WakeSettings {
	agentName := i18n.DeviceName()
	deviceType := s.config.DeviceTypeOrDefault()
	out := domain.WakeSettings{
		Enabled:        s.config.WakeWordEnabled(),
		VoiceInputMode: s.config.GetVoiceInputMode(),
		AgentName:      agentName,
		Phrases:        effectiveWakePhrases(s.config, agentName, deviceType),
		PhrasesCustom:  len(s.config.WakePhrases) > 0,
		PhrasesDefault: i18n.BuildSupportedVoiceWakeWords(agentName, deviceType),
		MaxPhrases:     config.MaxWakePhrases,
		MaxPhraseChars: config.MaxWakePhraseRunes,
		Chime:          s.config.WakeChimeEnabled(),
		HasLight:       Has(deviceType, CapLight),
		Light:          wakeLightLook(s.config),
		LightEffects:   slices.Clone(config.WakeLightEffects),
		LightSpeedMin:  config.WakeLightSpeedMin,
		LightSpeedMax:  config.WakeLightSpeedMax,
		FollowupMinS:   config.WakeFollowupMinS,
		FollowupMaxS:   config.WakeFollowupMaxS,
	}
	if s.config.WakeFollowupS != nil {
		out.FollowupS, out.FollowupCustom = *s.config.WakeFollowupS, true
	} else {
		out.FollowupS = halWakeFollowup()
	}
	return out
}

// wakeLightLook fills the owner's saved wake light over the board's listening look.
func wakeLightLook(c *config.Config) domain.WakeLightLook {
	look := domain.WakeLightLook{
		Enabled: c.WakeLightEnabled(),
		Color:   slices.Clone(config.WakeLightDefault.Color),
		Effect:  config.WakeLightDefault.Effect,
		Speed:   config.WakeLightDefault.Speed,
	}
	if w := c.WakeLight; w != nil {
		if len(w.Color) == 3 {
			look.Color = slices.Clone(w.Color)
		}
		if w.Effect != "" {
			look.Effect = w.Effect
		}
		if w.Speed != 0 {
			look.Speed = w.Speed
		}
	}
	return look
}

// halWakeFollowup is HAL's listening window in effect, or the code default when HAL is down.
func halWakeFollowup() int {
	res, err := hal.GetWakeFollowup()
	if err != nil || res.Seconds <= 0 {
		slog.Warn("wake followup from HAL unavailable; reporting the code default", "component", "device", "error", err)
		return config.WakeFollowupDefaultS
	}
	return int(math.Round(res.Seconds))
}
