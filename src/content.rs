//! Reading passages and training guides. All text written for OnicVoco.

pub struct Passage {
    pub title: &'static str,
    pub focus: &'static str,
    pub text: &'static str,
}

pub const PASSAGES: &[Passage] = &[
    Passage {
        title: "Morning market",
        focus: "Everyday speech. Keep it light and conversational.",
        text: "Every Saturday I walk down to the little market by the river. The baker always waves before I even reach her stall, and the man who sells honey insists I try a new jar each week. Today it was lavender, which tasted a bit like soap, but I bought it anyway because he looked so hopeful. By the time I got home my bag was full of bread, pears and far too many flowers.",
    },
    Passage {
        title: "Questions and answers",
        focus: "Intonation. Let questions rise and statements settle.",
        text: "Are you coming tonight? I think so. Do you know what time it starts? Around eight, I believe. Should I bring anything? Maybe something small to share. Is it far from the station? Not at all, it's just around the corner. Will you be there early? I'll try my best, but no promises!",
    },
    Passage {
        title: "The lighthouse",
        focus: "Long phrases. Keep resonance bright to the end of each sentence.",
        text: "On the far edge of the island there is a lighthouse that nobody has climbed in years. The stairs wind around and around, narrower with every turn, until they open onto a small round room full of salt and silence. From the window you can see the whole coastline curling away into the haze, and on clear evenings, the lights of a town that feels impossibly far away.",
    },
    Passage {
        title: "Phone call",
        focus: "Smooth starts. Begin each sentence gently, without a hard attack.",
        text: "Hi, it's me. I'm just calling to let you know I'll be a few minutes late. The bus decided to take the scenic route again. Could you save me a seat near the window? Oh, and if they have the lemon cake, please get me a slice. I'll pay you back, I promise. See you soon!",
    },
    Passage {
        title: "Numbers and names",
        focus: "Steady pitch on short words. Avoid dropping at the ends.",
        text: "My appointment is on Tuesday the twelfth at half past four. The address is twenty-seven Willow Lane, flat number nine. Please ask for Morgan or Elise at the front desk. If nobody answers, try the side door, the one with the blue frame. The phone number is on the card I gave you last week.",
    },
    Passage {
        title: "Excited story",
        focus: "Pitch variety. Let it move up and down with the meaning.",
        text: "You will not believe what happened! I was halfway up the hill when this tiny dog came running out of nowhere, carrying an entire baguette. Its owner was chasing it, shouting, laughing, completely out of breath. The dog stopped right in front of me, dropped the bread at my feet, and sat down like it was waiting for applause. So of course, I clapped.",
    },
];

/// Short lines for quick reps, like warming up with phrases.
pub const QUICK_PHRASES: &[&str] = &[
    "Hello, how are you today?",
    "Thank you so much, that's really kind.",
    "Could I get a medium coffee, please?",
    "Oh no, I'm so sorry about that!",
    "Wait, are you serious right now?",
    "I'll see you on Thursday, then.",
    "Mm-hmm, yeah, that makes sense.",
    "My name is... and I'm calling about my order.",
];

pub struct Guide {
    pub title: &'static str,
    pub summary: &'static str,
    pub body: &'static [&'static str],
    pub exercises: &'static [(&'static str, &'static str)],
}

