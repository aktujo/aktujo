// SPDX-License-Identifier: MIT
//! Values in text, for the built-in rules and for programs that specialise them: words and labels, dates, amounts
//! with their currency, numbers and references, names. German, English, French, Italian, Spanish and Dutch.

use std::ops::Range;
use std::sync::LazyLock;

use aktujo_doc::Value;
use regex::Regex;
use serde::{Deserialize, Serialize};

/// The words of a text in lower case, each with where it is (byte range); a word is a run of letters and digits.
pub fn words(text: &str) -> Vec<(String, Range<usize>)> {
    let mut words = vec![];
    let mut start = None;
    for (i, c) in text.char_indices().chain(std::iter::once((text.len(), ' '))) {
        match (c.is_alphanumeric(), start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                words.push((text[s..i].to_lowercase(), s..i));
                start = None;
            }
            _ => {}
        }
    }
    words
}

/// A text in lower case, its words separated by single spaces, with a space before and after – for matching whole
/// words and phrases with `contains(" phrase ")`.
pub fn normalise(text: &str) -> String {
    let words: Vec<String> = words(text).into_iter().map(|(w, _)| w).collect();
    format!(" {} ", words.join(" "))
}

/// Whether `phrase` (words, any case) occurs in `text` as whole words.
pub fn contains(text: &str, phrase: &str) -> bool {
    let phrase = normalise(phrase);
    phrase.trim() != "" && normalise(text).contains(&phrase)
}

/// Where `label` (words, any case) occurs in `text` as whole words: the byte range from its first to its last word.
pub fn find_label(text: &str, label: &str) -> Option<Range<usize>> {
    let needle: Vec<String> = words(label).into_iter().map(|(w, _)| w).collect();
    if needle.is_empty() {
        return None;
    }
    let haystack = words(text);
    haystack.windows(needle.len()).find(|w| w.iter().zip(&needle).all(|((a, _), b)| a == b)).map(|w| {
        let (first, last) = (&w[0].1, &w[w.len() - 1].1);
        first.start..last.end
    })
}

/// The order of day and month in dates with slashes (`03/04/2026`), which differs between countries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DateOrder {
    /// `05/11/2026` is the 5th of November – most of Europe.
    DayFirst,
    /// `11/05/2026` is the 5th of November – the United States.
    MonthFirst,
}

/// Month names and abbreviations in German, English, French, Italian, Spanish and Dutch.
const MONTHS: &[(&str, u32)] = &[
    ("januar", 1),
    ("jänner", 1),
    ("january", 1),
    ("janvier", 1),
    ("gennaio", 1),
    ("enero", 1),
    ("januari", 1),
    ("jan", 1),
    ("janv", 1),
    ("gen", 1),
    ("ene", 1),
    ("februar", 2),
    ("february", 2),
    ("février", 2),
    ("fevrier", 2),
    ("febbraio", 2),
    ("febrero", 2),
    ("februari", 2),
    ("feb", 2),
    ("févr", 2),
    ("märz", 3),
    ("maerz", 3),
    ("march", 3),
    ("mars", 3),
    ("marzo", 3),
    ("maart", 3),
    ("mär", 3),
    ("mar", 3),
    ("mrt", 3),
    ("april", 4),
    ("avril", 4),
    ("aprile", 4),
    ("abril", 4),
    ("apr", 4),
    ("avr", 4),
    ("abr", 4),
    ("mai", 5),
    ("may", 5),
    ("maggio", 5),
    ("mayo", 5),
    ("mei", 5),
    ("mag", 5),
    ("juni", 6),
    ("june", 6),
    ("juin", 6),
    ("giugno", 6),
    ("junio", 6),
    ("jun", 6),
    ("giu", 6),
    ("juli", 7),
    ("july", 7),
    ("juillet", 7),
    ("luglio", 7),
    ("julio", 7),
    ("jul", 7),
    ("juil", 7),
    ("lug", 7),
    ("august", 8),
    ("août", 8),
    ("aout", 8),
    ("agosto", 8),
    ("augustus", 8),
    ("aug", 8),
    ("ago", 8),
    ("september", 9),
    ("septembre", 9),
    ("settembre", 9),
    ("septiembre", 9),
    ("setiembre", 9),
    ("sept", 9),
    ("sep", 9),
    ("set", 9),
    ("oktober", 10),
    ("october", 10),
    ("octobre", 10),
    ("ottobre", 10),
    ("octubre", 10),
    ("okt", 10),
    ("oct", 10),
    ("ott", 10),
    ("november", 11),
    ("novembre", 11),
    ("noviembre", 11),
    ("nov", 11),
    ("dezember", 12),
    ("december", 12),
    ("décembre", 12),
    ("decembre", 12),
    ("dicembre", 12),
    ("diciembre", 12),
    ("dez", 12),
    ("dec", 12),
    ("déc", 12),
    ("dic", 12),
];

