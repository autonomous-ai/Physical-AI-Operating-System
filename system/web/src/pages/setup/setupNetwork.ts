// Resolve the submitted network and validate it without adding a network round trip.
export function resolveSetupNetwork({
  ssid, password, wifiConnected, currentSsid, wiredUplink,
  hasNetworkPassword, savedSsid,
}: {
  ssid: string;
  password: string;
  wifiConnected: boolean;
  currentSsid: string;
  wiredUplink: boolean;
  hasNetworkPassword: boolean;
  savedSsid: string;
}) {
  const targetSsid = ssid.trim() || (wifiConnected ? currentSsid : "");
  const usingCurrentWifi = wifiConnected && !!currentSsid && targetSsid === currentSsid;
  const passwordConfigured = hasNetworkPassword && !!targetSsid && targetSsid === savedSsid;
  let error: string | null = null;
  if (!targetSsid && !wiredUplink) {
    error = "Choose a Wi-Fi network before continuing.";
  } else if (targetSsid && !usingCurrentWifi && !password && !passwordConfigured) {
    error = "Enter the Wi-Fi password.";
  }
  return { ssid: targetSsid, passwordConfigured, usingCurrentWifi, error };
}
