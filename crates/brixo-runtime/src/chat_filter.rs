//! The chat filter: hides swearing, slurs, sexual words and telling people
//! to hurt themselves, and lets everything else through.
//!
//! It's deliberately **not** strict about ordinary gaming talk: "noob",
//! "dumb", "loser", "hell", "damn", "shut up", "I'll kill you" (it's a
//! sword fight) all get through. It works on whole words, so "class",
//! "assassin", "Scunthorpe" and "cocktail" are fine, and it sees through
//! the usual dodges: capitals, stretched letters ("fuuuck"), numbers and
//! symbols for letters ("sh1t", "a$$"), stars ("f*ck"), dots and spaces
//! ("f.u.c.k", "f u c k"), and words glued together ("motherfucker").
//! A hidden word is replaced by #s. The same filter checks usernames,
//! profile blurbs and game descriptions on the website.

/// Blocked anywhere inside a word: no ordinary word contains these.
const ANYWHERE: &[&str] = &["fuck", "shit", "bitch", "nigger", "nigga", "faggot", "whore", "dildo", "porn"];

/// Blocked as a whole word, or with an ending ("dicks", "raped",
/// "retarded", "sluts", "dickhead"). Not inside other words, which is where
/// the false alarms come from ("Dickens" is fine, "therapist" is fine).
const WORDS: &[&str] = &[
    "ass", "asshole", "arse", "cunt", "pussy", "fuk", "bastard", "dick", "cock", "cum", "penis", "vagina", "boob", "tit", "tits", "slut", "rape", "rapist",
    "retard", "fag", "tranny", "chink", "spic", "kike", "wank", "wanker", "twat", "piss", "sex", "sexy", "nude", "nudes", "horny",
    "kys", "nazi", "hitler", "molest",
];

/// Endings a blocked WORD can have and still be blocked.
const ENDINGS: &[&str] = &["", "s", "es", "ed", "er", "ers", "ing", "in", "y", "ies", "head", "heads", "face", "hole", "holes", "wad", "bag", "ish"];

/// Blocked when these words come one after the other.
const PHRASES: &[&[&str]] = &[
    &["kill", "yourself"],
    &["kill", "urself"],
    &["kill", "ur", "self"],
    &["kill", "your", "self"],
    &["hang", "yourself"],
    &["go", "kill", "yourself"],
    &["neck", "yourself"],
];

/// Reads a word the way a dodger meant it: lowercase, 0 as o, 1 as i, $ as
/// s, @ as a, and so on, and nothing but letters. Numbers alone ("1337",
/// "404") stay numbers and aren't words at all.
fn plain(word: &str) -> String {
    let lower = word.to_lowercase();
    if !lower.chars().any(|c| c.is_alphabetic()) {
        return String::new();
    }
    lower
        .chars()
        .map(|c| match c {
            '0' => 'o',
            '1' | '!' | '|' => 'i',
            '3' => 'e',
            '4' | '@' => 'a',
            '5' | '$' => 's',
            '7' | '+' => 't',
            '8' => 'b',
            '9' => 'g',
            c => c,
        })
        .filter(|c| c.is_alphabetic() || *c == '*')
        .collect()
}

/// Stretched letters (three or more the same) squeezed to one: "fuuuck"
/// is "fuck". Doubles stay doubles: "shiitake" is a mushroom.
fn squeeze(word: &str) -> String {
    let chars: Vec<char> = word.chars().collect();
    let mut out = String::with_capacity(word.len());
    let mut i = 0;
    while i < chars.len() {
        let mut j = i;
        while j < chars.len() && chars[j] == chars[i] {
            j += 1;
        }
        let run = j - i;
        for _ in 0..if run >= 3 { 1 } else { run } {
            out.push(chars[i]);
        }
        i = j;
    }
    out
}

/// Whether one (plain) word is blocked on its own.
fn blocked_word(w: &str) -> bool {
    if w.is_empty() {
        return false;
    }
    // Stars standing for letters: "f*ck", "sh**", "b*tch". Same first and
    // last letter as a blocked word, about the same length.
    // Stars round a word are emphasis ("*sigh*"), not hidden letters.
    if w.starts_with('*') || w.ends_with('*') {
        let inner = w.trim_matches('*');
        return inner != w && blocked_word(inner);
    }
    if w.contains('*') {
        let letters = w.trim_matches('*');
        if letters.chars().count() < 2 {
            return false;
        }
        let (first, last) = (w.chars().next().unwrap(), w.chars().last().unwrap());
        return ANYWHERE.iter().chain(WORDS).filter(|b| b.len() >= 4).any(|b| {
            let (bf, bl) = (b.chars().next().unwrap(), b.chars().last().unwrap());
            first == bf && last == bl && (w.len() as i32 - b.len() as i32).abs() <= 1
        });
    }
    let squeezed = squeeze(w);
    for form in [w, squeezed.as_str()] {
        if ANYWHERE.iter().any(|b| form.contains(b)) {
            return true;
        }
        for b in WORDS {
            if let Some(rest) = form.strip_prefix(*b) {
                if ENDINGS.contains(&rest) {
                    return true;
                }
            }
        }
    }
    false
}

