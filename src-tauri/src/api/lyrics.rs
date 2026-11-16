//! Apple Music lyrics: TTML → a typed, render-ready model.
//!
//! Apple serves lyrics as TTML from an **undocumented** endpoint:
//!
//! ```text
//! GET /v1/catalog/{storefront}/songs/{catalogId}/lyrics?l={lang}
//!   → data[0].attributes.ttml
//! ```
//!
//! Measured against a live session (39 tracks in the `id` storefront, `en-GB`):
//!
//! | shape | count | what it contains |
//! |-------|-------|------------------|
//! | `itunes:timing="Line"` | 36 | `<p begin end>` per line, **no word spans** |
//! | `itunes:timing="None"` | 3 | bare `<p>text</p>`, no timing at all |
//! | 404 | 1 | no lyrics for that catalog song |
//!
//! So word-level timing is *supported* here (some songs and locales do ship
//! `<span begin end>`), but in practice it is rare. When a line has no timed spans we
//! synthesise per-word segments by spreading the line's window across its words in
//! proportion to word length. That is an approximation, and [`Lyrics::word_timed`]
//! says which is which so the UI and the docs never claim more than Apple gave us.
//!
//! Parsing rules that were learned the hard way:
//!
//! - **Whitespace lives in text nodes between spans.** `<p>Hello <span>world</span></p>`
//!   must not become `Helloworld`, so text events are kept verbatim and only trimmed at
//!   the edges of a line/segment.
//! - **Times are mixed formats.** A single document uses both `44.851` (bare seconds)
//!   and `1:04.163` (M:SS.mmm); the body `dur` can be `5:26.491`.
//! - **`ttm:role="x-bg"`** marks background vocals. They are kept as separate segments
//!   so the UI can paint them dimmer, which is what Apple does.
//! - **Degrade, never fail.** Unknown tags, unknown roles and missing attributes are
//!   ignored; a document with no `<p>` yields [`Lyrics::default`].

use serde::{Deserialize, Serialize};

/// How precisely Apple timed the song.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LyricsTiming {
    /// Real per-word (or per-syllable) spans were present.
    Word,
    /// Line-level `<p begin end>` only.
    Line,
    /// No timing at all (bare `<p>` elements).
    #[default]
    None,
}

/// One paintable chunk of a line: normally a word, sometimes a background-vocal run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricSegment {
    pub start_ms: u64,
    pub end_ms: u64,
    /// Includes a trailing space when one is needed to keep the line readable.
    pub text: String,
    /// `ttm:role="x-bg"` — background vocals / ad-libs.
    #[serde(default)]
    pub background: bool,
}

/// One lyric line (`<p>`), with its word segments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricLine {
    pub start_ms: u64,
    pub end_ms: u64,
    /// Full line text, used for unsynced display and for search-in-lyrics later.
    pub text: String,
    /// Word-level chunks. Empty for unsynced lyrics.
    #[serde(default)]
    pub segments: Vec<LyricSegment>,
    /// Apple's stable line key (`itunes:key="L12"`), when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

impl LyricLine {
    /// Consumers can highlight words without inventing text of their own.
    pub fn display_segments(&self) -> impl Iterator<Item = &LyricSegment> {
        self.segments.iter()
    }
}

/// A whole song's lyrics, already normalised to milliseconds.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lyrics {
    pub lines: Vec<LyricLine>,
    pub timing: LyricsTiming,
    /// `true` when segment timings are Apple's own spans; `false` when they were
    /// interpolated across the line. Never assume true.
    pub word_timed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// `leadingSilence` from `<iTunesMetadata>`; 0 when absent.
    #[serde(default)]
    pub leading_silence_ms: u64,
}

