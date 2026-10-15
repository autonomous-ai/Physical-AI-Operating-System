# What lamp-4ace's eyes published, 2026-10-14 18:23 to 2026-10-15 08:46

For #763 and #766 (a face that never moves). Numbers only: no picture, no name.

- Source: `lamp-bus record /var/tmp/day-20261014 … humans/faces/tracked`
  on lamp-4ace (`humans_faces_tracked.jsonl`, 21.5 MB), every message the
  eyes published while the camera was open.
- `faces-day-20261014.csv.gz`: one row per face per message, and one row
  with empty fields for a message with no face. Columns: `at_unix_ms`,
  `face_id` (the eyes' passing id), `xmin`, `ymin`, `xmax`, `ymax` (shares
  of the picture), `confidence`, `facing` (1, 0 or empty). 119,323 rows.
- What it shows (first builder's count): in the empty office from 01:00 to
  05:59, 54 of 55 faces were at one spot (centre 0.7 across, 0.6 down,
  0.10 to 0.12 of the height, confidence 0.60 to 0.79). From 18:00 to
  20:59, 203 of 295 detections were there too. The real people were at the
  picture's edge (centre 0.9 to 1.0 across). No face reached 0.14 of the
  height in the fifteen hours.
