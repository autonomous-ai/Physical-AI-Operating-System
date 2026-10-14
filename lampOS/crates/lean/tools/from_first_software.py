# Turn the first lamp software's body description into the lean check's input file, then compare the two on the same poses.
# Run from the repository root with the lean check's example built (docs/lean.md, "lamp-4ace, 14 October"):
#   python3.12 -I lampOS/crates/lean/tools/from_first_software.py . lampOS/deploy/lamp-4ace/lean-first-software.json \
#     "$CARGO_TARGET_DIR/release/examples/check"
# Standard library only; it imports the first software's own tipping function from hal/ to compare against it.
import importlib.util, json, math, random, subprocess, sys, xml.etree.ElementTree as ET
wt, out, check = sys.argv[1:4]
urdf_path = wt + "/robots/lamp/urdf/lamp.urdf"
spec = importlib.util.spec_from_file_location("v1stab", wt + "/hal/drivers/motors/recording_stability.py")
v1 = importlib.util.module_from_spec(spec); sys.modules["v1stab"] = v1; spec.loader.exec_module(v1)
geo = v1.parse_urdf(open(urdf_path).read())
# The first software's motor calibration as kept on lamp-4ace (dated 2026-08-21): zero degrees is the middle of each range.
cal = {"base_yaw": (847, 3275), "base_pitch": (0, 4095), "elbow_pitch": (0, 4095), "wrist_roll": (621, 3254), "wrist_pitch": (1464, 3071)}
name = {"base_yaw": "base_yaw", "base_pitch": "shoulder", "elbow_pitch": "elbow", "wrist_pitch": "head_pitch", "wrist_roll": "head_roll"}
mid = {j: (a + b) / 2 for j, (a, b) in cal.items()}
R = 0.1; N = 32
links = []
for l in geo.chain:
    links.append({"joint": name[l.joint], "origin_m": list(l.offset), "rpy_rad": list(l.rpy), "axis": list(l.axis),
                  "angle_zero_rad": round((mid[l.joint] - 2048) * math.tau / 4096, 9), "mass_kg": l.mass, "com_m": [0.0, 0.0, 0.0]})
g = {"schema_version": 1, "measurement": "stand_in",
     "label": "First lamp software's model (robots/lamp/urdf/lamp.urdf): joints from the maker's drawings; masses ESTIMATED, 1.49 kg against 7 kg on the spec; footprint a circle inside the spec's 200 mm square. Nothing weighed. Qualifies no motion. See docs/lean.md.",
     "footprint_m": [[round(R * math.cos(i * math.tau / N), 9) + 0.0, round(R * math.sin(i * math.tau / N), 9) + 0.0] for i in range(N)],
     "margin_m": 0.0, "base": {"mass_kg": geo.root_mass, "com_m": [0.0, 0.0, 0.0]}, "links": links}
text = "{\n" + ",\n".join(
    (f'  "{k}": [\n' + ",\n".join("    " + json.dumps(x) for x in v) + "\n  ]") if k in ("links",) else f'  "{k}": {json.dumps(v)}'
    for k, v in g.items()) + "\n}\n"
open(out, "w").write(text); json.loads(text)
# Compare: this check's centre of mass against the first software's own function, same motor readings.
order = ["base_yaw", "base_pitch", "elbow_pitch", "wrist_roll", "wrist_pitch"]  # motor ids 1..5
random.seed(4)
poses = [("rest, read 11:08", [2023, 2052, 2047, 2053, 1950]), ("standing, read 11:06", [2023, 2558, 2503, 2053, 1820])]
poses += [(f"random {i}", [random.randint(max(a, 1024), min(b, 3072)) for a, b in (cal[j] for j in order)]) for i in range(40)]
worst = 0.0; rows = []
for label, counts in poses:
    frame = {j: (c - mid[j]) * 360 / 4095 for j, c in zip(order, counts)}
    v1mm = v1.cog_offset_mm(frame, geo)
    r = subprocess.run([check, out, wt + "/lampOS/crates/expression/behaviours.json", ",".join(map(str, counts))], capture_output=True, text=True)
    d = json.loads(r.stdout); c = d["baseline"]["centre_of_mass_m"]; mm = math.hypot(c[0], c[1]) * 1000
    worst = max(worst, abs(mm - v1mm)); rows.append((label, counts, v1mm, mm, d["baseline"]["clearance_m"] * 1000, r.returncode, d.get("model_within_margin_all_checks")))
for row in rows[:8]: print("%-22s %s first %.3f mm  lean %.3f mm  clearance %+.2f mm  exit %d all16 %s" % row)
print("poses", len(rows), "largest difference mm %.4f" % worst)
print("outside the limit:", sum(1 for r in rows if r[4] <= 0), "of", len(rows))

# Scored as the first software ran on its lamps: its motors in -100..100 of each calibrated range, read by its tipping check as degrees.
def as_run(counts):
    return v1.cog_offset_mm({j: (c - cal[j][0]) / (cal[j][1] - cal[j][0]) * 200 - 100 for j, c in zip(order, counts)}, geo)
for label, counts in poses[:2]:
    print("as the first software scored it: %-22s %.1f mm (its limit 22, the clip that tipped 31.6, its worst shipped 17.7)" % (label, as_run(counts)))