/// Replaces blocked words with #s (the same length), leaving the rest.
pub fn filter_chat(text: &str) -> String {
    // The words, with where each one is in the text (in bytes).
    let mut words: Vec<(usize, usize, String)> = Vec::new();
    let mut start = None;
    for (i, c) in text.char_indices() {
        let part_of_word = !c.is_whitespace() && !matches!(c, ',' | ';' | '?' | '"' | '(' | ')');
        match (part_of_word, start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                words.push((s, i, plain(&text[s..i])));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        words.push((s, text.len(), plain(&text[s..])));
    }
    // A word's trailing punctuation isn't part of it ("stupid!" is "stupid").
    for w in &mut words {
        let trimmed = w.2.trim_end_matches(|c: char| !c.is_alphabetic() && c != '*').to_string();
        w.2 = trimmed;
    }

    let mut hide = vec![false; words.len()];
    for (i, w) in words.iter().enumerate() {
        hide[i] = blocked_word(&w.2);
    }
    // Spelled out a letter at a time: "f u c k", "f.u.c.k" (one word, dots
    // already gone), "k y s".
    let mut i = 0;
    while i < words.len() {
        if words[i].2.chars().count() == 1 {
            let mut j = i;
            while j < words.len() && words[j].2.chars().count() == 1 {
                j += 1;
            }
            if j - i >= 3 {
                let joined: String = words[i..j].iter().map(|w| w.2.as_str()).collect();
                if blocked_word(&joined) {
                    hide[i..j].iter_mut().for_each(|h| *h = true);
                }
            }
            i = j;
        } else {
            i += 1;
        }
    }
    // Phrases.
    for phrase in PHRASES {
        for i in 0..words.len() {
            if i + phrase.len() <= words.len() && phrase.iter().enumerate().all(|(k, p)| squeeze(&words[i + k].2) == squeeze(p)) {
                hide[i..i + phrase.len()].iter_mut().for_each(|h| *h = true);
            }
        }
    }

    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for (k, (s, e, _)) in words.iter().enumerate() {
        out.push_str(&text[at..*s]);
        if hide[k] {
            // Keep a word's trailing punctuation: "######!".
            let word = &text[*s..*e];
            let keep = word.trim_end_matches(|c: char| matches!(c, '!' | '.' | ':' | '~'));
            let keep = if keep.is_empty() { word } else { keep };
            out.push_str(&"#".repeat(keep.chars().count()));
            out.push_str(&word[keep.len()..]);
        } else {
            out.push_str(&text[*s..*e]);
        }
        at = *e;
    }
    out.push_str(&text[at..]);
    out
}

#[cfg(test)]
mod tests {
    use super::filter_chat;

    #[track_caller]
    fn clean(text: &str) {
        assert_eq!(filter_chat(text), text, "{text:?} should get through");
    }

    #[track_caller]
    fn hidden(text: &str) {
        assert!(filter_chat(text).contains('#'), "{text:?} should be hidden, got {:?}", filter_chat(text));
    }

    #[test]
    fn ordinary_gaming_talk_gets_through() {
        for t in [
            "gg ez noob",
            "ur so bad lol, loser",
            "that was dumb",
            "shut up and jump",
            "what the hell",
            "damn that's a good build",
            "crap I fell in the lava",
            "I'll kill you with my sword",
            "die die die (rocket launcher)",
            "you're stupid good at this",
            "idiot proof obby",
            "I hate this level",
        ] {
            clean(t);
        }
    }

    #[test]
    fn ordinary_words_that_contain_bad_ones_get_through() {
        for t in [
            "class", "pass the ball", "assassin", "grass", "bass guitar", "assume", "Scunthorpe", "cocktail", "cockpit",
            "Dickens", "therapist", "analysis", "Essex", "button", "hello shellfish", "shiitake", "documents", "Sussex",
            "cumulative", "title", "petition", "specific", "cockatoo", "hitchhike", "skyscraper", "as if", "sextant", "*sigh*",
            "Mississippi", "5*5=25", "Fukushima", "pussycat dolls",
            "1337 404 2x speed", "we won 5-3!", "brb, afk?",
        ] {
            clean(t);
        }
    }

    #[test]
    fn swearing_and_its_dodges_are_hidden() {
        for t in [
            "fuck", "FUCK YOU", "fuuuuck", "f*ck", "f**k", "sh*t", "sh1t", "b1tch", "a$$", "a$$hole", "f.u.c.k", "f u c k this",
            "motherfucker", "fucking hell", "shitty game", "bullshit", "you're a dick", "dickhead", "piss off", "what an asshole",
        ] {
            hidden(t);
        }
    }

    #[test]
    fn slurs_sexual_words_and_self_harm_are_hidden() {
        for t in ["kys", "k y s", "kill yourself", "go kill urself", "hang yourself", "send nudes", "u r a retard", "retarded", "porn", "sexy"] {
            hidden(t);
        }
    }

    #[test]
    fn only_the_bad_word_is_hidden() {
        assert_eq!(filter_chat("that's shit!"), "that's ####!");
        assert_eq!(filter_chat("gg, fuck off"), "gg, #### off");
        assert_eq!(filter_chat("pls kill yourself now"), "pls #### ######## now");
    }
}
