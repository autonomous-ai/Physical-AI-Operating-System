# Deterministic simulation fixtures (not speech): writes ask.wav and reply.wav
# into the directory given as the only argument. Expected SHA-256:
#   ask.wav   529ce6c432482b7b216be2b95a866c97445ccc143252098be5c62b6a8919815c
#   reply.wav d9afb80e886e898c7230abc443c60dc4d229b1598f74571e021bc54714132421
import math, os, struct, sys, wave

out = sys.argv[1]
# 1.2 s "question": 200 Hz tone with an 800-sample envelope at 16 kHz.
w = wave.open(os.path.join(out, "ask.wav"), "wb")
w.setnchannels(1); w.setsampwidth(2); w.setframerate(16000)
n = int(16000 * 1.2)
w.writeframes(b"".join(struct.pack("<h", int(9000 * math.sin(2 * math.pi * 200 * i / 16000) * min(1, i / 800, (n - i) / 800))) for i in range(n)))
w.close()
# 2 s fixed reply: 330 Hz tone at 24 kHz (lamp-live's render rate).
w = wave.open(os.path.join(out, "reply.wav"), "wb")
w.setnchannels(1); w.setsampwidth(2); w.setframerate(24000)
w.writeframes(b"".join(struct.pack("<h", int(8000 * math.sin(2 * math.pi * 330 * i / 24000))) for i in range(24000 * 2)))
w.close()
