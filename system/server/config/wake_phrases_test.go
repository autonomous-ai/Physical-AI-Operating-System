package config

import (
	"reflect"
	"strings"
	"testing"
)

func TestNormalizeWakePhrases(t *testing.T) {
	got, err := NormalizeWakePhrases([]string{"  Hey   Sam ", "", "hey sam", "Ê Bé Na Ơi"})
	if err != nil || !reflect.DeepEqual(got, []string{"hey sam", "ê bé na ơi"}) {
		t.Fatalf("got %v, %v", got, err)
	}
	if got, err := NormalizeWakePhrases(nil); err != nil || len(got) != 0 {
		t.Fatalf("nil: %v, %v", got, err)
	}
	for _, bad := range [][]string{
		{strings.Repeat("a", MaxWakePhraseRunes+1)},
		{"123 456"},
	} {
		if _, err := NormalizeWakePhrases(bad); err == nil {
			t.Fatalf("%q must be rejected", bad)
		}
	}
	many := make([]string, MaxWakePhrases+1)
	for i := range many {
		many[i] = "hey " + strings.Repeat("x", i+1)
	}
	if _, err := NormalizeWakePhrases(many); err == nil {
		t.Fatal("more than MaxWakePhrases must be rejected")
	}
}

func TestValidateWakeFollowupBounds(t *testing.T) {
	for _, ok := range []int{WakeFollowupMinS, WakeFollowupDefaultS, WakeFollowupMaxS} {
		if err := ValidateWakeFollowup(ok); err != nil {
			t.Fatalf("%d: %v", ok, err)
		}
	}
	for _, bad := range []int{0, WakeFollowupMinS - 1, WakeFollowupMaxS + 1} {
		if ValidateWakeFollowup(bad) == nil {
			t.Fatalf("%d must be rejected", bad)
		}
	}
}

func TestWakeLightValidate(t *testing.T) {
	good := []WakeLight{{}, {Effect: "blink", Speed: 0.25, Color: []int{255, 0, 0}}, {Effect: "solid", Speed: 3}}
	for _, w := range good {
		if err := w.Validate(); err != nil {
			t.Fatalf("%+v: %v", w, err)
		}
	}
	bad := []WakeLight{{Effect: "rainbow"}, {Speed: 5}, {Color: []int{0, 0, 0}}, {Color: []int{1, 2}}, {Color: []int{256, 0, 0}}}
	for _, w := range bad {
		if w.Validate() == nil {
			t.Fatalf("%+v must be rejected", w)
		}
	}
}
