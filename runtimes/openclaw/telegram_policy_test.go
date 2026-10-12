package openclaw

import (
	"reflect"
	"testing"
)

func TestTelegramDMPolicyIsClosedWithoutUserID(t *testing.T) {
	for _, tc := range []struct {
		name     string
		existing any
		userID   string
		want     []string
	}{
		{"no user id: nobody", nil, "", []string{}},
		{"old open config is closed", []any{"*"}, "", []string{}},
		{"owner only", nil, "123", []string{"123"}},
		{"owner replaces an old wildcard", []any{"*"}, "123", []string{"123"}},
	} {
		t.Run(tc.name, func(t *testing.T) {
			m := map[string]any{}
			if tc.existing != nil {
				m["allowFrom"] = tc.existing
			}
			setTelegramDMPolicy(m, tc.userID)
			if m["dmPolicy"] != "allowlist" {
				t.Fatalf("dmPolicy = %v, want allowlist", m["dmPolicy"])
			}
			if got := m["allowFrom"].([]string); !reflect.DeepEqual(got, tc.want) {
				t.Fatalf("allowFrom = %v, want %v", got, tc.want)
			}
		})
	}
}
