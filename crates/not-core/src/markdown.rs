//! Styling ranges for live markdown editing: the editors keep the markdown text as is and style it
//! in place (headings bigger, `**bold**` bold, markers dimmed), so both apps render notes the same
//! way and nothing is lost converting to and from a rich-text model.
//!
//! Offsets are UTF-16 code units, which is what `NSString` and Kotlin strings index by.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    /// Level 1–3; deeper headings use 3.
    Heading(u8),
    Bold,
    Italic,
    Code,
    Strike,
    Quote,
    /// A list bullet or number.
    Bullet,
    /// `[ ]` / `[x]` of a checklist item.
    Checkbox {
        checked: bool,
    },
    /// The text of a checked checklist item.
    Done,
    /// Markup characters (`#`, `**`, backticks…), drawn dimmed.
    Syntax,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: u32,
    pub end: u32,
    pub style: Style,
}

/// An image line: `![alt](not-image:<id>)` alone on its line (spaces around allowed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageRef {
    /// UTF-16 range of the line, without its newline.
    pub start: u32,
    pub end: u32,
    pub id: String,
}

/// The image lines of a note, outside code fences, in order.
pub fn images(text: &str) -> Vec<ImageRef> {
    let mut out = Vec::new();
    let mut offset = 0u32;
    let mut in_fence = false;
    for line in text.split('\n') {
        let len = line.encode_utf16().count() as u32;
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_fence = !in_fence;
        } else if !in_fence && let Some(id) = image_line(trimmed) {
            out.push(ImageRef {
                start: offset,
                end: offset + len,
                id: id.to_owned(),
            });
        }
        offset += len + 1;
    }
    out
}

/// What a task block line (`![](not-tasks:…)`) shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskSource {
    /// A whole list (or [`crate::INBOX`]): `not-tasks:list:<id>`.
    List(String),
    /// Picked tasks with their subtasks: `not-tasks:<id>,<id>`.
    Tasks(Vec<String>),
}

/// A task block line of a note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskBlockRef {
    /// UTF-16 range of the line, without its newline.
    pub start: u32,
    pub end: u32,
    pub source: TaskSource,
}

const TASKS_SCHEME: &str = "not-tasks:";

/// The line for a task block.
pub fn task_block_markup(source: &TaskSource) -> String {
    match source {
        TaskSource::List(id) => format!("![]({TASKS_SCHEME}list:{id})"),
        TaskSource::Tasks(ids) => format!("![]({TASKS_SCHEME}{})", ids.join(",")),
    }
}

/// The task block lines of a note, outside code fences, in order.
pub fn task_blocks(text: &str) -> Vec<TaskBlockRef> {
    let mut out = Vec::new();
    let mut offset = 0u32;
    let mut in_fence = false;
    for line in text.split('\n') {
        let len = line.encode_utf16().count() as u32;
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_fence = !in_fence;
        } else if !in_fence && let Some(source) = task_block_line(trimmed) {
            out.push(TaskBlockRef {
                start: offset,
                end: offset + len,
                source,
            });
        }
        offset += len + 1;
    }
    out
}

fn task_block_line(line: &str) -> Option<TaskSource> {
    let rest = line.strip_prefix("![")?;
    let (_, rest) = rest.split_once("](")?;
    let target = rest.strip_suffix(')')?.strip_prefix(TASKS_SCHEME)?;
    let valid = |id: &str| {
        !id.is_empty()
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '@' || c == '-' || c == '_')
    };
    if let Some(id) = target.strip_prefix("list:") {
        return valid(id).then(|| TaskSource::List(id.to_owned()));
    }
    let ids: Vec<String> = target.split(',').map(str::to_owned).collect();
    ids.iter().all(|id| valid(id)).then_some(TaskSource::Tasks(ids))
}

/// The image line for `id`, as the editors insert it.
pub fn image_markup(id: &str) -> String {
    format!("![]({}{id})", not_protocol::IMAGE_SCHEME)
}

fn image_line(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("![")?;
    let (_, rest) = rest.split_once("](")?;
    let id = rest.strip_suffix(')')?.strip_prefix(not_protocol::IMAGE_SCHEME)?;
    not_protocol::is_image_id(id).then_some(id)
}