impl Lyrics {
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

// ─── Public entry point ────────────────────────────────────────

/// Parse an Apple lyrics TTML document. Never fails: malformed input degrades to
/// whatever could be recovered, and garbage degrades to an empty [`Lyrics`].
pub fn parse_ttml(ttml: &str) -> Lyrics {
    let mut lyrics = Lyrics::default();
    let mut line: Option<PendingLine> = None;
    let mut segment: Option<PendingSegment> = None;
    let mut saw_timed_span = false;

    for event in tokenize(ttml) {
        match event {
            Event::Start { name, attrs, .. } => match local_name(&name) {
                "tt" => {
                    if let Some(value) =
                        attr(&attrs, "itunes:timing").or_else(|| attr(&attrs, "timing"))
                    {
                        lyrics.timing = match value.to_ascii_lowercase().as_str() {
                            "word" | "syllable" => LyricsTiming::Word,
                            "line" => LyricsTiming::Line,
                            _ => LyricsTiming::None,
                        };
                    }
                    lyrics.language = attr(&attrs, "xml:lang").or_else(|| attr(&attrs, "lang"));
                }
                "iTunesMetadata" | "itunesmetadata" => {
                    if let Some(v) = attr(&attrs, "leadingSilence") {
                        lyrics.leading_silence_ms = seconds_to_ms(&v).unwrap_or(0);
                    }
                }
                "p" => {
                    line = Some(PendingLine {
                        start_ms: attr(&attrs, "begin")
                            .and_then(|v| parse_time(&v))
                            .unwrap_or(0),
                        end_ms: attr(&attrs, "end")
                            .and_then(|v| parse_time(&v))
                            .unwrap_or(0),
                        key: attr(&attrs, "itunes:key").or_else(|| attr(&attrs, "key")),
                        text: String::new(),
                        segments: Vec::new(),
                    });
                    segment = None;
                }
                "span" => {
                    if line.is_some() {
                        let start = attr(&attrs, "begin").and_then(|v| parse_time(&v));
                        let end = attr(&attrs, "end").and_then(|v| parse_time(&v));
                        if start.is_some() && end.is_some() {
                            saw_timed_span = true;
                        }
                        let role = attr(&attrs, "ttm:role").or_else(|| attr(&attrs, "role"));
                        segment = Some(PendingSegment {
                            start_ms: start,
                            end_ms: end,
                            background: role.as_deref() == Some("x-bg"),
                            text: String::new(),
                        });
                    }
                }
                "br" => append_text(&mut line, &mut segment, LINE_BREAK),
                _ => {}
            },
            Event::End { name } => match local_name(&name) {
                "span" => {
                    if let Some(done) = segment.take() {
                        // A span with no `begin`/`end` is just markup around text, not a
                        // karaoke unit. Its text already reached the line, so dropping it
                        // here keeps "segments non-empty ⇒ segments are timed" true.
                        let timed = done.start_ms.is_some() && done.end_ms.is_some();
                        if timed {
                            if let Some(current) = line.as_mut() {
                                // Keep the span text verbatim: the space that separates two
                                // words often lives at the end of the previous span, and
                                // trimming it would render "Helloworld".
                                if !done.text.trim().is_empty() {
                                    current.segments.push(LyricSegment {
                                        start_ms: done.start_ms.unwrap_or(current.start_ms),
                                        end_ms: done.end_ms.unwrap_or(current.end_ms),
                                        text: done.text,
                                        background: done.background,
                                    });
                                }
                            }
                        }
                    }
                }
                "p" => {
                    // A `<p>` closes any open span without a matching end tag.
                    segment = None;
                    if let Some(done) = line.take() {
                        let text = normalize_line_text(&done.text);
                        if !text.is_empty() {
                            let mut segments = done.segments;
                            // Real timed spans win; otherwise spread the line's window
                            // across its words ourselves (and `word_timed` stays false).
                            if segments.is_empty() {
                                segments = synthesize_segments(&text, done.start_ms, done.end_ms);
                            }
                            lyrics.lines.push(LyricLine {
                                start_ms: done.start_ms,
                                end_ms: done.end_ms,
                                text,
                                segments,
                                key: done.key,
                            });
                        }
                    }
                }
                _ => {}
            },
            Event::Text(text) => append_text(&mut line, &mut segment, &text),
        }
    }

    // No timed spans anywhere ⇒ segment times are ours, not Apple's.
    lyrics.word_timed = saw_timed_span;
    if !lyrics.word_timed && lyrics.timing == LyricsTiming::Word {
        // Apple said Word but shipped no timed spans; do not over-claim.
        lyrics.timing = LyricsTiming::Line;
    }
    if lyrics
        .lines
        .iter()
        .all(|l| l.start_ms == 0 && l.end_ms == 0)
    {
        lyrics.timing = LyricsTiming::None;
    }

    lyrics
}

// ─── Internals ─────────────────────────────────────────────────

struct PendingLine {
    start_ms: u64,
    end_ms: u64,
    key: Option<String>,
    text: String,
    segments: Vec<LyricSegment>,
}

struct PendingSegment {
    start_ms: Option<u64>,
    end_ms: Option<u64>,
    background: bool,
    text: String,
}

fn append_text(line: &mut Option<PendingLine>, segment: &mut Option<PendingSegment>, text: &str) {
    if let Some(seg) = segment.as_mut() {
        seg.text.push_str(text);
    }
    if let Some(current) = line.as_mut() {
        current.text.push_str(text);
    }
}

/// Split a line into per-word segments, spreading the line's window in proportion to
/// word length. Approximate by design: Apple did not tell us when each word is sung.
fn synthesize_segments(text: &str, start_ms: u64, end_ms: u64) -> Vec<LyricSegment> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return Vec::new();
    }
    let window = end_ms.saturating_sub(start_ms);
    if window == 0 {
        let last_index = words.len() - 1;
        return words
            .iter()
            .enumerate()
            .map(|(index, w)| LyricSegment {
                start_ms,
                end_ms,
                text: if index == last_index {
                    (*w).to_string()
                } else {
                    format!("{w} ")
                },
                background: false,
            })
            .collect();
    }

    let total_chars: usize = words.iter().map(|w| w.chars().count().max(1)).sum();
    let mut out = Vec::with_capacity(words.len());
    let mut consumed = 0usize;
    let mut cursor = start_ms;

    for (index, word) in words.iter().enumerate() {
        consumed += word.chars().count().max(1);
        let last = index + 1 == words.len();
        let boundary =
            start_ms + ((window as f64) * (consumed as f64) / (total_chars as f64)).round() as u64;
        let segment_end = if last { end_ms } else { boundary };
        let mut text = (*word).to_string();
        if !last {
            text.push(' ');
        }
        out.push(LyricSegment {
            start_ms: cursor,
            end_ms: segment_end.max(cursor),
            text,
            background: false,
        });
        cursor = segment_end;
    }

    out
}