fn month(name: &str) -> Option<u32> {
    let name = name.to_lowercase();
    MONTHS.iter().find(|(n, _)| *n == name).map(|(_, m)| *m)
}

static ISO: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(\d{4})[-/](\d{2})[-/](\d{2})\b").unwrap());
static DOTTED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(\d{1,2})\.\s?(\d{1,2})\.\s?(\d{4}|\d{2})\b").unwrap());
static DASHED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(\d{1,2})-(\d{1,2})-(\d{4})\b").unwrap());
static SLASHED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(\d{1,2})/(\d{1,2})/(\d{4}|\d{2})\b").unwrap());
// "5. November 2026", "5 nov. 2026", "1er janvier 2026", "5 de noviembre de 2026".
static DAY_MONTH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(\d{1,2})(?:\.|er|º)?\s+(?:de\s+)?(\p{L}{3,10})\.?,?\s+(?:de\s+)?(\d{4})\b").unwrap()
});
// "November 5, 2026", "Nov 5th 2026".
static MONTH_DAY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(\p{L}{3,10})\.?\s+(\d{1,2})(?:st|nd|rd|th)?,?\s+(\d{4})\b").unwrap());

fn iso(year: u32, month: u32, day: u32) -> Option<String> {
    let date = format!("{year:04}-{month:02}-{day:02}");
    Value::Date(date.clone()).check().is_ok().then_some(date)
}

/// The dates in `text`, as `YYYY-MM-DD`, in the order they occur. Dates with slashes are read in `order`; without
/// one only where they cannot mean two days (`25/12/2026`), because `03/04/2026` differs between countries.
pub fn dates(text: &str, order: Option<DateOrder>) -> Vec<String> {
    let mut found: Vec<(usize, String)> = vec![];
    let n = |s: &str| s.parse::<u32>().ok();
    let year = |s: &str| n(s).map(|y| if s.len() == 2 { 2000 + y } else { y });
    let mut push = |at: usize, date: Option<String>| found.extend(date.map(|d| (at, d)));
    let start = |c: &regex::Captures| c.get(0).expect("the whole match").start();
    for c in ISO.captures_iter(text) {
        if let (Some(y), Some(m), Some(d)) = (n(&c[1]), n(&c[2]), n(&c[3])) {
            push(start(&c), iso(y, m, d));
        }
    }
    for c in DOTTED.captures_iter(text).chain(DASHED.captures_iter(text)) {
        if let (Some(d), Some(m), Some(y)) = (n(&c[1]), n(&c[2]), year(&c[3])) {
            push(start(&c), iso(y, m, d));
        }
    }
    for c in SLASHED.captures_iter(text) {
        let (Some(a), Some(b), Some(y)) = (n(&c[1]), n(&c[2]), year(&c[3])) else { continue };
        let order = match order {
            Some(order) => Some(order),
            None if a > 12 && b <= 12 => Some(DateOrder::DayFirst),
            None if b > 12 && a <= 12 => Some(DateOrder::MonthFirst),
            None => None,
        };
        match order {
            Some(DateOrder::DayFirst) => push(start(&c), iso(y, b, a)),
            Some(DateOrder::MonthFirst) => push(start(&c), iso(y, a, b)),
            None => {}
        }
    }
    for c in DAY_MONTH.captures_iter(text) {
        if let (Some(d), Some(m), Some(y)) = (n(&c[1]), month(&c[2]), n(&c[3])) {
            push(start(&c), iso(y, m, d));
        }
    }
    for c in MONTH_DAY.captures_iter(text) {
        if let (Some(m), Some(d), Some(y)) = (month(&c[1]), n(&c[2]), n(&c[3])) {
            push(start(&c), iso(y, m, d));
        }
    }
    found.sort_by_key(|(at, _)| *at);
    found.dedup();
    found.into_iter().map(|(_, d)| d).collect()
}

