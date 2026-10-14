"""What is said: the phrases that mean stop, and those that only sound near them.

A phrase that asks the lamp to stop counts if it holds one of the three
targets: "stop", "wait" or "hold on". "Stop it" counts: it is said to make
something stop. The near misses are said in ordinary talk and must not set
the listener off.
"""

TARGETS = ("stop", "wait", "hold on")

# Phrases said to make the lamp stop, by target. The target's own word(s)
# are found in each by alignment, and its end is what the listener answers to.
ASKING = {
    "stop": [
        "Stop.", "Stop!", "Stop, stop.", "Stop it.", "Stop, please.", "Please stop.",
        "Okay, stop.", "No, stop.", "Lamp, stop.", "Stop talking.", "Hey, stop.",
        "Stop right there.", "Stop that.", "Just stop.", "Stop now.", "Okay okay, stop.",
    ],
    "wait": [
        "Wait.", "Wait!", "Wait, wait.", "Wait a second.", "Wait a minute.", "Wait, what?",
        "Lamp, wait.", "No, wait.", "Wait, no.", "Okay, wait.", "Wait, please.", "Hey, wait.",
        "Wait up.", "Just wait.", "Wait, wait, wait.", "Oh wait.",
    ],
    "hold on": [
        "Hold on.", "Hold on!", "Hold on a second.", "Hold on, hold on.", "Hold on a minute.",
        "Lamp, hold on.", "Okay, hold on.", "Hold on, please.", "Wait, hold on.", "No, hold on.",
        "Hey, hold on.", "Hold on, wait.", "Hold on one sec.", "Oh, hold on.",
    ],
}

# Ordinary talk that comes near the targets, by what it is near. None of it
# asks the lamp to stop.
NEAR = {
    "don't stop": [
        "Don't stop.", "Don't stop now.", "Please don't stop.", "Don't stop the music.",
        "No, don't stop, keep going.", "Don't stop there.", "Don't stop me now.",
    ],
    "stopped": [
        "We stopped at the store.", "It stopped raining.", "The clock stopped.",
        "She stopped by yesterday.", "The car stopped suddenly.", "Has it stopped yet?",
        "He stopped.",
    ],
    "stopping / stops": [
        "There's no stopping him.", "The train stops here.", "It never stops.",
        "We're stopping for lunch.", "The music stops at ten.",
    ],
    "bus stop": [
        "The bus stop is close.", "Meet me at the bus stop.", "I waited at the bus stop.",
        "It's the next bus stop.", "Where is the bus stop?",
    ],
    "stop in a sentence": [
        "It's a non-stop flight.", "There's a stop sign ahead.", "Can you stop by the shop later?",
        "The rain will stop soon.", "I need to stop eating sugar.", "Put a full stop at the end.",
        "We should stop and think about it.",
    ],
    "wait in a sentence": [
        "I can't wait to see you.", "We had to wait in line.", "It was worth the wait.",
        "What's the wait time?", "I'll wait for you outside.", "Wait times are long today.",
        "You'll have to wait your turn.", "Let's wait and see.",
    ],
    "waiting / waited": [
        "I'm waiting for you.", "We waited an hour.", "Are you still waiting?",
        "She waited by the door.", "Thanks for waiting.",
    ],
    "hold": [
        "Hold the door.", "I'm on hold.", "Hold this for me.", "Can you hold my bag?",
        "We were on hold for an hour.", "Hold it steady.", "Get a hold of yourself.",
    ],
    "sounds like": [
        "Top.", "Shop.", "Stomp.", "Mop the floor.", "Stuff.", "Weight.", "Gate.", "Way.",
        "Old one.", "Gold.", "Hello.", "What?", "Okay.", "Stuck.", "Stock.", "Spot.",
        "Hot dog.", "Rolled on.", "Told on.", "Late.", "Eight.", "Bait.", "Waiter.",
    ],
}

# Short replies a person gives while listening: not a stop.
LISTENING = ["Mm-hm.", "Yeah.", "Uh-huh.", "Right.", "Okay.", "I see.", "Sure.", "Yes.", "No."]


def has_target(text: str) -> bool:
    """Whether a line of text holds one of the targets as words (for keeping them out of talk)."""
    words = " " + " ".join("".join(c if c.isalpha() or c == "'" else " " for c in text.lower()).split()) + " "
    return " stop " in words or " wait " in words or " hold on " in words


def near_words(text: str) -> list[str]:
    """Which near misses a line of text holds."""
    words = " " + " ".join("".join(c if c.isalpha() or c == "'" else " " for c in text.lower()).split()) + " "
    found = []
    for key, needle in [
        ("don't stop", " don't stop "),
        ("stopped", " stopped "),
        ("stopping / stops", " stopping "),
        ("stopping / stops", " stops "),
        ("bus stop", " bus stop "),
        ("waiting / waited", " waiting "),
        ("waiting / waited", " waited "),
        ("hold", " hold "),
    ]:
        if needle in words and key not in found:
            found.append(key)
    return found
