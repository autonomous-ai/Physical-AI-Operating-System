# Brain research notes

These notes are the research behind [the brain design](../brain.md), gathered on
12 October 2026. Each was produced by a code-reading or web-research pass. Each
records the claims re-read by hand for the design; treat every other claim as
a pointer to check.

| File | Question it answers | Sources |
| --- | --- | --- |
| [lampos-integration.md](lampos-integration.md) | Where a brain plugs into today's lampOS core, what it can observe, what it can gate, and what happens at the Gemini usage limit | `lamp-v2-chat` at `10607a439` |
| [economy-protocols.md](economy-protocols.md) | The exact wire protocols V1 uses for the Deepgram relay, Gemini speech synthesis, Gemini Live through the Autonomous proxy and the Hermes API, and how each service signals a limit | V1 `main` at `170a1b85e` |
| [gemini-live-tools-tts.md](gemini-live-tools-tts.md) | Google's documented facts for Live tool calling, device-initiated speech, session limits, Gemini speech synthesis and prices | Google AI and Vertex AI docs, read 12 October 2026 |

The V1 behavior study is separate: [V1 brain baseline](../v1-brain-baseline.md)
and its [working notes](../v1-baseline/README.md).
