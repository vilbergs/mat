# mat

`cat` for Markdown: pretty-prints Markdown in the terminal.

```sh
mat README.md
mat notes/*.md
curl -s https://example.com/doc.md | mat
```

## What it renders

- Headings, **bold**, *italic*, ~~strikethrough~~, `inline code`
- Links as clickable OSC 8 hyperlinks, with the URL shown after the text
- Nested bullet, numbered and task lists
- Blockquotes and GitHub alerts (`> [!NOTE]`, `> [!WARNING]`, …)
- Fenced code blocks with syntax highlighting (bat's syntaxes and themes, via [two-face](https://crates.io/crates/two-face))
- Tables with alignment, and cell wrapping when they don't fit
- Footnotes, images (alt text), horizontal rules, front matter, `[[wiki links]]`

Text wraps to the terminal width, capped at 100 columns. The colors come from
your terminal's 16-color palette, so the output follows your terminal theme. The
default code theme, `ansi`, does the same.

## Options

```
-w, --width <WIDTH>   Wrap width in columns [default: terminal width, at most 100]
    --color <WHEN>    auto | always | never  [default: auto]
-t, --theme <THEME>   Code highlighting theme [default: ansi]
    --list-themes     List the available code themes
```

Colors are turned off when stdout isn't a terminal or `NO_COLOR` is set. Use
`--color always` when piping into `less -R`.

## Install

```sh
cargo install --git https://github.com/vilbergs/mat
```
