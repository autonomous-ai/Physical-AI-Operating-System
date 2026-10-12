import { useEffect, useState } from "react";
import { toast } from "sonner";
import { C, ConfirmDialog, SectionCard } from "@/components/setup/shared";
import { disableGuardMode, getDeviceConfig, getGuardMode, updateDeviceConfig } from "@/lib/api";
import type { CloudPerception } from "@/lib/api";

const ALL_OFF: CloudPerception = { motion: false, emotion: false, pose: false, fire_hazard: false };

// Each entry says what the camera is used for and exactly what is uploaded for cloud
// analysis, so turning it on is an informed choice.
const OPTIONS: { key: keyof CloudPerception; label: string; purpose: string; upload: string; short: string }[] = [
  {
    key: "pose",
    label: "Posture reminders",
    purpose: "Uses the camera to notice when you sit badly for a long time and reminds you to adjust. "
      + "A reminder sent to your Telegram includes the photos with the worst posture.",
    upload: "Uploads one full camera image for cloud analysis about every 30 seconds while someone is in front of the lamp.",
    short: "Posture reminders (one full image about every 30 s while someone is in front of the lamp; "
      + "reminders sent to Telegram include photos)",
  },
  {
    key: "fire_hazard",
    label: "Fire and smoke hints",
    purpose: "Uses the camera to look for visible flames or smoke and mention them. Best effort only: "
      + "this is not a smoke or fire alarm and does not replace one.",
    upload: "Uploads full camera images for cloud analysis about every 5 seconds, even when nobody is in the room.",
    short: "Fire and smoke hints (full images, about every 5 s, even with nobody in the room)",
  },
  {
    key: "motion",
    label: "Activity recognition",
    purpose: "Uses the camera to recognise what you are doing (for example working or stretching) so the lamp can react.",
    upload: "Uploads full camera images for cloud analysis about every 2 seconds while someone has been seen recently.",
    short: "Activity recognition (full images, about every 2 s while someone is around)",
  },
  {
    key: "emotion",
    label: "Facial emotion",
    purpose: "Uses the camera to read facial expressions so the lamp can respond to how people seem to feel.",
    upload: "Uploads a cropped photo of each detected face for cloud analysis about every 2 seconds.",
    short: "Facial emotion (a cropped photo of each face, about every 2 s)",
  },
];

// Perception requests go to the configured LLM base URL (HAL DL_BACKEND_URL), so show its host.
function recipientHost(baseUrl: string | undefined): string {
  if (!baseUrl) return "";
  try {
    return new URL(baseUrl).host;
  } catch {
    return baseUrl;
  }
}

function same(a: CloudPerception, b: CloudPerception): boolean {
  return OPTIONS.every(({ key }) => a[key] === b[key]);
}