pub fn spans(text: &str) -> Vec<Span> {
    let mut out = Vec::new();
    let mut offset = 0u32;
    let mut in_fence = false;
    for line in text.split('\n') {
        let chars: Vec<char> = line.chars().collect();
        // UTF-16 offset of each char, plus the end.
        let mut at = Vec::with_capacity(chars.len() + 1);
        let mut o = offset;
        for c in &chars {
            at.push(o);
            o += c.len_utf16() as u32;
        }
        at.push(o);
        let mut add = |from: usize, to: usize, style| {
            if to > from {
                out.push(Span {
                    start: at[from],
                    end: at[to],
                    style,
                });
            }
        };

        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            add(0, chars.len(), Style::Syntax);
            in_fence = !in_fence;
        } else if in_fence {
            add(0, chars.len(), Style::Code);
        } else {
            let inline_from = block(&chars, &mut add);
            inline(&chars, inline_from, &mut add);
        }
        offset = o + 1; // '\n'
    }
    out
}

/// Styles block markers at the start of a line. Returns where inline markup may start.
fn block(chars: &[char], add: &mut impl FnMut(usize, usize, Style)) -> usize {
    let n = chars.len();
    let hashes = chars.iter().take_while(|&&c| c == '#').count();
    if (1..=6).contains(&hashes) && chars.get(hashes) == Some(&' ') {
        add(0, hashes + 1, Style::Syntax);
        add(0, n, Style::Heading(hashes.min(3) as u8));
        return hashes + 1;
    }
    if chars.first() == Some(&'>') {
        let marker = if chars.get(1) == Some(&' ') { 2 } else { 1 };
        add(0, marker, Style::Syntax);
        add(0, n, Style::Quote);
        return marker;
    }
    let indent = chars.iter().take_while(|c| **c == ' ' || **c == '\t').count();
    let rest = &chars[indent..];
    if rest.len() >= 2 && matches!(rest[0], '-' | '*' | '+') && rest[1] == ' ' {
        add(indent, indent + 1, Style::Bullet);
        let item = indent + 2;
        if let ['[', mark @ (' ' | 'x' | 'X'), ']', ..] = chars[item..] {
            let checked = mark != ' ';
            let after = (item + 3 + usize::from(chars.get(item + 3) == Some(&' '))).min(n);
            add(item, item + 3, Style::Checkbox { checked });
            if checked {
                add(after, n, Style::Done);
            }
            return after;
        }
        return item;
    }
    let digits = rest.iter().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 && rest.get(digits) == Some(&'.') && rest.get(digits + 1) == Some(&' ') {
        add(indent, indent + digits + 1, Style::Bullet);
        return indent + digits + 2;
    }
    0
}

/// `code`, **bold**, *italic* / _italic_, ~~strike~~ — within one line, no nesting of the same kind.
fn inline(chars: &[char], from: usize, add: &mut impl FnMut(usize, usize, Style)) {
    let n = chars.len();
    let find = |from: usize, delim: &[char]| -> Option<usize> {
        (from..=n.saturating_sub(delim.len())).find(|&j| chars[j..].starts_with(delim))
    };
    let mut i = from;
    while i < n {
        let (delim, style): (&[char], Style) = match chars[i] {
            '`' => (&['`'], Style::Code),
            '*' if chars.get(i + 1) == Some(&'*') => (&['*', '*'], Style::Bold),
            '~' if chars.get(i + 1) == Some(&'~') => (&['~', '~'], Style::Strike),
            '*' => (&['*'], Style::Italic),
            // Snake_case words aren't emphasis.
            '_' if i == 0 || !chars[i - 1].is_alphanumeric() => (&['_'], Style::Italic),
            _ => {
                i += 1;
                continue;
            }
        };
        let open_end = i + delim.len();
        let opens = chars.get(open_end).is_some_and(|c| !c.is_whitespace());
        let close = opens.then(|| find(open_end + 1, delim)).flatten();
        // `***` closing bold around italic: the bold marker is the last two.
        let close = close.map(|j| {
            if delim.len() == 2 && chars.get(j + 2) == Some(&delim[0]) {
                j + 1
            } else {
                j
            }
        });
        let close = close.filter(|&j| {
            // Closing markers hug the text, and `*` mustn't be half of a `**`.
            !chars[j - 1].is_whitespace() && (style != Style::Italic || chars.get(j + 1) != Some(&delim[0]))
        });
        let Some(close) = close else {
            i = open_end;
            continue;
        };
        add(i, open_end, Style::Syntax);
        add(open_end, close, style);
        add(close, close + delim.len(), Style::Syntax);
        if style == Style::Bold || style == Style::Strike {
            // Italic or code inside bold text.
            inline(&chars[..close], open_end, add);
        }
        i = close + delim.len();
    }
}

