// ESP32-C3 SuperMini -> WS2812 bridge for Orange Pi 4 Pro (lamp-0c89).
//
// The OPi's SPI3 MOSI cannot drive the WS2812 through the long cable; the C3's
// output can. So the OPi sends pixels as UART frames and the C3 regenerates the
// WS2812 waveform locally, driving the cable itself.
//
// Link:  OPi pin 8 (TXD.7 / PL6, /dev/ttyS7) -> C3 GPIO20 (RX).  3.3V both sides,
//        direct connect, common ground required (the USB cable to the OPi supplies it).
//
//        BAUD MUST BE 500000, and that is not negotiable -- see below. Serial0 is
//        used because GPIO20/21 are UART0's native IOMUX pins; it is proven working.
//
// BAUD:  The OPi's UART7 is s_uart0 in the AON domain, clocked from the 24 MHz
//        oscillator, so the only reachable rates are 24e6/16/n = 1500000/n for
//        integer n: 1500000, 750000, 500000, 375000, 300000, 250000 ...
//        Ask it for 460800 and n rounds 3.255 -> 3, so the wire actually runs at
//        500000 -- an 8.5% error, far past UART tolerance. The symptom is brutal to
//        diagnose: the pin toggles (a raw digitalRead counts hundreds of edges) but
//        no UART frames a single byte, so you get zero received AND zero errors.
//        115200 survives only by luck (1500000/13 = 115384, +0.16%).
//        Measured on the wire, not assumed: a baud scan decoded valid Adalight
//        headers at 500000 and at no other rate.
// LEDs:  C3 GPIO2 (silkscreen A2) -> WS2812 DIN, through the long cable.
// Debug: USB CDC at 115200 -> live frame/resync counters. Build with
//        CDCOnBoot=cdc, which also moves `Serial` off UART0 so it cannot
//        collide with the link on GPIO20/21.
//
// Protocol: Adalight. Header "Ada" + hi + lo + chk, where (hi<<8|lo) == count-1
// and chk == hi^lo^0x55, then count*3 bytes of R,G,B. The magic word makes the
// stream self-resynchronising: a corrupted frame costs one frame, not the link.
//
// GPIO2 IS A STRAPPING PIN on the C3 and must read HIGH at reset. A WS2812 DIN is
// high-Z so this normally boots fine, but if the board won't start with the strip
// attached, that is why -- add a 10k pullup to 3V3, or move LED_PIN to GPIO3 (A3).

#include <Adafruit_NeoPixel.h>

static const uint8_t  LED_PIN   = 2;        // A2 on the SuperMini silkscreen
static const uint16_t LED_COUNT = 32;       // must match the OPi sender
static const uint8_t  RX_PIN    = 20;       // from OPi pin 8
static const uint8_t  TX_PIN    = 21;       // to OPi pin 10 (unused by the protocol)
static const uint32_t BAUD      = 500000;   // NOT 460800 -- OPi uart7 cannot generate it

// Blank the strip if the OPi goes quiet, so a crashed sender doesn't freeze a
// colour on forever.
static const uint32_t LINK_TIMEOUT_MS = 2000;
static const uint32_t REPORT_MS       = 1000;

Adafruit_NeoPixel strip(LED_COUNT, LED_PIN, NEO_GRB + NEO_KHZ800);

static uint8_t  frame[LED_COUNT * 3];
static uint32_t lastFrameMs = 0;
static bool     blanked     = false;

// Counters, reported over USB once a second. frames climbing at the sender's fps
// with resync/badhdr flat means the link is clean; resync climbing means the wire
// is marginal or the baud is off.
static uint32_t nFrames = 0, nResync = 0, nBadHdr = 0, nShort = 0;
static uint32_t lastReportMs = 0;

// Block until n bytes arrive or the deadline passes. Returns false on timeout,
// which drops us back to hunting for the magic word rather than parsing garbage.
static bool readExact(uint8_t *dst, size_t n, uint32_t deadlineMs) {
  size_t got = 0;
  while (got < n) {
    if ((int32_t)(millis() - deadlineMs) >= 0) return false;
    int c = Serial0.read();
    if (c < 0) continue;
    dst[got++] = (uint8_t)c;
  }
  return true;
}

void setup() {
  Serial.begin(115200);                                   // USB CDC (debug only)
  Serial0.begin(BAUD, SERIAL_8N1, RX_PIN, TX_PIN);        // link from the OPi
  strip.begin();
  strip.setBrightness(255);   // OPi already clamps brightness; don't scale twice
  strip.clear();
  strip.show();

  // Power-on proof of life: red, green, blue, off. If you see this at reset, the
  // C3 -> strip path (GPIO2, the cable, the strip's power) is good, independent
  // of whether the OPi link works at all.
  const uint32_t probe[3] = {strip.Color(64, 0, 0), strip.Color(0, 64, 0), strip.Color(0, 0, 64)};
  for (int i = 0; i < 3; i++) {
    strip.fill(probe[i]);
    strip.show();
    delay(250);
  }
  strip.clear();
  strip.show();

  Serial.println("# esp32c3-ws2812 ready");
  Serial.printf("# LED_PIN=%u count=%u  link=GPIO%u@%lu 8N1\n",
                LED_PIN, LED_COUNT, RX_PIN, (unsigned long)BAUD);
}

void loop() {
  uint32_t now = millis();
  if (now - lastReportMs >= REPORT_MS) {
    lastReportMs = now;
    Serial.printf("frames=%lu resync=%lu badhdr=%lu short=%lu link=%s\n",
                  (unsigned long)nFrames, (unsigned long)nResync,
                  (unsigned long)nBadHdr, (unsigned long)nShort,
                  (now - lastFrameMs < LINK_TIMEOUT_MS) ? "UP" : "down");
  }

  // Hunt for "Ada". Anything that isn't the magic word is discarded a byte at a
  // time, so we resynchronise naturally after a partial or corrupted frame.
  static uint8_t magic = 0;
  int c = Serial0.read();
  if (c < 0) {
    if (!blanked && now - lastFrameMs > LINK_TIMEOUT_MS) {
      strip.clear();
      strip.show();
      blanked = true;
    }
    return;
  }

  const char want[3] = {'A', 'd', 'a'};
  if ((uint8_t)c == (uint8_t)want[magic]) {
    if (++magic < 3) return;
    magic = 0;
  } else {
    if (magic) nResync++;                 // we were mid-magic and lost it
    magic = ((uint8_t)c == 'A') ? 1 : 0;  // this byte may itself start a match
    return;
  }

  uint8_t hdr[3];
  if (!readExact(hdr, 3, millis() + 100)) { nShort++; return; }
  if ((uint8_t)(hdr[0] ^ hdr[1] ^ 0x55) != hdr[2]) { nBadHdr++; return; }

  uint16_t count = ((uint16_t)hdr[0] << 8 | hdr[1]) + 1;
  if (count != LED_COUNT) { nBadHdr++; return; }

  if (!readExact(frame, sizeof(frame), millis() + 200)) { nShort++; return; }

  for (uint16_t i = 0; i < LED_COUNT; i++) {
    strip.setPixelColor(i, frame[i * 3], frame[i * 3 + 1], frame[i * 3 + 2]);
  }
  strip.show();
  nFrames++;
  lastFrameMs = millis();
  blanked = false;
}
