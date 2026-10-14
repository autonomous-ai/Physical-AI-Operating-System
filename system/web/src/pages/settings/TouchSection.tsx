import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import { C, SectionCard, LABEL_STYLE } from "@/components/setup/shared";
import { hwUrl } from "@/lib/api";
import { useEventSource } from "@/hooks/useEventSource";

interface TouchConfig {
  running: boolean;
  touch: number[];
  release: number[];
  sensitivity: number;
  sensitivity_range: [number, number];
  swipe_axis: number[];
  noise: number[] | null;
  warnings: number[];
  user_adjusted: boolean;
}

interface StreamFrame { ready: boolean; delta?: number[]; touched?: boolean[]; touch?: number[]; release?: number[] }

const PADS = 12;
const BAR_HEIGHT = 150;
// Auto-calibration listens this long; the strip must not be touched meanwhile.
const CALIBRATE_S = 10;
const SLIDER_STYLE: React.CSSProperties = { width: "100%", accentColor: "var(--lm-amber)" };

function sensitivityLabel(value: number, [min, max]: [number, number]): string {
  const mid = Math.round((min + max) / 2);
  if (value === mid) return "Calibrated";
  return value < mid ? `Lighter (${mid - value})` : `Firmer (${value - mid})`;
}

async function call(path: string, init?: RequestInit): Promise<TouchConfig> {
  const r = await fetch(hwUrl(path), init);
  if (!r.ok) {
    let detail = `HTTP ${r.status}`;
    try { detail = (await r.json()).detail ?? detail; } catch { /* plain-text error */ }
    throw new Error(detail);
  }
  return r.json();
}

