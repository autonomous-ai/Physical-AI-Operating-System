package http

import "testing"

func TestPosePhotoRecipientIsOwner(t *testing.T) {
	cases := []struct {
		owner, recipient string
		want             bool
	}{
		{"12345", "12345", true},
		{" 12345 ", "12345", true},
		{"12345", "67890", false},
		{"", "12345", false}, // no owner configured: never attach photos
		{"", "", false},
	}
	for _, c := range cases {
		if got := posePhotoRecipientIsOwner(c.owner, c.recipient); got != c.want {
			t.Errorf("owner=%q recipient=%q: got %v, want %v", c.owner, c.recipient, got, c.want)
		}
	}
}