pub const GUIDES: &[Guide] = &[
    Guide {
        title: "Chest voice",
        summary: "Your everyday, full, buzzy speaking voice.",
        body: &[
            "In chest voice the vocal folds are thicker and stay closed for longer in each vibration. That gives a rich sound with lots of upper harmonics, which you feel as a buzz in your chest when you speak low.",
            "Chest voice carries well and feels natural, but it is also \"heavy\". Pushing heavy chest voice higher is what makes a voice strain and crack.",
            "If you are training toward a lighter or more feminine voice, the goal is usually not to abandon chest voice, but to make it lighter and thinner (less vocal weight).",
        ],
        exercises: &[
            ("Chest check", "Put a hand on your upper chest and say \"uh-huh\" low and relaxed. Feel the buzz. That's chest resonance."),
            ("Weight dial", "Say \"hello\" five times, each time softer and thinner, as if you're talking to a sleeping cat. Notice the buzz fading while the pitch stays similar."),
        ],
    },
    Guide {
        title: "Head voice and falsetto",
        summary: "The lighter, higher registers above your chest voice.",
        body: &[
            "In head voice and falsetto the vocal folds are stretched thin and close only briefly (or not fully). The sound is lighter and often airier, with a strong fundamental and weaker upper harmonics.",
            "Falsetto is the airiest version: the folds barely meet, so it sounds breathy and floaty. Head voice is more connected and clear.",
            "Neither is \"wrong\". For speech, pure falsetto usually sounds too breathy and gets tiring, so many people aim for a connected head voice or a light mix instead.",
        ],
        exercises: &[
            ("Owl hoots", "Say \"hoo\" like a gentle owl, high and light. No pushing. This finds your head voice."),
            ("Whimper", "Make a soft puppy whimper on \"mm\". It is one of the easiest ways to feel a light, connected high sound."),
        ],
    },
    Guide {
        title: "Passaggio",
        summary: "The transition zone where your voice changes gear.",
        body: &[
            "The passaggio (Italian for \"passage\") is the pitch area where your voice moves from chest toward head voice. It is not one single note, more a small zone of a few semitones.",
            "For many voices the first passaggio sits somewhere around E4 to G4, but it varies a lot from person to person. It is also where voice cracks happen most.",
            "Knowing where yours is helps you plan: you can speak comfortably below it, or learn to blend smoothly through it. Use \"Find my passaggio\" on the Practice tab to measure it.",
        ],
        exercises: &[
            ("Siren", "On \"oo\" or \"ng\", glide slowly from your lowest comfortable note to your highest and back, quietly. Notice where it wants to flip. That's the zone to practise."),
            ("Small slides", "Pick the note just below your break and slide up three notes and back on \"ng\". Repeat a few times, keeping the volume low."),
        ],
    },
    Guide {
        title: "Mix voice",
        summary: "A blend of chest and head that stays smooth across the break.",
        body: &[
            "Mix is not a separate register so much as a balance: some of the clarity of chest voice with the lightness of head voice. It lets you move through the passaggio without a sudden flip.",
            "Mix is built with quiet, gentle practice. Loudness makes it harder because it pulls you back into heavy chest voice.",
        ],
        exercises: &[
            ("Ng sirens", "Hum \"ng\" (as in \"sing\") and slide up and down across your passaggio. The narrow \"ng\" shape makes it easier to keep the sound connected."),
            ("Bratty \"nay\"", "Say \"nay nay nay\" in a slightly whiny, bratty tone on a rising pattern. The twangy quality helps chest and head meet."),
        ],
    },
    Guide {
        title: "Resonance",
        summary: "How bright or dark the voice sounds. Often more important than pitch.",
        body: &[
            "Resonance is shaped by the size of the space above your vocal folds: your throat and mouth. A smaller space (larynx a bit higher, tongue forward) sounds brighter. A larger space sounds darker.",
            "Two voices at the same pitch can be perceived very differently because of resonance. That's why OnicVoco shows it separately from pitch.",
        ],
        exercises: &[
            ("Big dog, small dog", "Pant like a big dog (deep, open, dark), then like a tiny dog (quick, high, bright). Feel your larynx move up for the small dog. Aim to speak with a bit of that small-dog space."),
            ("Whisper siren", "Whisper \"ee\" and slide it from low to high. The whisper lets you hear the resonance change without any pitch."),
        ],
    },
    Guide {
        title: "Voice cracks",
        summary: "Why the voice breaks, and how to make it smoother.",
        body: &[
            "A crack happens when your vocal folds switch suddenly from thick (chest) to thin (head) vibration, or back. It is completely normal and happens to every voice, especially near the passaggio.",
            "Cracks get worse with too much volume, too much air pressure, a tight throat or jaw, tiredness and dehydration.",
            "OnicVoco marks cracks on the pitch graph so you can see where they happen. If they keep appearing at the same note, that's your practice zone.",
        ],
        exercises: &[
            ("Get quieter near the break", "As you approach the cracking note, reduce volume instead of pushing. Loudness forces the heavy chest setting to hold on too long."),
            ("Lighten early", "Start thinning the voice a few notes before the break, so there's less to switch over."),
            ("Lip trills and straw", "Do lip trills, or hum through a drinking straw, while sliding across the break. They balance air pressure and make transitions smoother."),
            ("Relax the setup", "Keep your jaw loose, tongue relaxed, shoulders down. Tension above the larynx makes flips more abrupt."),
            ("Rest and water", "A tired, dry voice cracks more. Short sessions, sips of water, and stopping when it feels scratchy all help."),
        ],
    },
    Guide {
        title: "Practice safely",
        summary: "Small, regular sessions beat long, hard ones.",
        body: &[
            "Aim for 10 to 20 minutes a day rather than an hour once a week. Voice changes come from repetition.",
            "Never push through pain, tickling or hoarseness. Discomfort means stop, rest and drink some water.",
            "Warm up first with humming, lip trills or gentle sirens. Cool down by speaking softly in your natural voice for a minute.",
            "If you have ongoing hoarseness or pain, see a voice therapist or ENT doctor. Many speech therapists specialise in gender-affirming voice training.",
        ],
        exercises: &[
            ("Two-minute warm-up", "30 seconds humming, 30 seconds lip trills, 30 seconds \"ng\" sirens, 30 seconds of quiet \"mm-hmm\" answers."),
        ],
    },
];
