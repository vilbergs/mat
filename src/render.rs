use std::rc::Rc;
use std::sync::LazyLock;

use pulldown_cmark::{
    Alignment, BlockQuoteKind, CodeBlockKind, Event, HeadingLevel, LinkType, MetadataBlockKind,
    Options as MdOptions, Parser, Tag, TagEnd,
};
use syntect::easy::HighlightLines;
use syntect::highlighting::FontStyle;
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;
use two_face::theme::LazyThemeSet;
use unicode_width::UnicodeWidthStr;

use crate::style::{Color, Painter, Style};
use crate::wrap::{Frag, Inline, Line, push_frag, width_of, wrap};

static SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(two_face::syntax::extra_newlines);
pub static THEMES: LazyLock<LazyThemeSet> = LazyLock::new(|| two_face::theme::extra().into());

pub struct Options {
    pub width: usize,
    pub painter: Painter,
    pub theme: String,
}

enum Container {
    Quote(Style),
    /// A list item or footnote: its marker is printed on the first line, then
    /// replaced by indentation of the same width.
    Item { marker: String, style: Style, used: bool },
}

struct Table {
    aligns: Vec<Alignment>,
    rows: Vec<Vec<Vec<Inline>>>,
    row: Vec<Vec<Inline>>,
}

struct Link {
    url: String,
    start: usize,
    show_url: bool,
}

pub fn render(markdown: &str, opts: &Options) -> String {
    let md_opts = MdOptions::ENABLE_TABLES
        | MdOptions::ENABLE_STRIKETHROUGH
        | MdOptions::ENABLE_TASKLISTS
        | MdOptions::ENABLE_FOOTNOTES
        | MdOptions::ENABLE_GFM
        | MdOptions::ENABLE_WIKILINKS
        | MdOptions::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | MdOptions::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS;
    let events: Vec<Event> = Parser::new_ext(markdown, md_opts).collect();
    let mut r = Renderer {
        opts,
        out: String::new(),
        containers: Vec::new(),
        inline: Vec::new(),
        styles: Vec::new(),
        lists: Vec::new(),
        links: Vec::new(),
        code: None,
        table: None,
        heading: None,
        gap: false,
        started: false,
    };
    for (i, event) in events.iter().enumerate() {
        r.event(event, &events[i + 1..]);
    }
    r.flush_tight();
    r.out
}

struct Renderer<'a> {
    opts: &'a Options,
    out: String,
    containers: Vec<Container>,
    inline: Vec<Inline>,
    styles: Vec<Style>,
    /// Per open list: the next number (ordered lists) and whether it's tight.
    lists: Vec<(Option<u64>, bool)>,
    links: Vec<Link>,
    code: Option<(String, String)>,
    table: Option<Table>,
    heading: Option<HeadingLevel>,
    /// A blank line is owed before the next block.
    gap: bool,
    started: bool,
}