/// Currency signs and codes, and the ISO 4217 code each stands for.
const CURRENCIES: [(&str, &str); 8] = [
    ("€", "EUR"),
    ("EUR", "EUR"),
    ("$", "USD"),
    ("USD", "USD"),
    ("£", "GBP"),
    ("GBP", "GBP"),
    ("CHF", "CHF"),
    ("Fr.", "CHF"),
];

// Thousands separated by a point, comma, apostrophe (Switzerland) or a no-break space (France); a plain space is
// not, because it also separates a quantity from a price.
static AMOUNT: LazyLock<Regex> = LazyLock::new(|| {
    let currency = r"€|EUR|\$|USD|£|GBP|CHF|Fr\.";
    Regex::new(&format!(
        r"(?:({currency})\s?)?(-\s?)?\b(\d{{1,3}}(?:[.,'’\u{{a0}}\u{{202f}}]\d{{3}})*[.,]\d{{2}}|\d+[.,]\d{{2}})\b(?:\s?({currency}))?"
    ))
    .unwrap()
});

fn code(currency: &str) -> Option<&'static str> {
    CURRENCIES.iter().find(|(sign, _)| *sign == currency).map(|(_, code)| *code)
}

/// The currency named most often in `text` (signs and codes), if any.
pub fn main_currency(text: &str) -> Option<&'static str> {
    let mut counts: Vec<(&str, usize)> = vec![];
    for (sign, code) in CURRENCIES {
        let found = if sign.chars().all(|c| c.is_ascii_alphabetic()) {
            words(text).iter().filter(|(w, _)| w.eq_ignore_ascii_case(sign)).count()
        } else {
            text.matches(sign).count()
        };
        match counts.iter_mut().find(|(c, _)| *c == code) {
            Some((_, n)) => *n += found,
            None => counts.push((code, found)),
        }
    }
    counts.into_iter().filter(|(_, n)| *n > 0).max_by_key(|(_, n)| *n).map(|(c, _)| c)
}

/// The amounts in `text` with cents, in the order they occur: the amount as a decimal number with a point
/// (`1234.56`) and its currency – the sign or code next to it, else `fallback`. Without either, an amount is
/// skipped.
pub fn amounts(text: &str, fallback: Option<&str>) -> Vec<Value> {
    let mut found = vec![];
    for c in AMOUNT.captures_iter(text) {
        let currency = c.get(1).or(c.get(4)).and_then(|m| code(m.as_str())).or(fallback);
        let Some(currency) = currency else { continue };
        let number = c.get(3).expect("the number");
        // Part of a date or a longer number ("05.11.2026", "1.2.3"): the regex cannot look around.
        let (before, after) = (&text[..number.start()], &text[number.end()..]);
        let digit_after = after.strip_prefix(['.', ',']).is_some_and(|a| a.starts_with(|c: char| c.is_ascii_digit()));
        let digit_before = before.strip_suffix(['.', ',']).is_some_and(|b| b.ends_with(|c: char| c.is_ascii_digit()));
        if digit_after || digit_before {
            continue;
        }
        let number = number.as_str();
        // The separator before the last two digits is the decimal one; every other is for thousands.
        let (whole, cents) = number.split_at(number.len() - 3);
        let whole: String = whole.chars().filter(char::is_ascii_digit).collect();
        let whole = whole.trim_start_matches('0');
        let sign = if c.get(2).is_some() { "-" } else { "" };
        let amount = format!("{sign}{}.{}", if whole.is_empty() { "0" } else { whole }, &cents[1..]);
        found.push(Value::Money { amount, currency: currency.to_owned() });
    }
    found
}

