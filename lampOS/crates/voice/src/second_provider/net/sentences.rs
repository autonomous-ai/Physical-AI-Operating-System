//! Streamed model text cut into whole sentences, so speech can begin with
//! the first one while the rest is still being written.

/// Words whose `.` does not end a sentence.
// ponytail: a fixed English list and "terminator then whitespace"; quotes
// after the stop, "e.g." mid-word, ellipses and non-Latin stops (the ideographic full stop) are not
// handled. A real segmenter if speech is heard cut in the wrong place.
const ABBREVIATIONS: &[&str] = &[
    "mr", "mrs", "ms", "dr", "st", "jr", "sr", "vs", "etc", "prof",
];

/// The fewest words a first clause is said on its own with.
pub const CLAUSE_WORDS: usize = 4;

#[derive(Debug, Default)]
pub struct Sentences {
    text: String,
}

impl Sentences {
    /// Add text as it arrives; returns each sentence it completed, trimmed.
    /// A stop is a sentence's end only once the whitespace after it has
    /// arrived, so `3.5` split across two pieces is not cut.
    pub fn push(&mut self, piece: &str) -> Vec<String> {
        self.text.push_str(piece);
        let mut done = Vec::new();
        let mut start = 0;
        let mut chars = self.text.char_indices().peekable();
        while let Some((at, c)) = chars.next() {
            let Some(&(next_at, next)) = chars.peek() else {
                break;
            };
            if matches!(c, '.' | '!' | '?')
                && next.is_whitespace()
                && !(c == '.' && abbreviation(&self.text[start..at]))
            {
                let sentence = self.text[start..=at].trim();
                if !sentence.is_empty() {
                    done.push(sentence.to_owned());
                }
                start = next_at;
            }
        }
        self.text.drain(..start);
        done
    }

    /// The words written so far up to a comma, semicolon or colon, if at
    /// least [`CLAUSE_WORDS`] words come before it: for an answer's first
    /// words, so speech starts before the first sentence is finished.
    pub fn clause(&mut self) -> Option<String> {
        let mut words = 0;
        let mut chars = self.text.char_indices().peekable();
        while let Some((at, c)) = chars.next() {
            if c.is_whitespace() {
                words += 1;
            }
            let Some(&(next_at, next)) = chars.peek() else {
                break;
            };
            if matches!(c, ',' | ';' | ':') && next.is_whitespace() && words + 1 >= CLAUSE_WORDS {
                let clause = self.text[..=at].trim().to_owned();
                self.text.drain(..next_at);
                return Some(clause);
            }
        }
        None
    }

    /// Whatever is left when the text ends.
    pub fn finish(&mut self) -> Option<String> {
        let rest = std::mem::take(&mut self.text);
        let rest = rest.trim();
        (!rest.is_empty()).then(|| rest.to_owned())
    }
}

fn abbreviation(before: &str) -> bool {
    let word = before
        .rsplit(|c: char| !c.is_alphabetic())
        .next()
        .unwrap_or("");
    ABBREVIATIONS
        .iter()
        .any(|known| word.eq_ignore_ascii_case(known))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all(pieces: &[&str]) -> Vec<String> {
        let mut sentences = Sentences::default();
        let mut out: Vec<String> = pieces.iter().flat_map(|p| sentences.push(p)).collect();
        out.extend(sentences.finish());
        out
    }

    #[test]
    fn a_first_clause_of_four_words_is_cut_at_its_comma() {
        let mut sentences = Sentences::default();
        assert!(sentences.push("Well, ").is_empty());
        assert_eq!(sentences.clause(), None);
        sentences.push("it is about nine million, give or take");
        assert_eq!(
            sentences.clause().as_deref(),
            Some("Well, it is about nine million,")
        );
        assert_eq!(sentences.finish().as_deref(), Some("give or take"));
    }

    #[test]
    fn sentences_end_at_a_stop_and_whitespace() {
        assert_eq!(
            all(&["Hello there! How", " are you? I'm", " fine."]),
            ["Hello there!", "How are you?", "I'm fine."]
        );
    }

    #[test]
    fn a_sentence_is_held_until_its_stop_is_followed() {
        let mut sentences = Sentences::default();
        assert!(sentences.push("It is 3.").is_empty());
        assert!(sentences.push("5 degrees.").is_empty());
        assert_eq!(sentences.push(" Cold."), ["It is 3.5 degrees."]);
        assert_eq!(sentences.finish().as_deref(), Some("Cold."));
    }

    #[test]
    fn titles_do_not_end_a_sentence() {
        assert_eq!(
            all(&["Ask Mr. Smith and Dr. Who. Then go."]),
            ["Ask Mr. Smith and Dr. Who.", "Then go."]
        );
    }

    #[test]
    fn nothing_left_is_nothing() {
        let mut sentences = Sentences::default();
        assert_eq!(sentences.push("Done.\n"), ["Done."]);
        assert_eq!(sentences.finish(), None);
    }
}
