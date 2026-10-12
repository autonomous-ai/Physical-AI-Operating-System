package i18n

import "testing"

func TestBuildChitchatWakeWordsPreservesLocalAttentionAliases(t *testing.T) {
	words := BuildChitchatWakeWords("Luna")
	want := []string{"hello luna", "hey luna", "này luna", "ê luna", "luna ơi", "luna"}
	if len(words) != len(want) {
		t.Fatalf("BuildChitchatWakeWords() = %v, want %v", words, want)
	}
	for i, word := range want {
		if words[i] != word {
			t.Fatalf("BuildChitchatWakeWords() = %v, want %v", words, want)
		}
	}
}

func TestBuildVoiceWakeWordsUsesHALPrefixes(t *testing.T) {
	words := BuildVoiceWakeWords("Luna")
	want := []string{"wake up luna", "hello luna", "okay luna", "hey luna", "hi luna", "alo luna", "ok luna"}
	if len(words) != len(want) {
		t.Fatalf("BuildVoiceWakeWords() = %v, want %v", words, want)
	}
	for i, word := range want {
		if words[i] != word {
			t.Fatalf("BuildVoiceWakeWords() = %v, want %v", words, want)
		}
	}
}

func TestBuildSupportedVoiceWakeWordsIncludesCurrentAndPermanentAliases(t *testing.T) {
	words := BuildSupportedVoiceWakeWords("Luna", "lamp")
	want := []string{
		"wake up autonomous", "hello autonomous", "okay autonomous", "hey autonomous", "hi autonomous", "alo autonomous", "ok autonomous",
		"wake up lamp", "hello lamp", "okay lamp", "hey lamp", "hi lamp", "alo lamp", "ok lamp",
		"wake up luna", "hello luna", "okay luna", "hey luna", "hi luna", "alo luna", "ok luna",
	}
	if len(words) != len(want) {
		t.Fatalf("BuildSupportedVoiceWakeWords() = %v, want %v", words, want)
	}
	for i, word := range want {
		if words[i] != word {
			t.Fatalf("BuildSupportedVoiceWakeWords() = %v, want %v", words, want)
		}
	}
}

func TestOwnerWakePhrasesStripLongestFirst(t *testing.T) {
	SetOwnerWakePhrases([]string{"bé na", "ê bé na ơi"})
	t.Cleanup(func() { SetOwnerWakePhrases(nil) })

	words := ChitchatWakeWords()
	if len(words) < 2 || words[0] != "ê bé na ơi" || words[1] != "bé na" {
		t.Fatalf("owner phrases must lead, longest first: %v", words)
	}
	SetOwnerWakePhrases(nil)
	for _, w := range ChitchatWakeWords() {
		if w == "bé na" {
			t.Fatal("cleared owner phrases must not linger")
		}
	}
}