/// The amount of a money value as a number, for comparing; 0 for other values.
pub fn amount(value: &Value) -> f64 {
    match value {
        Value::Money { amount, .. } => amount.parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

/// The first word with a digit that is not a date – a number or a reference (`2026-0042`, `32.1-0457/26`).
pub fn code_word(text: &str) -> Option<String> {
    text.split_whitespace()
        .map(|w| w.trim_matches(|c: char| ",;:()[]\"'".contains(c)).trim_end_matches('.'))
        .find(|w| w.chars().any(|c| c.is_ascii_digit()) && w.chars().count() >= 2 && dates(w, None).is_empty())
        .map(str::to_owned)
}

static NUMBER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?:^|[^\w.,])(-?\d+(?:[.,]\d+)?)\b").unwrap());

/// The first decimal number in `text`, with a point (`3,5` → `3.5`).
pub fn number(text: &str) -> Option<String> {
    NUMBER.captures(text).map(|c| c[1].replace(',', "."))
}

/// The rest of a line after a label: without separators at the start, life dates "(1770–1827)" or stray
/// punctuation; none if fewer than two letters or digits are left.
pub fn clean(text: &str) -> Option<String> {
    let text = text.trim_start_matches(|c: char| c.is_whitespace() || ":.#-–—|/".contains(c));
    let mut text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if let Some(open) = text.find('(')
        && text[open..].chars().filter(char::is_ascii_digit).count() >= 4
    {
        text.truncate(open);
    }
    let text = text.trim_matches(|c: char| c.is_whitespace() || ",;/|·".contains(c));
    (text.chars().filter(|c| c.is_alphanumeric()).count() >= 2).then(|| text.to_owned())
}

/// A sender line cut at its separators ("Muster GmbH · Hauptstr. 1 · 12345 Stadt"): the part that holds the byte
/// `at`.
pub fn segment(text: &str, at: usize) -> &str {
    let separators = ['·', '|', '•', ',', ';'];
    let start = text[..at].rfind(separators).map_or(0, |i| i + text[i..].chars().next().map_or(1, char::len_utf8));
    let end = text[at..].find(separators).map_or(text.len(), |i| at + i);
    // " - " separates too, but not the hyphen in a name.
    let part = &text[start..end];
    let (before, after) = part.split_at(at - start);
    let start_dash = before.rfind(" - ").map_or(0, |i| i + 3);
    let end_dash = after.find(" - ").unwrap_or(after.len());
    part[start_dash..before.len() + end_dash].trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn money(amount: &str, currency: &str) -> Value {
        Value::Money { amount: amount.into(), currency: currency.into() }
    }

    #[test]
    fn reads_dates() {
        assert_eq!(dates("Datum: 05.11.2026", None), ["2026-11-05"]);
        assert_eq!(dates("vom 5.11.26 bis 2026-12-31", None), ["2026-11-05", "2026-12-31"]);
        assert_eq!(dates("Berlin, 5. November 2026", None), ["2026-11-05"]);
        assert_eq!(dates("Invoice date: November 5th, 2026", None), ["2026-11-05"]);
        assert_eq!(dates("5 Nov 2026 and 1. März 2027", None), ["2026-11-05", "2027-03-01"]);
        assert!(dates("31.02.2026, Tel. 0301.12.2026", None).is_empty());
    }

    #[test]
    fn reads_dates_in_other_languages() {
        assert_eq!(dates("Paris, le 1er décembre 2026", None), ["2026-12-01"]);
        assert_eq!(dates("Roma, 5 novembre 2026", None), ["2026-11-05"]);
        assert_eq!(dates("Madrid, 5 de noviembre de 2026", None), ["2026-11-05"]);
        assert_eq!(dates("Amsterdam, 5 mei 2026 en 05-11-2026", None), ["2026-05-05", "2026-11-05"]);
    }

    #[test]
    fn reads_dates_with_slashes_only_in_an_order() {
        assert!(dates("03/04/2026", None).is_empty());
        assert_eq!(dates("03/04/2026", Some(DateOrder::DayFirst)), ["2026-04-03"]);
        assert_eq!(dates("03/04/2026", Some(DateOrder::MonthFirst)), ["2026-03-04"]);
        // Unambiguous without an order.
        assert_eq!(dates("25/12/2026 and 12/25/26", None), ["2026-12-25", "2026-12-25"]);
        assert_eq!(dates("2026/11/05", None), ["2026-11-05"]);
    }

    #[test]
    fn reads_amounts() {
        assert_eq!(amounts("Gesamtbetrag 1.234,56 €", None), [money("1234.56", "EUR")]);
        assert_eq!(amounts("Total: EUR 1,234.56", None), [money("1234.56", "EUR")]);
        assert_eq!(amounts("Total TTC 1\u{202f}234,56 €", None), [money("1234.56", "EUR")]);
        assert_eq!(amounts("Total CHF 1'234.50", None), [money("1234.50", "CHF")]);
        assert_eq!(amounts("SUMME 12,50", Some("EUR")), [money("12.50", "EUR")]);
        assert_eq!(amounts("Rabatt -3,00 €", None), [money("-3.00", "EUR")]);
        assert_eq!(amounts("£0.99 and CHF 2.50", None), [money("0.99", "GBP"), money("2.50", "CHF")]);
        assert!(amounts("SUMME 12,50", None).is_empty());
        assert!(amounts("Menge 3 Stück, Datum 05.11.2026, Version 1.2.33", Some("EUR")).is_empty());
        assert_eq!(amounts("Pos 3 100,00 €", None), [money("100.00", "EUR")]);
    }

    #[test]
    fn finds_the_main_currency() {
        assert_eq!(main_currency("Betrag in EUR\n12,50\n€ 3,00\nUSD"), Some("EUR"));
        assert_eq!(main_currency("no money here, not even Europe"), None);
    }

    #[test]
    fn finds_labels_as_whole_words() {
        let text = "Rechnungs-Nr.: 2026-0042";
        let at = find_label(text, "rechnungs nr").unwrap();
        assert_eq!(&text[at.clone()], "Rechnungs-Nr");
        assert_eq!(code_word(&text[at.end..]).as_deref(), Some("2026-0042"));
        assert_eq!(code_word("vom 05.11.2026 Nr. A-17").as_deref(), Some("A-17"));
        assert_eq!(number("Menge: 3,5 kg").as_deref(), Some("3.5"));
        assert!(find_label("Lieferdatum 01.11.2026", "datum").is_none());
        assert_eq!(normalise("Mit freundlichen Grüßen,"), " mit freundlichen grüßen ");
        assert!(contains("Madame, Monsieur,", "madame monsieur"));
    }

    #[test]
    fn cleans_and_cuts() {
        assert_eq!(clean(": Ludwig van Beethoven (1770–1827)").as_deref(), Some("Ludwig van Beethoven"));
        assert_eq!(clean(" – ").as_deref(), None);
        let line = "Muster GmbH · Hauptstr. 1 · 12345 Stadt";
        assert_eq!(segment(line, line.find("GmbH").unwrap()), "Muster GmbH");
        let line = "Stadtwerke Muster-Stadt GmbH - Postfach 1";
        assert_eq!(segment(line, line.find("GmbH").unwrap()), "Stadtwerke Muster-Stadt GmbH");
    }
}