export function PrivacySection({ active }: { active: boolean }) {
  const [saved, setSaved] = useState<CloudPerception | null>(null);
  const [draft, setDraft] = useState<CloudPerception>(ALL_OFF);
  const [applying, setApplying] = useState(false);
  const [recipient, setRecipient] = useState("");
  const [confirming, setConfirming] = useState(false);
  const [guard, setGuard] = useState<boolean | null>(null);
  const [guardBusy, setGuardBusy] = useState(false);

  useEffect(() => {
    getDeviceConfig()
      .then((cfg) => {
        const current = { ...ALL_OFF, ...cfg.cloud_perception };
        setSaved(current);
        setDraft(current);
        setRecipient(recipientHost(cfg.llm_base_url));
      })
      .catch(() => toast.error("Could not load privacy settings"));
    getGuardMode().then(setGuard).catch(() => setGuard(null));
  }, []);

  async function turnOffGuard() {
    setGuardBusy(true);
    try {
      await disableGuardMode();
      setGuard(false);
      toast.success("Guard mode is off");
    } catch (e) {
      toast.error((e as Error).message || "Could not turn off guard mode");
    } finally {
      setGuardBusy(false);
    }
  }

  async function apply() {
    setApplying(true);
    try {
      await updateDeviceConfig({ cloud_perception: draft });
      setSaved(draft);
      toast.success("Applied on the lamp");
    } catch (e) {
      toast.error((e as Error).message || "Could not apply privacy settings");
    } finally {
      setApplying(false);
    }
  }

  const dirty = saved !== null && !same(saved, draft);
  // Only turning something on needs consent; turning off applies without a prompt.
  const newlyEnabled = saved === null ? [] : OPTIONS.filter(({ key }) => draft[key] && !saved[key]);
  const destination = recipient || "the configured server";

  return (
    <SectionCard id="privacy" title="Privacy" active={active}>
      <div style={{ fontSize: 12.5, color: C.textDim, marginBottom: 14, lineHeight: 1.6 }}>
        Face detection, Face ID, presence and gaze run on this device and send no images to
        a cloud server. When a face without a name has been seen twice, the lamp keeps one
        photo on the device for 24 hours so it can ask whether to remember that person.
        The features below need a cloud server: when on, the lamp uploads camera images for
        analysis. They are off until you turn them on. Anyone in view of the camera is
        included, not only you. Turning a feature off stops its new uploads once the lamp
        confirms the change; an upload already in progress may still finish, and images
        already sent are not recalled.
      </div>
      <div style={{ fontSize: 12.5, color: C.text, marginBottom: 14 }}>
        Images are sent to: <strong>{recipient || "no server configured (nothing is sent)"}</strong>
      </div>

      {saved === null ? (
        <div style={{ fontSize: 12, color: C.textMuted }}>Loading…</div>
      ) : (
        <>
          {OPTIONS.map(({ key, label, purpose, upload }) => (
            <label
              key={key}
              htmlFor={`privacy-${key}`}
              style={{ display: "flex", alignItems: "flex-start", gap: 10, marginBottom: 14, cursor: "pointer" }}
            >
              <input
                id={`privacy-${key}`}
                type="checkbox"
                style={{ flexShrink: 0, marginTop: 3 }}
                checked={draft[key]}
                onChange={(e) => setDraft({ ...draft, [key]: e.target.checked })}
              />
              <span>
                <span style={{ display: "block", fontSize: 13.5, color: C.text }}>{label}</span>
                <span style={{ display: "block", fontSize: 12, color: C.textMuted, lineHeight: 1.5 }}>{purpose}</span>
                <span style={{ display: "block", fontSize: 12, color: C.textMuted, lineHeight: 1.5 }}>
                  {upload} Sent to {destination}.
                </span>
              </span>
            </label>
          ))}

          <div style={{ display: "flex", alignItems: "center", gap: 12, marginTop: 6 }}>
            <button
              type="button"
              onClick={() => { if (newlyEnabled.length > 0) setConfirming(true); else void apply(); }}
              disabled={!dirty || applying}
              style={{
                padding: "6px 18px", borderRadius: 8, fontSize: 12, fontWeight: 600, border: "none",
                cursor: !dirty || applying ? "not-allowed" : "pointer",
                background: !dirty || applying ? C.surface : C.amber,
                color: !dirty || applying ? C.textMuted : "var(--lm-on-amber)",
                opacity: !dirty || applying ? 0.6 : 1,
              }}
            >
              {applying ? "Applying…" : "Apply"}
            </button>
            <span style={{ fontSize: 12, color: C.textMuted }}>
              Apply waits until the lamp confirms the change.
            </span>
          </div>
        </>
      )}

      <div style={{ borderTop: `1px solid ${C.border}`, marginTop: 18, paddingTop: 14 }}>
        <div style={{ fontSize: 13.5, color: C.text }}>
          Guard mode: <strong>{guard === null ? "unknown" : guard ? "on" : "off"}</strong>
        </div>
        <div style={{ fontSize: 12, color: C.textMuted, lineHeight: 1.5, marginTop: 4 }}>
          While guard mode is on, each person, movement or fire hint the lamp notices is sent to
          your Telegram with a camera photo. It is off until you ask the lamp to guard the room.
        </div>
        {guard && (
          <button
            type="button"
            onClick={() => void turnOffGuard()}
            disabled={guardBusy}
            style={{
              marginTop: 10, padding: "6px 18px", borderRadius: 8, fontSize: 12, fontWeight: 600,
              border: "none", cursor: guardBusy ? "not-allowed" : "pointer",
              background: C.amber, color: "var(--lm-on-amber)", opacity: guardBusy ? 0.6 : 1,
            }}
          >
            {guardBusy ? "Turning off…" : "Turn off guard mode"}
          </button>
        )}
      </div>

      {confirming && (
        <ConfirmDialog
          title="Upload camera images to the cloud?"
          message={`The lamp will upload camera images to ${destination} for cloud analysis: `
            + `${newlyEnabled.map((o) => o.short).join("; ")}. `
            + "Anyone in view of the camera is included. You can turn this off at any time; "
            + "the lamp then stops new uploads once it confirms, but images already sent are not recalled."}
          confirmLabel="Turn on and upload"
          onConfirm={() => { setConfirming(false); void apply(); }}
          onCancel={() => setConfirming(false)}
        />
      )}
    </SectionCard>
  );
}