/// `44.851`, `1:04.163`, `1:02:03.5` → milliseconds.
pub fn parse_time(value: &str) -> Option<u64> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut total = 0f64;
    let mut parts = 0usize;
    for part in trimmed.split(':') {
        total = total * 60.0 + part.trim().parse::<f64>().ok()?;
        parts += 1;
        if parts > 3 {
            return None;
        }
    }
    Some((total * 1000.0).round() as u64)
}

fn seconds_to_ms(value: &str) -> Option<u64> {
    parse_time(value)
}

/// Placeholder for `<br/>` while formatting whitespace is collapsed.
const LINE_BREAK: &str = "\u{000b}";

/// Collapse the whitespace that XML editors (and our own test fixtures) leave inside a
/// paragraph, while keeping explicit `<br/>` line breaks. Apple's own payload is one
/// long line, but nothing here should depend on that.
fn normalize_line_text(raw: &str) -> String {
    let mut collapsed = String::with_capacity(raw.len());
    let mut last_was_space = false;
    for ch in raw.chars() {
        if ch == '\u{000b}' {
            collapsed.push(ch);
            last_was_space = false;
            continue;
        }
        if ch.is_whitespace() {
            if !last_was_space && !collapsed.ends_with(LINE_BREAK) {
                collapsed.push(' ');
            }
            last_was_space = true;
            continue;
        }
        collapsed.push(ch);
        last_was_space = false;
    }

    collapsed
        .split(LINE_BREAK)
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn local_name(tag: &str) -> &str {
    match tag.rfind(':') {
        Some(index) => &tag[index + 1..],
        None => tag,
    }
}

fn attr(attrs: &[(String, String)], name: &str) -> Option<String> {
    attrs
        .iter()
        .find(|(key, _)| key == name || key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.clone())
}

// ─── Minimal XML tokenizer ─────────────────────────────────────
//
// TTML from Apple is a flat, well-formed subset: elements, attributes, text, CDATA
// and the occasional comment. A full XML dependency would be more code than this and
// another crate to audit, so the subset is handled directly.

enum Event {
    Start {
        name: String,
        attrs: Vec<(String, String)>,
    },
    End {
        name: String,
    },
    Text(String),
}

fn tokenize(src: &str) -> Vec<Event> {
    let bytes = src.as_bytes();
    let mut events = Vec::new();
    let mut index = 0usize;
    let mut text_start = 0usize;

    while index < bytes.len() {
        if bytes[index] != b'<' {
            index += 1;
            continue;
        }

        if index > text_start {
            events.push(Event::Text(decode_entities(&src[text_start..index])));
        }

        // Comments, CDATA and processing instructions are skipped, not parsed.
        if src[index..].starts_with("<!--") {
            index = src[index..]
                .find("-->")
                .map(|o| index + o + 3)
                .unwrap_or(bytes.len());
            text_start = index;
            continue;
        }
        if src[index..].starts_with("<![CDATA[") {
            let body_start = index + 9;
            let (body, next) = match src[body_start..].find("]]>") {
                Some(offset) => (
                    &src[body_start..body_start + offset],
                    body_start + offset + 3,
                ),
                None => (&src[body_start..], bytes.len()),
            };
            events.push(Event::Text(body.to_string()));
            index = next;
            text_start = index;
            continue;
        }
        if src[index..].starts_with("<!") || src[index..].starts_with("<?") {
            let end = src[index..]
                .find('>')
                .map(|o| index + o + 1)
                .unwrap_or(bytes.len());
            index = end;
            text_start = index;
            continue;
        }

        let (tag, next) = read_tag(src, index);
        index = next;
        text_start = index;

        let tag = tag.trim();
        if let Some(name) = tag.strip_prefix('/') {
            events.push(Event::End {
                name: name.trim().to_string(),
            });
        } else {
            let body = tag.trim_end_matches('/').trim_end();
            let (name, attrs) = parse_start_tag(body);
            if !name.is_empty() {
                events.push(Event::Start { name, attrs });
            }
        }
    }

    if text_start < bytes.len() {
        events.push(Event::Text(decode_entities(&src[text_start..])));
    }

    events
}

/// Returns the tag body between `<` and `>` (exclusive) plus the index after `>`.
fn read_tag(src: &str, start: usize) -> (&str, usize) {
    let bytes = src.as_bytes();
    let mut quote: Option<u8> = None;
    let mut index = start + 1;
    while index < bytes.len() {
        let byte = bytes[index];
        match quote {
            Some(q) if byte == q => quote = None,
            Some(_) => {}
            None => {
                if byte == b'"' || byte == b'\'' {
                    quote = Some(byte);
                } else if byte == b'>' {
                    return (&src[start + 1..index], index + 1);
                }
            }
        }
        index += 1;
    }
    (&src[start + 1..], bytes.len())
}

fn parse_start_tag(body: &str) -> (String, Vec<(String, String)>) {
    let mut chars = body.char_indices();
    let mut name_end = body.len();
    for (offset, ch) in chars.by_ref() {
        if ch.is_whitespace() {
            name_end = offset;
            break;
        }
    }
    let name = body[..name_end].to_string();
    let mut attrs = Vec::new();
    let rest = &body[name_end..];

    let bytes = rest.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        let key_start = index;
        while index < bytes.len() && !bytes[index].is_ascii_whitespace() && bytes[index] != b'=' {
            index += 1;
        }
        if key_start == index {
            index += 1;
            continue;
        }
        let key = rest[key_start..index].to_string();
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index < bytes.len() && bytes[index] == b'=' {
            index += 1;
            while index < bytes.len() && bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            let quote = if index < bytes.len() && (bytes[index] == b'"' || bytes[index] == b'\'') {
                let q = bytes[index];
                index += 1;
                Some(q)
            } else {
                None
            };
            let value_start = index;
            match quote {
                Some(q) => {
                    while index < bytes.len() && bytes[index] != q {
                        index += 1;
                    }
                    let value = decode_entities(&rest[value_start..index]);
                    attrs.push((key, value));
                    index += 1;
                }
                None => {
                    while index < bytes.len() && !bytes[index].is_ascii_whitespace() {
                        index += 1;
                    }
                    let value = decode_entities(&rest[value_start..index]);
                    attrs.push((key, value));
                }
            }
        } else {
            attrs.push((key, String::new()));
        }
    }

    (name, attrs)
}

