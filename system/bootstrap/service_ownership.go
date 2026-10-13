package bootstrap

import (
	"fmt"
	"go.autonomous.ai/os/system/domain"
	"os"
	"path/filepath"
	"sort"
)

const ServiceOTAProtocol = 1

func (b *Bootstrap) serviceDirectory() string {
	if b.serviceStateDir != "" {
		return b.serviceStateDir
	}
	return "/root/bootstrap/services"
}
func (b *Bootstrap) serviceRegistry() (*domain.OTAServiceRegistry, error) {
	return domain.LoadOTAServiceRegistry(filepath.Join(b.serviceDirectory(), "ownership.json"))
}
func (b *Bootstrap) serviceTargetAllowed(key string) bool {
	r, err := b.serviceRegistry()
	if err != nil || !r.Allows(key) {
		return false
	}
	record, registered := r.Components[key]
	return forceTargetAllowed[key] || key == domain.OTAKeyBuddy || (registered && record.Service == key)
}
func (b *Bootstrap) serviceKeys() []string {
	r, err := b.serviceRegistry()
	if err != nil {
		return nil
	}
	keys := []string{}
	for k := range r.Components {
		if r.Allows(k) {
			keys = append(keys, k)
		}
	}
	sort.Strings(keys)
	return keys
}
func (b *Bootstrap) halOwned() bool {
	r, err := b.serviceRegistry()
	return err == nil && r.Allows(domain.OTAKeyHal)
}
func (b *Bootstrap) serviceUpdaterProtected() bool {
	for _, name := range []string{"ownership.json", "transition.json"} {
		if _, err := os.Lstat(filepath.Join(b.serviceDirectory(), name)); !os.IsNotExist(err) {
			return true
		}
	}
	return false
}
func (b *Bootstrap) validateServiceTarget(key string, c domain.OTAComponent) error {
	r, err := b.serviceRegistry()
	if err != nil {
		return err
	}
	if !r.Allows(key) {
		return fmt.Errorf("component %q replaced by another service", key)
	}
	if _, ok := r.Components[key]; ok {
		return r.ValidateTopology(key, c)
	}
	if c.Service != "" || len(c.Replaces) > 0 {
		return fmt.Errorf("service %q requires explicit activation", key)
	}
	return nil
}