#[cfg(test)]
mod tests {
    use super::Style::*;
    use super::*;

    fn styled(text: &str) -> Vec<(String, Style)> {
        let units: Vec<u16> = text.encode_utf16().collect();
        spans(text)
            .into_iter()
            .map(|s| {
                (
                    String::from_utf16(&units[s.start as usize..s.end as usize]).unwrap(),
                    s.style,
                )
            })
            .collect()
    }

    #[test]
    fn blocks() {
        assert_eq!(
            styled("## Plan\n- [x] done\n1. one\n> quote"),
            [
                ("## ".into(), Syntax),
                ("## Plan".into(), Heading(2)),
                ("-".into(), Bullet),
                ("[x]".into(), Checkbox { checked: true }),
                ("done".into(), Done),
                ("1.".into(), Bullet),
                ("> ".into(), Syntax),
                ("> quote".into(), Quote),
            ]
        );
    }

    #[test]
    fn inline_markup() {
        assert_eq!(
            styled("a **b *c*** `d` snake_case ~~e~~ * f*"),
            [
                ("**".into(), Syntax),
                ("b *c*".into(), Bold),
                ("**".into(), Syntax),
                ("*".into(), Syntax),
                ("c".into(), Italic),
                ("*".into(), Syntax),
                ("`".into(), Syntax),
                ("d".into(), Code),
                ("`".into(), Syntax),
                ("~~".into(), Syntax),
                ("e".into(), Strike),
                ("~~".into(), Syntax),
            ]
        );
    }

    #[test]
    fn offsets_are_utf16() {
        let s = spans("😀 **x**");
        assert_eq!((s[1].start, s[1].end, s[1].style), (5, 6, Bold));
    }

    #[test]
    fn fences() {
        assert_eq!(
            styled("```\n**no**\n```\n*yes*"),
            [
                ("```".into(), Syntax),
                ("**no**".into(), Code),
                ("```".into(), Syntax),
                ("*".into(), Syntax),
                ("yes".into(), Italic),
                ("*".into(), Syntax),
            ]
        );
    }
}

#[cfg(test)]
mod image_tests {
    use super::*;

    #[test]
    fn image_lines_are_found_outside_code() {
        let id = format!("{}.jpg", "ab".repeat(32));
        let text = format!(
            "Trip\n{}\n  ![view](not-image:{id})  \n```\n![](not-image:{id})\n```\n![](http://x/y.jpg)",
            image_markup(&id)
        );
        let found = images(&text);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].id, id);
        assert_eq!(found[0].start, 5);
        assert_eq!(found[0].end, 5 + image_markup(&id).len() as u32);
        assert!(
            images(&format!("see ![](not-image:{id})")).is_empty(),
            "only whole lines"
        );
    }
}

#[cfg(test)]
mod block_tests {
    use super::*;

    #[test]
    fn block_lines_round_trip() {
        for source in [
            TaskSource::List("12@345".into()),
            TaskSource::List("inbox".into()),
            TaskSource::Tasks(vec!["1@2".into(), "3@4".into()]),
        ] {
            let text = format!("Note\n{}\nafter", task_block_markup(&source));
            let found = task_blocks(&text);
            assert_eq!(found.len(), 1);
            assert_eq!(found[0].source, source);
            assert_eq!(found[0].start, 5);
            assert_eq!(found[0].end, 5 + task_block_markup(&source).len() as u32);
        }
    }

    #[test]
    fn only_whole_valid_lines_outside_code_are_blocks() {
        let line = task_block_markup(&TaskSource::List("1@2".into()));
        assert!(task_blocks(&format!("see {line}")).is_empty(), "inside text");
        assert!(task_blocks(&format!("```\n{line}\n```")).is_empty(), "in code");
        assert!(task_blocks("![](not-tasks:)").is_empty());
        assert!(task_blocks("![](not-tasks:list:)").is_empty());
        assert!(task_blocks("![](not-tasks:1@2,,3@4)").is_empty());
        assert!(task_blocks("![](not-tasks:1@2) x").is_empty());
        assert!(task_blocks("![](not-tasks:<script>)").is_empty());
        assert_eq!(
            task_blocks(&format!("  {line}  ")).len(),
            1,
            "spaces around are fine"
        );
    }
}
