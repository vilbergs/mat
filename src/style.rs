use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Color {
    /// One of the terminal's palette colors, so output follows the terminal theme.
    Ansi(u8),
    Rgb(u8, u8, u8),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Style {
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub link: Option<Rc<str>>,
}

impl Style {
    pub fn fg(color: u8) -> Self {
        Style { fg: Some(Color::Ansi(color)), ..Default::default() }
    }

    pub fn dim() -> Self {
        Style { dim: true, ..Default::default() }
    }

    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }
}

pub struct Painter {
    pub color: bool,
    pub truecolor: bool,
}

impl Painter {
    pub fn paint(&self, out: &mut String, text: &str, style: &Style) {
        if !self.color {
            out.push_str(text);
            return;
        }
        if let Some(url) = &style.link {
            out.push_str(&format!("\x1b]8;;{url}\x1b\\"));
        }
        let mut codes: Vec<String> = Vec::new();
        if style.bold {
            codes.push("1".into());
        }
        if style.dim {
            codes.push("2".into());
        }
        if style.italic {
            codes.push("3".into());
        }
        if style.underline {
            codes.push("4".into());
        }
        if style.strike {
            codes.push("9".into());
        }
        if let Some(c) = style.fg {
            codes.push(self.color_code(c, false));
        }
        if let Some(c) = style.bg {
            codes.push(self.color_code(c, true));
        }
        if codes.is_empty() {
            out.push_str(text);
        } else {
            out.push_str(&format!("\x1b[{}m{text}\x1b[0m", codes.join(";")));
        }
        if style.link.is_some() {
            out.push_str("\x1b]8;;\x1b\\");
        }
    }

    fn color_code(&self, color: Color, bg: bool) -> String {
        match color {
            Color::Ansi(n) if n < 8 => format!("{}", if bg { 40 } else { 30 } + n),
            Color::Ansi(n) if n < 16 => format!("{}", if bg { 100 } else { 90 } + n - 8),
            Color::Ansi(n) => format!("{};5;{n}", if bg { 48 } else { 38 }),
            Color::Rgb(r, g, b) if self.truecolor => {
                format!("{};2;{r};{g};{b}", if bg { 48 } else { 38 })
            }
            Color::Rgb(r, g, b) => format!("{};5;{}", if bg { 48 } else { 38 }, rgb_to_256(r, g, b)),
        }
    }
}

/// Nearest color in the xterm 6x6x6 cube, for terminals without truecolor.
fn rgb_to_256(r: u8, g: u8, b: u8) -> u8 {
    let level = |v: u8| if v < 48 { 0 } else if v < 115 { 1 } else { (v - 35) / 40 };
    16 + 36 * level(r) + 6 * level(g) + level(b)
}