impl Renderer<'_> {
    fn color(&self) -> bool {
        self.opts.painter.color
    }

    fn style(&self) -> Style {
        self.styles.last().cloned().unwrap_or_default()
    }

    fn push_style(&mut self, f: impl FnOnce(&mut Style)) {
        let mut style = self.style();
        f(&mut style);
        self.styles.push(style);
    }

    fn text(&mut self, text: &str, style: Style) {
        self.inline.push(Inline::Text(text.to_string(), style));
    }

    fn event(&mut self, event: &Event, rest: &[Event]) {
        if let Some((_, code)) = &mut self.code {
            match event {
                Event::Text(t) => return code.push_str(t),
                Event::End(TagEnd::CodeBlock | TagEnd::MetadataBlock(_)) => {}
                _ => return,
            }
        }
        match event {
            Event::Start(tag) => self.start(tag, rest),
            Event::End(tag) => self.end(tag),
            Event::Text(t) => self.text(t, self.style()),
            Event::Code(t) => {
                let mut style = self.style();
                style.fg = Some(Color::Ansi(3));
                let t = if self.color() { t.to_string() } else { format!("`{t}`") };
                self.text(&t, style);
            }
            Event::InlineMath(t) | Event::DisplayMath(t) => self.text(t, Style::fg(3)),
            Event::Html(t) | Event::InlineHtml(t) => {
                for (i, line) in t.trim_end_matches('\n').split('\n').enumerate() {
                    if i > 0 {
                        self.inline.push(Inline::Break);
                    }
                    self.text(line, Style::dim());
                }
            }
            Event::FootnoteReference(label) => self.text(&format!("[{label}]"), Style::fg(6)),
            Event::SoftBreak => self.text(" ", self.style()),
            Event::HardBreak => self.inline.push(Inline::Break),
            Event::Rule => {
                self.flush_tight();
                self.start_block();
                let rule = "─".repeat(self.avail());
                self.emit_line(vec![(rule, Style::dim())]);
                self.gap = true;
            }
            Event::TaskListMarker(done) => {
                if let Some(Container::Item { marker, style, .. }) = self.containers.last_mut() {
                    *marker = if *done { "☑ " } else { "☐ " }.into();
                    *style = Style::fg(if *done { 2 } else { 8 });
                }
            }
        }
    }

    fn start(&mut self, tag: &Tag, rest: &[Event]) {
        match tag {
            Tag::Paragraph | Tag::HtmlBlock => self.flush_tight(),
            Tag::Heading { level, .. } => {
                self.flush_tight();
                self.heading = Some(*level);
                let (color, bold) = match level {
                    HeadingLevel::H1 => (5, true),
                    HeadingLevel::H2 => (4, true),
                    HeadingLevel::H3 => (6, true),
                    HeadingLevel::H4 => (2, true),
                    HeadingLevel::H5 => (3, true),
                    HeadingLevel::H6 => (8, false),
                };
                self.push_style(|s| {
                    s.fg = Some(Color::Ansi(color));
                    s.bold = bold;
                });
            }
            Tag::BlockQuote(kind) => {
                self.flush_tight();
                self.start_block();
                let (label, color) = match kind {
                    None => ("", 8),
                    Some(BlockQuoteKind::Note) => ("Note", 4),
                    Some(BlockQuoteKind::Tip) => ("Tip", 2),
                    Some(BlockQuoteKind::Important) => ("Important", 5),
                    Some(BlockQuoteKind::Warning) => ("Warning", 3),
                    Some(BlockQuoteKind::Caution) => ("Caution", 1),
                };
                self.containers.push(Container::Quote(Style::fg(color)));
                if !label.is_empty() {
                    self.emit_line(vec![(label.into(), Style::fg(color).bold())]);
                }
            }
            Tag::CodeBlock(kind) => {
                self.flush_tight();
                let lang = match kind {
                    CodeBlockKind::Fenced(info) => info.split([' ', ',', '{']).next().unwrap_or(""),
                    CodeBlockKind::Indented => "",
                };
                self.code = Some((lang.to_string(), String::new()));
            }
            Tag::MetadataBlock(kind) => {
                let lang = match kind {
                    MetadataBlockKind::YamlStyle => "yaml",
                    MetadataBlockKind::PlusesStyle => "toml",
                };
                self.code = Some((lang.to_string(), String::new()));
            }
            Tag::List(start) => {
                self.flush_tight();
                self.start_block();
                // Loose lists wrap every item's text in a paragraph; tight ones don't.
                let tight = !matches!(rest.get(1), Some(Event::Start(Tag::Paragraph)));
                self.lists.push((*start, tight));
            }
            Tag::Item => {
                self.flush_tight();
                let depth = self.lists.len();
                let Some((number, tight)) = self.lists.last_mut() else { return };
                if *tight {
                    self.gap = false;
                }
                let marker = match number {
                    Some(n) => {
                        *n += 1;
                        format!("{}. ", *n - 1)
                    }
                    None => format!("{} ", ["•", "◦", "▪"][(depth - 1) % 3]),
                };
                let style = Style::fg(if number.is_some() { 6 } else { 5 });
                self.containers.push(Container::Item { marker, style, used: false });
            }
            Tag::FootnoteDefinition(label) => {
                self.flush_tight();
                self.start_block();
                let marker = format!("[{label}] ");
                self.containers.push(Container::Item { marker, style: Style::fg(6), used: false });
            }
            Tag::Table(aligns) => {
                self.flush_tight();
                self.table = Some(Table { aligns: aligns.clone(), rows: Vec::new(), row: Vec::new() });
            }
            Tag::TableHead | Tag::TableRow | Tag::TableCell => {}
            Tag::Emphasis => self.push_style(|s| s.italic = true),
            Tag::Strong => self.push_style(|s| s.bold = true),
            Tag::Strikethrough => self.push_style(|s| {
                s.strike = true;
                s.dim = true;
            }),
            Tag::Superscript | Tag::Subscript => self.push_style(|_| {}),
            Tag::Link { link_type, dest_url, .. } => {
                let wiki = matches!(link_type, LinkType::WikiLink { .. });
                let show_url = !wiki && !matches!(link_type, LinkType::Autolink | LinkType::Email);
                self.links.push(Link { url: dest_url.to_string(), start: self.inline.len(), show_url });
                let url: Option<Rc<str>> = (!wiki).then(|| dest_url.as_ref().into());
                self.push_style(|s| {
                    s.fg = Some(Color::Ansi(4));
                    s.underline = true;
                    s.link = url;
                });
            }
            Tag::Image { dest_url, .. } => {
                self.text("[image: ", Style::dim());
                let url: Rc<str> = dest_url.as_ref().into();
                self.push_style(|s| {
                    s.dim = true;
                    s.italic = true;
                    s.link = Some(url);
                });
            }
            Tag::DefinitionList | Tag::DefinitionListTitle | Tag::DefinitionListDefinition => {
                self.flush_tight()
            }
        }
    }

    fn end(&mut self, tag: &TagEnd) {
        match tag {
            TagEnd::Paragraph | TagEnd::HtmlBlock => self.flush_block(),
            TagEnd::Heading(_) => {
                self.styles.pop();
                self.flush_block();
            }
            TagEnd::BlockQuote(_) => {
                self.flush_tight();
                self.containers.pop();
                self.gap = true;
            }
            TagEnd::CodeBlock | TagEnd::MetadataBlock(_) => {
                if let Some((lang, code)) = self.code.take() {
                    self.code_block(&lang, &code);
                }
            }
            TagEnd::List(_) => {
                self.flush_tight();
                self.lists.pop();
                self.gap = true;
            }
            TagEnd::Item | TagEnd::FootnoteDefinition => {
                self.flush_tight();
                if let Some(Container::Item { used: false, .. }) = self.containers.last() {
                    self.emit_line(Vec::new());
                }
                self.containers.pop();
                if matches!(tag, TagEnd::FootnoteDefinition) {
                    self.gap = true;
                }
            }
            TagEnd::TableCell => {
                let cell = std::mem::take(&mut self.inline);
                if let Some(table) = &mut self.table {
                    table.row.push(cell);
                }
            }
            TagEnd::TableHead | TagEnd::TableRow => {
                if let Some(table) = &mut self.table {
                    let row = std::mem::take(&mut table.row);
                    table.rows.push(row);
                }
            }
            TagEnd::Table => {
                if let Some(table) = self.table.take() {
                    self.render_table(table);
                }
            }
            TagEnd::Emphasis
            | TagEnd::Strong
            | TagEnd::Strikethrough
            | TagEnd::Superscript
            | TagEnd::Subscript => {
                self.styles.pop();
            }
            TagEnd::Link => {
                self.styles.pop();
                let Some(link) = self.links.pop() else { return };
                let text: String = self.inline[link.start..]
                    .iter()
                    .filter_map(|i| match i {
                        Inline::Text(t, _) => Some(t.as_str()),
                        Inline::Break => None,
                    })
                    .collect();
                if link.show_url && !link.url.is_empty() && text != link.url {
                    self.text(&format!(" ({})", link.url), Style::dim());
                }
            }
            TagEnd::Image => {
                self.styles.pop();
                self.text("]", Style::dim());
            }
            TagEnd::DefinitionList | TagEnd::DefinitionListTitle | TagEnd::DefinitionListDefinition => {
                self.flush_block()
            }
        }
    }

    // ---- output -------------------------------------------------------------

    /// Columns left for content once the quote bars and list indents are drawn.
    fn avail(&self) -> usize {
        let used: usize = self
            .containers
            .iter()
            .map(|c| match c {
                Container::Quote(_) => 2,
                Container::Item { marker, .. } => marker.width(),
            })
            .sum();
        self.opts.width.saturating_sub(used).max(10)
    }

    fn prefix(&mut self, consume: bool) -> Line {
        let mut line = Vec::new();
        for c in &mut self.containers {
            match c {
                Container::Quote(style) => push_frag(&mut line, "│ ", style),
                Container::Item { marker, style, used } => {
                    if *used || !consume {
                        push_frag(&mut line, &" ".repeat(marker.width()), &Style::default());
                    } else {
                        push_frag(&mut line, marker, style);
                        *used = true;
                    }
                }
            }
        }
        line
    }

    fn emit_line(&mut self, content: Line) {
        let mut line = self.prefix(true);
        line.extend(content);
        self.write(&line);
    }

    fn write(&mut self, line: &[Frag]) {
        let mut text = String::new();
        for (t, style) in line {
            self.opts.painter.paint(&mut text, t, style);
        }
        self.out.push_str(text.trim_end_matches(' '));
        self.out.push('\n');
        self.started = true;
    }

    /// Emit the owed blank line, if any, before a new block.
    fn start_block(&mut self) {
        if self.gap && self.started {
            let line = self.prefix(false);
            self.write(&line);
        }
        self.gap = false;
    }

    /// Text directly inside a tight list item: no blank line around it.
    fn flush_tight(&mut self) {
        if self.inline.is_empty() {
            return;
        }
        let inline = std::mem::take(&mut self.inline);
        self.start_block();
        for line in wrap(&inline, self.avail()) {
            self.emit_line(line);
        }
    }

    fn flush_block(&mut self) {
        let inline = std::mem::take(&mut self.inline);
        let heading = self.heading.take();
        if inline.is_empty() {
            return;
        }
        self.start_block();
        let lines = wrap(&inline, self.avail());
        let widest = lines.iter().map(|l| width_of(l)).max().unwrap_or(0);
        for line in lines {
            self.emit_line(line);
        }
        let underline = match heading {
            Some(HeadingLevel::H1) => Some(("━", 5)),
            Some(HeadingLevel::H2) => Some(("─", 4)),
            _ => None,
        };
        if let Some((ch, color)) = underline {
            self.emit_line(vec![(ch.repeat(widest), Style::fg(color))]);
        }
        self.gap = true;
    }

    fn code_block(&mut self, lang: &str, code: &str) {
        self.start_block();
        self.gap = true;
        let code = code.replace('\t', "    ");
        if !self.color() {
            for line in code.lines() {
                self.emit_line(vec![(format!("    {line}"), Style::default())]);
            }
            return;
        }
        let theme = THEMES.get(&self.opts.theme).or_else(|| THEMES.get("ansi")).expect("bundled theme");
        let syntax = SYNTAXES
            .find_syntax_by_token(lang)
            .unwrap_or_else(|| SYNTAXES.find_syntax_plain_text());
        let mut hl = HighlightLines::new(syntax, theme);
        let lines: Vec<Line> = LinesWithEndings::from(&code)
            .map(|line| {
                let mut frags = Vec::new();
                match hl.highlight_line(line, &SYNTAXES) {
                    Ok(ranges) => {
                        for (st, text) in ranges {
                            let text = text.trim_end_matches(['\n', '\r']);
                            if !text.is_empty() {
                                push_frag(&mut frags, text, &syntect_style(st));
                            }
                        }
                    }
                    Err(_) => push_frag(&mut frags, line.trim_end(), &Style::default()),
                }
                frags
            })
            .collect();

        // A box hugging the widest line, at most the available width. Lines too
        // long for it run past the right edge rather than being wrapped, so the
        // code survives copy and paste.
        let border = Style::fg(8);
        let label = if lang.is_empty() { String::new() } else { format!("─ {lang} ") };
        let widest = lines.iter().map(|l| width_of(l)).max().unwrap_or(0);
        let inner = widest.max(label.width()).min(self.avail().saturating_sub(4));
        let top = format!("╭{label}{}╮", "─".repeat((inner + 2).saturating_sub(label.width())));
        self.emit_line(vec![(top, border.clone())]);
        for content in lines {
            let w = width_of(&content);
            let mut line = vec![("│ ".to_string(), border.clone())];
            line.extend(content);
            if w <= inner {
                push_frag(&mut line, &" ".repeat(inner - w + 1), &Style::default());
                push_frag(&mut line, "│", &border);
            }
            self.emit_line(line);
        }
        self.emit_line(vec![(format!("╰{}╯", "─".repeat(inner + 2)), border)]);
    }

    fn render_table(&mut self, table: Table) {
        self.start_block();
        self.gap = true;
        let cols = table.rows.iter().map(Vec::len).max().unwrap_or(0);
        if cols == 0 {
            return;
        }
        let natural = |cell: &[Inline]| {
            cell.iter()
                .map(|i| match i {
                    Inline::Text(t, _) => t.width(),
                    Inline::Break => 1,
                })
                .sum::<usize>()
        };
        let mut widths = vec![1; cols];
        for row in &table.rows {
            for (i, cell) in row.iter().enumerate() {
                widths[i] = widths[i].max(natural(cell));
            }
        }
        // Shrink the widest column until the table fits; its cells wrap instead.
        let budget = self.avail().saturating_sub(3 * cols + 1);
        while widths.iter().sum::<usize>() > budget {
            let (i, &w) = widths.iter().enumerate().max_by_key(|(_, w)| **w).unwrap();
            if w <= 3 {
                break;
            }
            widths[i] = w - 1;
        }

        let border = Style::fg(8);
        let rule = |l: &str, m: &str, r: &str| {
            let inner: Vec<String> = widths.iter().map(|w| "─".repeat(w + 2)).collect();
            vec![(format!("{l}{}{r}", inner.join(m)), border.clone())]
        };
        self.emit_line(rule("┌", "┬", "┐"));
        for (r, row) in table.rows.iter().enumerate() {
            let wrapped: Vec<Vec<Line>> = (0..cols)
                .map(|i| {
                    let mut lines = wrap(row.get(i).map_or(&[][..], |c| c), widths[i]);
                    if r == 0 {
                        for (_, style) in lines.iter_mut().flatten() {
                            style.bold = true;
                        }
                    }
                    lines
                })
                .collect();
            let height = wrapped.iter().map(Vec::len).max().unwrap_or(1);
            for l in 0..height {
                let mut line = vec![("│".to_string(), border.clone())];
                for (i, cell) in wrapped.iter().enumerate() {
                    let content = cell.get(l).cloned().unwrap_or_default();
                    let pad = widths[i].saturating_sub(width_of(&content));
                    let (left, right) = match table.aligns.get(i) {
                        Some(Alignment::Right) => (pad, 0),
                        Some(Alignment::Center) => (pad / 2, pad - pad / 2),
                        _ => (0, pad),
                    };
                    push_frag(&mut line, &" ".repeat(left + 1), &Style::default());
                    line.extend(content);
                    push_frag(&mut line, &" ".repeat(right + 1), &Style::default());
                    push_frag(&mut line, "│", &border);
                }
                self.emit_line(line);
            }
            if r == 0 && table.rows.len() > 1 {
                self.emit_line(rule("├", "┼", "┤"));
            }
        }
        self.emit_line(rule("└", "┴", "┘"));
    }
}

/// Convert a syntect style. Like bat, the "ansi" themes encode palette indexes
/// in the red channel with alpha 0, and alpha 1 means "terminal default".
fn syntect_style(st: syntect::highlighting::Style) -> Style {
    let c = st.foreground;
    let fg = match c.a {
        0 => Some(Color::Ansi(c.r)),
        1 => None,
        _ => Some(Color::Rgb(c.r, c.g, c.b)),
    };
    Style {
        fg,
        bold: st.font_style.contains(FontStyle::BOLD),
        italic: st.font_style.contains(FontStyle::ITALIC),
        underline: st.font_style.contains(FontStyle::UNDERLINE),
        ..Default::default()
    }
}
