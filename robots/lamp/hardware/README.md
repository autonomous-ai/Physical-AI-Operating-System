# Hardware

Designs, schematics, and specifications for the Lamp hardware.

<table>
  <tr>
    <td><img src="images/core.jpg" alt="Core: OrangePi 4 Pro, Waveshare bus servo adapter, STS3215 servo"></td>
    <td><img src="images/head.jpg" alt="Head: WS2812 LED ring, USB camera, 4× TTP223 touch sensors around the rim"></td>
  </tr>
</table>

This folder is owned by the hardware team. Upload here:

- Mechanical designs (CAD, STEP, STL, renders)
- PCB schematics and board files (KiCad, Altium, Gerbers)
- BOMs and assembly drawings
- Datasheets for non-standard components
- Wiring diagrams and pinouts
- Enclosure and assembly photos

Bench / factory hardware test (camera, mics, speaker, MPR121 + per-unit calibration, buttons,
TTP223, servos), run from a test hub over each lamp's setup hotspot: [`test/`](test/README.md).

Large binary CAD files are stored via **Git LFS** (see `.gitattributes`).
File list and revision notes live in [`cad/README.md`](cad/README.md).
