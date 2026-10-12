import { useEffect, useState } from "react";
import { toast } from "sonner";
import { C, SectionCard } from "@/components/setup/shared";
import { getDeviceConfig, updateDeviceConfig } from "@/lib/api";
import type { CloudPerception } from "@/lib/api";

const ALL_OFF: CloudPerception = { motion: false, emotion: false, pose: false, fire_hazard: false };

// Each entry states what leaves the device and how often, so turning it on is an informed choice.
const OPTIONS: { key: keyof CloudPerception; label: string; detail: string }[] = [
  {
    key: "pose",
    label: "Posture reminders",
    detail: "Sends full camera frames about every 2 seconds, even when nobody is in the room.",
  },
  {
    key: "fire_hazard",
    label: "Fire and smoke hints",
    detail: "Sends full camera frames about every 5 seconds, even when nobody is in the room. "
      + "Best effort only: this is not a smoke or fire alarm and does not replace one.",
  },
  {
    key: "motion",
    label: "Activity recognition",
    detail: "Sends full camera frames about every 2 seconds while someone has been seen recently.",
  },
  {
    key: "emotion",
    label: "Facial emotion",
    detail: "Sends a cropped photo of each detected face about every 2 seconds.",
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

  useEffect(() => {
    getDeviceConfig()
      .then((cfg) => {
        const current = { ...ALL_OFF, ...cfg.cloud_perception };
        setSaved(current);
        setDraft(current);
        setRecipient(recipientHost(cfg.llm_base_url));
      })
      .catch(() => toast.error("Could not load privacy settings"));
  }, []);

  async function apply() {
    setApplying(true);
    try {
      await updateDeviceConfig({ cloud_perception: draft });
      setSaved(draft);
      toast.success("Privacy settings applied");
    } catch (e) {
      toast.error((e as Error).message || "Could not apply privacy settings");
    } finally {
      setApplying(false);
    }
  }

  const dirty = saved !== null && !same(saved, draft);

  return (
    <SectionCard id="privacy" title="Privacy" active={active}>
      <div style={{ fontSize: 12.5, color: C.textDim, marginBottom: 14, lineHeight: 1.6 }}>
        Face detection, Face ID, presence and gaze run on this device. The features below
        need a cloud server, so they upload camera images and stay off until you turn them
        on. Anyone in view of the camera is included, not only you. Turning a feature off
        stops new uploads; it does not recall images already sent.
      </div>
      <div style={{ fontSize: 12.5, color: C.text, marginBottom: 14 }}>
        Images are sent to: <strong>{recipient || "no server configured (nothing is sent)"}</strong>
      </div>

      {saved === null ? (
        <div style={{ fontSize: 12, color: C.textMuted }}>Loading…</div>
      ) : (
        <>
          {OPTIONS.map(({ key, label, detail }) => (
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
                <span style={{ display: "block", fontSize: 12, color: C.textMuted, lineHeight: 1.5 }}>{detail}</span>
              </span>
            </label>
          ))}

          <div style={{ display: "flex", alignItems: "center", gap: 12, marginTop: 6 }}>
            <button
              type="button"
              onClick={() => { void apply(); }}
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
              Applying restarts the hardware service; voice is unavailable for a few seconds.
            </span>
          </div>
        </>
      )}
    </SectionCard>
  );
}
