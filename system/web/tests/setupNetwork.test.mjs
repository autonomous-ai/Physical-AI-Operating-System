import assert from "node:assert/strict";
import test from "node:test";
import { resolveSetupNetwork } from "../src/pages/setup/setupNetwork.ts";

const initial = {
  ssid: "", password: "", wifiConnected: false, currentSsid: "",
  wiredUplink: false, hasNetworkPassword: false, savedSsid: "",
};
const connected = { ...initial, wifiConnected: true, currentSsid: "Pollen" };

test("existing online Wi-Fi needs no password and submits its actual SSID", () => {
  for (const ssid of ["", "Pollen", " Pollen "]) {
    const result = resolveSetupNetwork({ ...connected, ssid });
    assert.equal(result.error, null);
    assert.equal(result.ssid, "Pollen");
    assert.equal(result.usingCurrentWifi, true);
  }
});

test("wired connection is optional only while no Wi-Fi network is selected", () => {
  assert.equal(resolveSetupNetwork({ ...initial, wiredUplink: true }).error, null);
  assert.equal(resolveSetupNetwork({ ...initial, wiredUplink: true, ssid: "Other" }).error,
    "Enter the Wi-Fi password.");
});

test("choosing another network does not inherit the current connection or its saved password", () => {
  const selection = { ...connected, ssid: "Other", hasNetworkPassword: true, savedSsid: "Pollen" };
  const result = resolveSetupNetwork(selection);
  assert.equal(result.error, "Enter the Wi-Fi password.");
  assert.equal(result.passwordConfigured, false);
  assert.equal(result.usingCurrentWifi, false);
  assert.equal(resolveSetupNetwork({ ...selection, password: "new-password" }).error, null);
});

test("a saved password counts only for the selected SSID", () => {
  const selection = { ...initial, ssid: "Saved", hasNetworkPassword: true, savedSsid: "Saved" };
  assert.equal(resolveSetupNetwork(selection).error, null);
  assert.equal(resolveSetupNetwork(selection).passwordConfigured, true);
  assert.equal(resolveSetupNetwork({ ...selection, hasNetworkPassword: false }).error,
    "Enter the Wi-Fi password.");
});

test("an unresolved or offline probe cannot authorize password-free submission", () => {
  assert.equal(resolveSetupNetwork(initial).error, "Choose a Wi-Fi network before continuing.");
  assert.equal(resolveSetupNetwork({ ...connected, wifiConnected: false, ssid: "Pollen" }).error,
    "Enter the Wi-Fi password.");
  assert.equal(resolveSetupNetwork({ ...initial, ssid: "New", password: "secret" }).error, null);
  assert.equal(resolveSetupNetwork(connected).error, null);
});

test("repeated validation does not mutate input or lose the active SSID", () => {
  const input = Object.freeze({ ...connected });
  const first = resolveSetupNetwork(input);
  for (let i = 0; i < 100; i++) assert.deepEqual(resolveSetupNetwork(input), first);
});