fn decode_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] != b'&' {
            // Copy one whole UTF-8 scalar, not one byte.
            let ch = text[index..].chars().next().unwrap_or('&');
            out.push(ch);
            index += ch.len_utf8();
            continue;
        }
        if let Some(semi) = text[index..].find(';').map(|o| index + o) {
            let entity = &text[index + 1..semi];
            let decoded = match entity {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some('\u{00a0}'),
                other => {
                    if let Some(hex) = other
                        .strip_prefix("#x")
                        .or_else(|| other.strip_prefix("#X"))
                    {
                        u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
                    } else if let Some(dec) = other.strip_prefix('#') {
                        dec.parse::<u32>().ok().and_then(char::from_u32)
                    } else {
                        None
                    }
                }
            };
            if let Some(ch) = decoded {
                out.push(ch);
                index = semi + 1;
                continue;
            }
        }
        out.push('&');
        index += 1;
    }
    out
}

// ─── Tests ─────────────────────────────────────────────────────
//
// Fixtures are real responses captured from a signed-in session (P4.0), so these
// tests fail if Apple's shape drifts rather than if someone's guess was wrong.

#[cfg(test)]
mod tests {
    use super::*;

    const LINE_TTML: &str = include_str!("../../tests/fixtures/lyrics-line.ttml");
    const PLAIN_TTML: &str = include_str!("../../tests/fixtures/lyrics-plain.ttml");

