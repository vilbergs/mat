use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::style::Style;

#[derive(Clone, Debug)]
pub enum Inline {
    Text(String, Style),
    Break,
}

pub type Frag = (String, Style);
pub type Line = Vec<Frag>;

pub fn width_of(line: &[Frag]) -> usize {
    line.iter().map(|(t, _)| t.width()).sum()
}

pub fn push_frag(line: &mut Line, text: &str, style: &Style) {
    match line.last_mut() {
        Some((t, s)) if s == style => t.push_str(text),
        _ => line.push((text.to_string(), style.clone())),
    }
}

enum Tok {
    Word(Line, usize),
    Space(Style),
    Break,
}

fn tokenize(inlines: &[Inline]) -> Vec<Tok> {
    let mut toks = Vec::new();
    let mut word: Line = Vec::new();
    let mut word_w = 0;
    let flush = |toks: &mut Vec<Tok>, word: &mut Line, word_w: &mut usize| {
        if !word.is_empty() {
            toks.push(Tok::Word(std::mem::take(word), *word_w));
            *word_w = 0;
        }
    };
    for inline in inlines {
        match inline {
            Inline::Text(text, style) => {
                for ch in text.chars() {
                    if ch.is_whitespace() {
                        flush(&mut toks, &mut word, &mut word_w);
                        if !matches!(toks.last(), Some(Tok::Space(_))) {
                            toks.push(Tok::Space(style.clone()));
                        }
                    } else {
                        let mut buf = [0; 4];
                        push_frag(&mut word, ch.encode_utf8(&mut buf), style);
                        word_w += ch.width().unwrap_or(0);
                    }
                }
            }
            Inline::Break => {
                flush(&mut toks, &mut word, &mut word_w);
                toks.push(Tok::Break);
            }
        }
    }
    flush(&mut toks, &mut word, &mut word_w);
    toks
}

/// Greedy word wrap of styled text into lines no wider than `width`.
pub fn wrap(inlines: &[Inline], width: usize) -> Vec<Line> {
    let width = width.max(1);
    let mut lines: Vec<Line> = vec![Vec::new()];
    let mut cur = 0;
    let mut space: Option<Style> = None;
    for tok in tokenize(inlines) {
        match tok {
            Tok::Space(style) => {
                if cur > 0 {
                    space = Some(style);
                }
            }
            Tok::Break => {
                lines.push(Vec::new());
                cur = 0;
                space = None;
            }
            Tok::Word(frags, w) => {
                let gap = usize::from(space.is_some());
                if cur > 0 && cur + gap + w > width {
                    lines.push(Vec::new());
                    cur = 0;
                    space = None;
                }
                let line = lines.last_mut().unwrap();
                if let Some(style) = space.take() {
                    push_frag(line, " ", &style);
                    cur += 1;
                }
                if w <= width - cur {
                    for (text, style) in &frags {
                        push_frag(line, text, style);
                    }
                    cur += w;
                    continue;
                }
                // A word longer than the whole line: break it wherever it overflows.
                for (text, style) in &frags {
                    for ch in text.chars() {
                        let cw = ch.width().unwrap_or(0);
                        if cur + cw > width && cur > 0 {
                            lines.push(Vec::new());
                            cur = 0;
                        }
                        let mut buf = [0; 4];
                        push_frag(lines.last_mut().unwrap(), ch.encode_utf8(&mut buf), style);
                        cur += cw;
                    }
                }
            }
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(lines: &[Line]) -> Vec<String> {
        lines.iter().map(|l| l.iter().map(|(t, _)| t.as_str()).collect()).collect()
    }

    fn text(s: &str) -> Inline {
        Inline::Text(s.into(), Style::default())
    }

    #[test]
    fn wraps_at_word_boundaries() {
        let lines = wrap(&[text("the quick brown fox jumps")], 10);
        assert_eq!(plain(&lines), ["the quick", "brown fox", "jumps"]);
    }

    #[test]
    fn keeps_styled_word_parts_together() {
        let bold = Style::default().bold();
        let inlines = [text("aaaa "), Inline::Text("bb".into(), bold), text("cc dd")];
        assert_eq!(plain(&wrap(&inlines, 6)), ["aaaa", "bbcc", "dd"]);
    }

    #[test]
    fn splits_overlong_words() {
        assert_eq!(plain(&wrap(&[text("abcdefgh")], 3)), ["abc", "def", "gh"]);
    }

    #[test]
    fn honours_hard_breaks() {
        let lines = wrap(&[text("a"), Inline::Break, text("b")], 80);
        assert_eq!(plain(&lines), ["a", "b"]);
    }
}
