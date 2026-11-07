use super::super::Entry;
use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

pub(super) fn fold(s: &str) -> String {
    s.to_lowercase()
        .nfd()
        .filter(|c| !is_combining_mark(*c))
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect()
}
// Also allow typing a Russian title in Latin letters (or a Japanese name like Kūkai in Cyrillic).
pub(super) fn roman(word: &str) -> String {
    let mut out = String::new();
    for c in word.chars() {
        out.push_str(match c {
            'а' => "a",
            'б' => "b",
            'в' => "v",
            'г' => "g",
            'д' => "d",
            'е' => "e",
            'ж' => "zh",
            'з' => "z",
            'и' | 'й' => "i",
            'к' => "k",
            'л' => "l",
            'м' => "m",
            'н' => "n",
            'о' => "o",
            'п' => "p",
            'р' => "r",
            'с' => "s",
            'т' => "t",
            'у' => "u",
            'ф' => "f",
            'х' => "kh",
            'ц' => "ts",
            'ч' => "ch",
            'ш' => "sh",
            'щ' => "shch",
            'ъ' | 'ь' => "",
            'ы' => "y",
            'э' => "e",
            'ю' => "yu",
            'я' => "ya",
            c => {
                out.push(c);
                continue;
            }
        });
    }
    out
}
fn word_score(needle: &str, word: &str) -> Option<i32> {
    if needle == word {
        return Some(120);
    }
    if word.starts_with(needle) {
        return Some(100);
    }
    if word.contains(needle) {
        return Some(78);
    }
    let mut chars = needle.chars();
    let mut next = chars.next()?;
    let mut start = None;
    let n = needle.chars().count();
    let mut best = None;
    for (i, c) in word.chars().enumerate() {
        if c == next {
            let a = *start.get_or_insert(i);
            if let Some(c) = chars.next() {
                next = c;
            } else {
                let gaps = i + 1 - a - n;
                if gaps <= n + 1 && n >= 2 {
                    best = Some(65 - (gaps.min(20) + a.min(10)) as i32);
                }
                break;
            }
        }
    }
    best
}
pub(super) struct Candidate {
    pub(super) entry: Entry,
    words: Vec<(String, String)>,
}
impl Candidate {
    pub(super) fn new(entry: Entry) -> Self {
        let mut text = entry.name.clone();
        if let Some(m) = &entry.meta {
            for field in std::iter::once(&m.title).chain(&m.authors).chain(&m.series) {
                text.push(' ');
                text.push_str(field);
            }
        }
        let mut words = fold(&text)
            .split_whitespace()
            .map(|w| (w.to_owned(), roman(w)))
            .collect::<Vec<_>>();
        words.sort_unstable();
        words.dedup();
        Self { entry, words }
    }
    pub(super) fn score(&self, query: &[(String, String)]) -> Option<i32> {
        query
            .iter()
            .map(|(q, r)| {
                self.words
                    .iter()
                    .filter_map(|(w, t)| {
                        word_score(q, w).or_else(|| {
                            if (q != r || w != t) && !r.is_empty() {
                                word_score(r, t).map(|s| s - 12)
                            } else {
                                None
                            }
                        })
                    })
                    .max()
            })
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_title_words_rank_above_abbreviations_and_romanizations() {
        assert!(word_score("amber", "amber") > word_score("amber", "ambler"));
        assert!(word_score("желз", "желязны").is_some());
        assert_eq!(roman(&fold("Кукай")), fold("Kūkai"));
        assert!(word_score("море", "янтарь").is_none());
    }
}
