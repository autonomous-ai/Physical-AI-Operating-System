"""What is said: the three ways of calling the lamp, and what comes near them.

A text in [[ ]] is given to the voice as phonemes (espeak's IPA), for the
Vietnamese "ơi" (/ə:j/) and the Vietnamese names, which an English voice
cannot read from letters. The name is said at the start of the line, as a
person calls; what follows is one of the questions.
"""

# The ways of calling, each with how its start is written.
CALLS = {
    "hey": ["Hey Lamp", "Hey, Lamp", "Hey Lamp"],
    "oi": ["[[lˈæmp ˈəːj]]", "[[lˈæmp ˈəːj]]", "[[lˈɛmp ˈəːj]]", "[[lˈaːm ˈəːj]]"],
    "bare": ["Lamp", "Lamp", "Lamp"],
}

QUESTIONS = [
    "what time is it?", "what's the weather like today?", "can you help me?", "are you there?",
    "tell me a joke.", "what do you think?", "I need your help with something.", "how are you?",
    "what's on my calendar?", "remind me at three.", "play some music.", "look at this.",
    "turn the light down a little.", "can you hear me?", "what should I have for lunch?",
    "how do you say thank you in Vietnamese?", "set a timer for ten minutes.", "what's two plus two?",
    "we could go to the beach this weekend, what do you think?", "I'm back.", "good morning.",
    "is it going to rain?", "can you be quieter?", "what was I doing?", "say something.",
    "how long until five?", "do you know where my keys are?", "help me focus.", "I'm tired.",
    "what's new?", "brighter please.", "which one looks better?", "are you awake?",
]

# Lines that end with the call: said alone, or with a question after.
ENDINGS = [".", "?", "!", ","]

# Near misses, by kind: the same place in the sentence, another word.
NEAR = {
    "Lan": ["Lan", "Hey Lan", "[[lˈaːn ˈəːj]]"],
    "Liam": ["Liam", "Hey Liam"],
    "lamb": ["Lamb", "Lamb chops"],
    "land": ["Land", "Land ahead"],
    "plan": ["Plan", "Plan B"],
    "lab": ["Lab", "Lab results"],
    "Sam": ["Sam", "Hey Sam"],
    "ma'am": ["Ma'am", "Excuse me ma'am"],
    "Lâm": ["[[lˈəm]]", "[[lˈəm ˈəːj]]", "Hey [[lˈəm]]"],
    "other names": [
        "Hey man", "Hey Mom", "Hey Lance", "Pam", "Hey Pam", "Nam", "[[nˈaːm ˈəːj]]", "[[ˈɛm ˈəːj]]",
        "Hey Lamar", "Lars", "Lana", "Hey Lena",
    ],
    "rhymes": ["Camp", "Damp", "Ramp", "Stamp", "Clamp", "Lap", "Lamps", "Lump", "Limp"],
    "lamp in a sentence": [
        "Turn on the lamp", "This lamp is too bright", "I bought a new lamp", "The lamp is on my desk",
        "Can you move the lamp a bit", "Where did you get that lamp", "My old lamp broke", "Is the lamp still on",
        "Put the lamp over there", "That lamp looks nice",
    ],
}

# The lamp's own voice: the kind of thing it says. Some say its name.
REPLIES = [
    "Sure, it's a quarter past three.", "Hmm, maybe something spicy like a good bowl of pho?",
    "I'm Lamp, I sit on your desk and keep you company.", "I can dim myself a little if that helps.",
    "That sounds like a lovely plan for the weekend.", "Let me think about that for a second.",
    "Good morning! You look ready for the day.", "I'm not sure, but I can find out.",
    "It might rain later, so take an umbrella.", "Ten minutes, starting now.",
    "Two plus two is four, easy one.", "You've been at it a while, maybe stretch a bit?",
    "I don't have eyes on your keys, sorry.", "As a lamp, I can't eat lunch, but I can recommend it.",
    "That one looks better to me, the blue one.", "I'll remind you at three o'clock.",
    "Here's a joke: why did the lamp go to school? To get brighter.", "I'm right here.",
    "Lamp here, what can I do for you?", "Okay, I'll keep quiet for a while.",
    "Your next meeting is at four.", "Thank you in Vietnamese is cam on.",
    "It's a bit late, maybe time to wrap up.", "I hear you loud and clear.",
    "Nice to see you back.", "I can turn the light warmer in the evening.",
]