export function TouchSection({ active }: { active: boolean }) {
  const [config, setConfig] = useState<TouchConfig | null>(null);
  const [unavailable, setUnavailable] = useState(false);
  const [sensitivity, setSensitivity] = useState(5);
  const [busy, setBusy] = useState(false);
  const [calibrating, setCalibrating] = useState(0);
  const [advanced, setAdvanced] = useState(false);
  const timer = useRef<number | null>(null);
  // Bars are updated straight from the 10 Hz stream through refs, not React state.
  const fills = useRef<(HTMLDivElement | null)[]>([]);
  const touchLines = useRef<(HTMLDivElement | null)[]>([]);
  const releaseLines = useRef<(HTMLDivElement | null)[]>([]);
  const values = useRef<(HTMLSpanElement | null)[]>([]);

  const apply = useCallback((c: TouchConfig) => {
    setConfig(c);
    setSensitivity(c.sensitivity);
  }, []);

  useEffect(() => {
    if (!active || config) return;
    let cancelled = false;
    call("/touch/config")
      .then((c) => { if (!cancelled) apply(c); })
      .catch(() => { if (!cancelled) setUnavailable(true); });
    return () => { cancelled = true; };
  }, [active, config, apply]);

  useEffect(() => () => { if (timer.current) window.clearTimeout(timer.current); }, []);

  useEventSource(hwUrl("/touch/stream"), {
    onMessage: (ev) => {
      const frame: StreamFrame = JSON.parse(ev.data);
      if (!frame.ready || !frame.delta || !frame.touch || !frame.release) return;
      // Full scale: twice the highest touch threshold, so a firm press fills the bar.
      const scale = Math.max(30, 2 * Math.max(...frame.touch));
      for (let pad = 0; pad < PADS; pad++) {
        const pct = (v: number) => `${Math.max(0, Math.min(100, (v / scale) * 100))}%`;
        const fill = fills.current[pad];
        if (fill) {
          fill.style.height = pct(frame.delta[pad]);
          fill.style.background = frame.touched?.[pad] ? C.amber : "var(--lm-text-muted)";
        }
        const t = touchLines.current[pad];
        if (t) t.style.bottom = pct(frame.touch[pad]);
        const r = releaseLines.current[pad];
        if (r) r.style.bottom = pct(frame.release[pad]);
        const v = values.current[pad];
        if (v) v.textContent = String(frame.delta[pad]);
      }
    },
  }, { enabled: active && !!config });

  const saveSensitivity = useCallback(async (value: number) => {
    setBusy(true);
    try {
      apply(await call("/touch/config", {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ sensitivity: value }),
      }));
    } catch (e) {
      toast.error(`Could not update touch sensitivity: ${(e as Error).message}`);
    } finally {
      setBusy(false);
    }
  }, [apply]);

  const onSlider = (value: number) => {
    setSensitivity(value);
    if (timer.current) window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => { void saveSensitivity(value); }, 250);
  };

  const calibrate = async () => {
    setBusy(true);
    setCalibrating(CALIBRATE_S);
    const tick = window.setInterval(() => setCalibrating((s) => Math.max(0, s - 1)), 1000);
    try {
      const r = await fetch(hwUrl("/touch/calibrate"), {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ seconds: CALIBRATE_S, save: true }),
      });
      const body = await r.json();
      if (!r.ok) throw new Error(body.detail ?? `HTTP ${r.status}`);
      apply(body.config);
      toast.success("Touch strip calibrated");
    } catch (e) {
      toast.error(`Calibration failed: ${(e as Error).message}`);
    } finally {
      window.clearInterval(tick);
      setCalibrating(0);
      setBusy(false);
    }
  };

  const reset = async () => {
    setBusy(true);
    try {
      apply(await call("/touch/config/user", { method: "DELETE" }));
      toast.success("Back to the factory calibration");
    } catch (e) {
      toast.error(`Could not reset: ${(e as Error).message}`);
    } finally {
      setBusy(false);
    }
  };

  if (!config) {
    return (
      <SectionCard id="touch" title="Touch pad" active={active}>
        <div style={{ fontSize: 12, color: C.textMuted }}>
          {unavailable ? "This device has no touch strip, or the hardware service is not running." : "Loading…"}
        </div>
      </SectionCard>
    );
  }

  const [min, max] = config.sensitivity_range;
  // Pads in the strip's physical order, so the bars move with your finger.
  const order = config.swipe_axis.length === PADS ? config.swipe_axis : Array.from({ length: PADS }, (_, i) => i);

  return (
    <SectionCard id="touch" title="Touch pad" active={active}>
      <div style={{ fontSize: 12.5, color: C.textDim, marginBottom: 14, lineHeight: 1.6 }}>
        Slide a finger along the touch strip: each bar is one touch point. A bar lights up
        when it passes its line and counts as a touch. If the lamp reacts when nobody touches
        it, move the slider toward <b>Firm</b> or run <b>Auto-calibrate</b>.
      </div>

      <div
        role="img"
        aria-label="Live touch strip signal, one bar per touch point"
        style={{
          display: "grid", gridTemplateColumns: `repeat(${PADS}, 1fr)`, gap: 6,
          padding: "12px 10px 6px", marginBottom: 18, borderRadius: 10,
          background: C.surface, border: `1px solid ${C.border}`,
        }}
      >
        {order.map((pad) => (
          <div key={pad} style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 4 }}>
            <div style={{
              position: "relative", width: "100%", maxWidth: 28, height: BAR_HEIGHT,
              borderRadius: 6, background: "var(--lm-bg)", overflow: "hidden",
              outline: config.warnings.includes(pad) ? `1px solid ${C.yellow}` : undefined,
            }}>
              <div ref={(el) => { fills.current[pad] = el; }} style={{
                position: "absolute", left: 0, right: 0, bottom: 0, height: 0,
                background: "var(--lm-text-muted)", borderRadius: 6, transition: "height 80ms linear",
              }} />
              <div ref={(el) => { touchLines.current[pad] = el; }} title="Touch line" style={{
                position: "absolute", left: 0, right: 0, height: 2, background: C.amber, bottom: "0%",
              }} />
              <div ref={(el) => { releaseLines.current[pad] = el; }} title="Release line" style={{
                position: "absolute", left: 0, right: 0, height: 0, borderTop: `1px dashed ${C.textMuted}`, bottom: "0%",
              }} />
            </div>
            <span ref={(el) => { values.current[pad] = el; }} style={{ fontSize: 10.5, color: C.textMuted, fontFamily: "monospace" }}>–</span>
          </div>
        ))}
      </div>

      <label htmlFor="touch-sensitivity" style={LABEL_STYLE}>Sensitivity</label>
      <input
        id="touch-sensitivity" type="range" min={min} max={max} step={1} value={sensitivity}
        onChange={(e) => onSlider(Number(e.target.value))}
        disabled={calibrating > 0}
        style={SLIDER_STYLE}
      />
      <div style={{ display: "flex", justifyContent: "space-between", fontSize: 11.5, color: C.textMuted, marginBottom: 16 }}>
        <span>Light</span>
        <span style={{ color: C.text }}>{sensitivityLabel(sensitivity, [min, max])}{busy && !calibrating ? " · saving…" : ""}</span>
        <span>Firm</span>
      </div>

      <div style={{ display: "flex", flexWrap: "wrap", gap: 10, marginBottom: 12 }}>
        <button type="button" onClick={() => { void calibrate(); }} disabled={busy}
          style={{ padding: "8px 14px", borderRadius: 8, cursor: busy ? "default" : "pointer",
                   background: C.amberDim, border: `1px solid ${C.amber}`, color: C.amber, fontSize: 13 }}>
          {calibrating > 0 ? `Hands off… ${calibrating}s` : "Auto-calibrate"}
        </button>
        <button type="button" onClick={() => { void reset(); }} disabled={busy || !config.user_adjusted}
          style={{ padding: "8px 14px", borderRadius: 8, cursor: busy || !config.user_adjusted ? "default" : "pointer",
                   background: C.surface, border: `1px solid ${C.border}`, color: config.user_adjusted ? C.text : C.textMuted, fontSize: 13 }}>
          Reset to factory
        </button>
        <button type="button" onClick={() => setAdvanced((a) => !a)}
          style={{ padding: "8px 14px", borderRadius: 8, cursor: "pointer", background: "transparent",
                   border: "none", color: C.textMuted, fontSize: 12.5 }}>
          {advanced ? "Hide details" : "Details"}
        </button>
      </div>
      {calibrating > 0 && (
        <div role="status" style={{ fontSize: 12.5, color: C.amber, marginBottom: 12 }}>
          Measuring the strip at rest. Keep hands and objects off it for {calibrating} more seconds.
        </div>
      )}

      {config.warnings.length > 0 && (
        <div role="status" style={{
          marginBottom: 12, padding: "9px 12px", borderRadius: 8, fontSize: 12.5, lineHeight: 1.5,
          background: "var(--lm-yellow-dim)", border: `1px solid ${C.yellow}`, color: C.text,
        }}>
          Touch point{config.warnings.length > 1 ? "s" : ""} {config.warnings.join(", ")} sit{config.warnings.length > 1 ? "" : "s"} close
          to the strip's resting noise and may trigger on their own. Move the slider toward Firm.
        </div>
      )}

      {advanced && (
        <table style={{ width: "100%", fontSize: 12, color: C.textDim, borderCollapse: "collapse", fontFamily: "monospace" }}>
          <thead>
            <tr style={{ color: C.textMuted, textAlign: "left" }}>
              <th style={{ fontWeight: 500 }}>Point</th>
              {order.map((pad) => <th key={pad} style={{ fontWeight: 500 }}>{pad}</th>)}
            </tr>
          </thead>
          <tbody>
            <tr><td>Touch</td>{order.map((pad) => <td key={pad}>{config.touch[pad]}</td>)}</tr>
            <tr><td>Release</td>{order.map((pad) => <td key={pad}>{config.release[pad]}</td>)}</tr>
            {config.noise && <tr><td>Noise</td>{order.map((pad) => <td key={pad}>{config.noise?.[pad]}</td>)}</tr>}
          </tbody>
        </table>
      )}
    </SectionCard>
  );
}
