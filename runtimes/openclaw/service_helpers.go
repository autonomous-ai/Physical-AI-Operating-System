package openclaw

import (
	"context"
	"fmt"
	"os"
	"os/user"
	"strconv"
	"strings"
	"time"
)

func ensureMap(parent map[string]any, key string) map[string]any {
	existing, _ := parent[key].(map[string]any)
	if existing != nil {
		return existing
	}
	created := make(map[string]any)
	parent[key] = created
	return created
}

func setDefaultValue(target map[string]any, key string, value any) {
	existing, ok := target[key]
	if !ok || existing == nil {
		target[key] = value
		return
	}
	if s, ok := existing.(string); ok && strings.TrimSpace(s) == "" {
		target[key] = value
		return
	}
	if n, ok := existing.(float64); ok && n <= 0 {
		target[key] = value
		return
	}
	if n, ok := existing.(int); ok && n <= 0 {
		target[key] = value
		return
	}
	if n, ok := existing.(int64); ok && n <= 0 {
		target[key] = value
	}
}

func mergeStringList(existing any, required ...string) []string {
	list := make([]string, 0)
	seen := map[string]struct{}{}
	appendIfMissing := func(v string) {
		v = strings.TrimSpace(v)
		if v == "" {
			return
		}
		if _, ok := seen[v]; ok {
			return
		}
		seen[v] = struct{}{}
		list = append(list, v)
	}
	switch values := existing.(type) {
	case string:
		appendIfMissing(values)
	case []string:
		for _, v := range values {
			appendIfMissing(v)
		}
	case []any:
		for _, item := range values {
			if v, ok := item.(string); ok {
				appendIfMissing(v)
			}
		}
	}
	for _, v := range required {
		appendIfMissing(v)
	}
	return list
}

func getStringValue(m map[string]any, key string) string {
	if m == nil {
		return ""
	}
	value, _ := m[key].(string)
	return value
}

func chownRuntimeUserIfRoot(path, username string) error {
	if os.Geteuid() != 0 {
		return nil
	}
	u, err := user.Lookup(username)
	if err != nil {
		return fmt.Errorf("lookup user %q: %w", username, err)
	}
	uid, err := strconv.Atoi(u.Uid)
	if err != nil {
		return fmt.Errorf("parse uid for %q: %w", username, err)
	}
	gid, err := strconv.Atoi(u.Gid)
	if err != nil {
		return fmt.Errorf("parse gid for %q: %w", username, err)
	}
	if err := os.Chown(path, uid, gid); err != nil {
		return fmt.Errorf("chown %s to %s: %w", path, username, err)
	}
	return nil
}

func sleepCtx(ctx context.Context, d time.Duration) bool {
	t := time.NewTimer(d)
	defer t.Stop()
	select {
	case <-ctx.Done():
		return false
	case <-t.C:
		return true
	}
}

// setTelegramDMPolicy lets only the configured owner message the bot. Without a user
// ID nobody can: an open bot would let anyone who finds it command the device,
// camera snapshots included. A "*" left by an older config is removed.
func setTelegramDMPolicy(telegramMap map[string]any, userID string) {
	allow := make([]string, 0)
	for _, v := range mergeStringList(telegramMap["allowFrom"], userID) {
		if v != "*" {
			allow = append(allow, v)
		}
	}
	telegramMap["dmPolicy"] = "allowlist"
	telegramMap["allowFrom"] = allow
}