    #[test]
    fn parses_line_timed_lyrics() {
        let lyrics = parse_ttml(LINE_TTML);
        assert_eq!(lyrics.timing, LyricsTiming::Line);
        assert!(!lyrics.word_timed, "Apple shipped no spans for this track");
        assert_eq!(lyrics.language.as_deref(), Some("en"));
        assert_eq!(lyrics.leading_silence_ms, 160);
        assert_eq!(lyrics.lines.len(), 29, "all <p> elements survive");

        let first = &lyrics.lines[0];
        assert_eq!(first.text, "I know a place");
        assert_eq!(first.start_ms, 44_851);
        assert_eq!(first.end_ms, 50_917);
        assert_eq!(first.key.as_deref(), Some("L1"));

        // Times in `M:SS.mmm` form must land in the same space as bare seconds.
        let second = &lyrics.lines[1];
        assert_eq!(second.start_ms, 54_534);
        assert_eq!(second.end_ms, 61_557);
    }

    #[test]
    fn line_timed_lyrics_get_synthesized_word_segments() {
        let lyrics = parse_ttml(LINE_TTML);
        let line = &lyrics.lines[0];
        assert_eq!(line.segments.len(), 4, "I / know / a / place");

        // Segments cover the line window with no gap and no overlap.
        assert_eq!(line.segments[0].start_ms, line.start_ms);
        assert_eq!(line.segments.last().unwrap().end_ms, line.end_ms);
        for pair in line.segments.windows(2) {
            assert_eq!(pair[0].end_ms, pair[1].start_ms);
        }
        // Trailing spaces keep the rendered line readable.
        assert_eq!(line.segments[0].text, "I ");
        assert_eq!(line.segments[3].text, "place");
        // Joining the segments must reproduce the original text.
        let joined: String = line.segments.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(joined, "I know a place");
    }

    #[test]
    fn parses_unsynced_lyrics_without_timing() {
        let lyrics = parse_ttml(PLAIN_TTML);
        assert_eq!(lyrics.timing, LyricsTiming::None);
        assert!(!lyrics.word_timed);
        assert_eq!(lyrics.lines.len(), 49);
        assert!(lyrics
            .lines
            .iter()
            .all(|l| l.start_ms == 0 && l.end_ms == 0));
        assert_eq!(lyrics.lines[0].text, "You belong to nobody");
        // Every line still has paint-ready segments.
        assert_eq!(lyrics.lines[0].segments.len(), 4);
        assert!(lyrics.lines[0].segments.iter().all(|s| s.end_ms == 0));
    }

    #[test]
    fn apostrophes_are_preserved_from_the_real_payload() {
        // The captured document uses U+2019, which is not an XML entity.
        let lyrics = parse_ttml(PLAIN_TTML);
        assert!(lyrics.lines.iter().any(|l| l.text.contains('\u{2019}')));
    }

