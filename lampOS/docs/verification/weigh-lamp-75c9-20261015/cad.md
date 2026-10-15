# The design file, as a check on lamp-75c9's readings

Given by the eighth builder's person at 17:31: `Lamp Final Oct82026 v2.step`
(SHA-256 `6cf17da1b09c7a8b4291a255a86aa3b899fe773cda063a3678d69062729a9517`,
written by Autodesk on 2026-10-15). It is the maker's design, not lamp-75c9
measured, and it is not kept in this repository; only the figures below are
taken from it. Read with OpenCascade in a scratch folder (a script that lists
each solid's volume, centre of volume and bounding box); lampOS has no Python.

- Its materials are placeholders: 77 bodies `Generic` at 1,000 kg/m3, three
  `Steel` at 7,850 kg/m3. So it gives no part's mass.
- The base turns about its centre: the swivel bearing's centre is at
  (3021.0, 549.5) mm in the file's frame, and the base shell's square at
  (3021.05, 549.45).
- The head and its light face +x, toward a flat side of the square, as the
  person said.
- The base shell is a 184.9 mm square and the plate under it 174.6 mm; the
  person read 181 mm (first 174 mm). The feet are not in the file, so the
  person's 161 mm across them and 4.5 mm radius stand.
