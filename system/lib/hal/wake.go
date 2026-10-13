package hal

import "net/http"

// WakeFollowup is HAL GET /voice/wake-followup: the listening window in effect and the image default.
type WakeFollowup struct {
	Seconds       float64 `json:"seconds"`
	ImageDefaultS float64 `json:"image_default_s"`
}

// WakeLightLook is the body of HAL POST /led/wake-light/preview.
type WakeLightLook struct {
	Color  []int   `json:"color"`
	Effect string  `json:"effect,omitempty"`
	Speed  float64 `json:"speed,omitempty"`
}

// GetWakeFollowup reads the listening window after a wake that HAL applies now.
func GetWakeFollowup() (*WakeFollowup, error) {
	var out WakeFollowup
	if err := doJSON(http.MethodGet, "/voice/wake-followup", nil, &out); err != nil {
		return nil, err
	}
	return &out, nil
}

// PreviewWakeLight plays a candidate wake light for a few seconds without saving it.
// Painted is false while sleep, speech, music or the mic-muted light owns the strip.
func PreviewWakeLight(look WakeLightLook) (*RestingLEDPreview, error) {
	var out RestingLEDPreview
	if err := doJSON(http.MethodPost, "/led/wake-light/preview", look, &out); err != nil {
		return nil, err
	}
	return &out, nil
}