    #[test]
    fn parses_word_timed_spans_and_background_vocals() {
        let ttml = r#"<tt xmlns:itunes="x" xmlns:ttm="y" itunes:timing="Word"><body>
            <div><p begin="1.0" end="3.0" itunes:key="L1">
              <span begin="1.0" end="1.5">Hel</span><span begin="1.5" end="2.2">lo </span>
              <span begin="2.2" end="3.0" ttm:role="x-bg">(there)</span>
            </p></div></body></tt>"#;
        let lyrics = parse_ttml(ttml);
        assert_eq!(lyrics.timing, LyricsTiming::Word);
        assert!(lyrics.word_timed);

        let line = &lyrics.lines[0];
        assert_eq!(line.segments.len(), 3);
        assert_eq!(line.segments[0].text, "Hel");
        assert_eq!(line.segments[0].start_ms, 1_000);
        assert_eq!(line.segments[1].text, "lo ");
        assert_eq!(line.segments[2].text, "(there)");
        assert!(line.segments[2].background);
        // The <p> text is the concatenation, so search/unsynced display still works.
        assert_eq!(line.text, "Hello (there)");
    }

    #[test]
    fn decodes_xml_entities() {
        let ttml = r#"<tt itunes:timing="Line"><body><p begin="0" end="1">Rock &amp; Roll &#39;n&#39; &#x2019;til &lt;3</p></body></tt>"#;
        let lyrics = parse_ttml(ttml);
        assert_eq!(lyrics.lines[0].text, "Rock & Roll 'n' \u{2019}til <3");
    }

    #[test]
    fn keeps_text_around_spans_so_words_do_not_glue_together() {
        let ttml = r#"<tt itunes:timing="Line"><body><p begin="0" end="2">Hello <span>brave</span> new world</p></body></tt>"#;
        let lyrics = parse_ttml(ttml);
        assert_eq!(lyrics.lines[0].text, "Hello brave new world");
        // The untimed span is not usable for karaoke, so the line is synthesized.
        assert!(!lyrics.word_timed);
        let joined: String = lyrics.lines[0]
            .segments
            .iter()
            .map(|s| s.text.as_str())
            .collect();
        assert_eq!(joined, "Hello brave new world");
    }

    #[test]
    fn honors_explicit_line_breaks() {
        let ttml =
            r#"<tt itunes:timing="Line"><body><p begin="0" end="2">One<br/>Two</p></body></tt>"#;
        let lyrics = parse_ttml(ttml);
        assert_eq!(lyrics.lines[0].text, "One\nTwo");
    }

    #[test]
    fn empty_and_garbage_input_degrade_to_empty() {
        assert!(parse_ttml("").is_empty());
        assert!(parse_ttml("<tt></tt>").is_empty());
        assert!(parse_ttml("not xml at all").is_empty());
        assert!(parse_ttml("<tt><body><div></div></body></tt>").is_empty());
    }

    #[test]
    fn ignores_comments_and_cdata() {
        let ttml = r#"<tt itunes:timing="Line"><body><!-- <p>x</p> --><p begin="0" end="1"><![CDATA[a < b]]></p></body></tt>"#;
        let lyrics = parse_ttml(ttml);
        assert_eq!(lyrics.lines.len(), 1);
        assert_eq!(lyrics.lines[0].text, "a < b");
    }

    #[test]
    fn time_formats() {
        assert_eq!(parse_time("44.851"), Some(44_851));
        assert_eq!(parse_time("1:04.163"), Some(64_163));
        assert_eq!(parse_time("5:26.491"), Some(326_491));
        assert_eq!(parse_time("1:02:03.5"), Some(3_723_500));
        assert_eq!(parse_time("0"), Some(0));
        assert_eq!(parse_time(""), None);
        assert_eq!(parse_time("abc"), None);
    }

    #[test]
    fn model_serializes_camel_case_for_the_frontend() {
        let lyrics = parse_ttml(LINE_TTML);
        let json = serde_json::to_value(&lyrics).unwrap();
        assert!(json.get("wordTimed").is_some());
        assert!(json.get("leadingSilenceMs").is_some());
        let line = &json["lines"][0];
        assert!(line.get("startMs").is_some());
        assert!(line.get("segments").is_some());
    }
}
