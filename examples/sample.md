---
title: Sample
tags: [demo]
---

# mat — cat for markdown

A paragraph with **bold**, *italic*, ~~struck~~, `inline code` and a [link](https://example.com), plus an autolink <https://rust-lang.org> and a [[Wiki Link]]. This sentence is long enough that it should wrap nicely across multiple lines in a narrow terminal.

## Lists

- First item
- Second item with a fairly long line of text that needs to wrap onto the next line properly
  - Nested item
    - Deeper
- [ ] Todo
- [x] Done

1. One
2. Two
3. Three

> A plain quote that also goes on for a bit so we can see the bar repeated on wrapped lines.
>
> - with a list inside

> [!WARNING]
> Careful now.

### Code

```rust
fn main() {
    let name = "world";
    println!("hello, {name}!"); // comment
}
```

| Name | Age | City |
|:-----|----:|:----:|
| Alice | 31 | Göteborg |
| Bob | 4 | Stockholm |

---

Footnote ref[^1].

[^1]: The footnote.

![alt text](img.png)
