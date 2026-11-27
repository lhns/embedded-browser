//! A small, tolerant HTML tree builder for the stub DOM.
//!
//! It is not a spec parser (the real engine uses html5ever); it handles what the corpus pages
//! and `innerHTML` fragments need: elements, attributes, text, comments, void and raw-text
//! elements, entities and the common implied end tags.

#[derive(Debug, PartialEq)]
pub enum Node {
    Element {
        tag: String,
        attrs: Vec<(String, String)>,
        children: Vec<Node>,
    },
    Text(String),
    Comment(String),
}

const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track",
    "wbr",
];
const RAW_TEXT: &[&str] = &["script", "style", "textarea", "title"];

/// Tags whose start tag implicitly closes an open element of the listed kinds.
fn closes(open: &str, new: &str) -> bool {
    match new {
        "li" => open == "li",
        "option" => open == "option",
        "tr" => matches!(open, "tr" | "td" | "th"),
        "td" | "th" => matches!(open, "td" | "th"),
        "p" | "div" | "ul" | "ol" | "table" | "form" | "h1" | "h2" | "h3" | "pre" => open == "p",
        _ => false,
    }
}

/// An open element while building: tag, attributes, children so far.
type Open = (String, Vec<(String, String)>, Vec<Node>);

pub fn parse(html: &str) -> Vec<Node> {
    // Stack of open elements: (tag, attrs, children). Index 0 is the synthetic root.
    let mut stack: Vec<Open> = vec![(String::new(), Vec::new(), Vec::new())];
    let b = html.as_bytes();
    let mut i = 0;
    let mut text_start = 0;

    fn flush_text(stack: &mut [Open], s: &str) {
        if !s.is_empty() {
            stack
                .last_mut()
                .unwrap()
                .2
                .push(Node::Text(decode_entities(s)));
        }
    }
    fn close_top(stack: &mut Vec<Open>) {
        let (tag, attrs, children) = stack.pop().unwrap();
        stack.last_mut().unwrap().2.push(Node::Element {
            tag,
            attrs,
            children,
        });
    }

    while i < b.len() {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        let rest = &html[i..];
        if let Some(body) = rest.strip_prefix("<!--") {
            flush_text(&mut stack, &html[text_start..i]);
            let end = body.find("-->").map_or(html.len(), |e| i + 4 + e);
            let data = &html[i + 4..end];
            stack
                .last_mut()
                .unwrap()
                .2
                .push(Node::Comment(data.to_string()));
            i = (end + 3).min(html.len());
            text_start = i;
        } else if rest.starts_with("<!") || rest.starts_with("<?") {
            // Doctype is dropped; `<?...>` is a bogus comment, as in browsers (Lit uses it).
            flush_text(&mut stack, &html[text_start..i]);
            let end = rest.find('>').map_or(html.len(), |e| i + e);
            if rest.starts_with("<?") {
                let data = &html[i + 1..end];
                stack
                    .last_mut()
                    .unwrap()
                    .2
                    .push(Node::Comment(data.to_string()));
            }
            i = (end + 1).min(html.len());
            text_start = i;
        } else if rest.starts_with("</") && rest[2..].starts_with(|c: char| c.is_ascii_alphabetic())
        {
            flush_text(&mut stack, &html[text_start..i]);
            let name_end = rest[2..]
                .find(|c: char| c.is_ascii_whitespace() || c == '>' || c == '/')
                .map_or(rest.len(), |e| e + 2);
            let tag = rest[2..name_end].to_ascii_lowercase();
            let end = rest.find('>').map_or(html.len(), |e| i + e);
            if let Some(pos) = stack.iter().rposition(|(t, _, _)| *t == tag)
                && pos > 0
            {
                while stack.len() > pos {
                    close_top(&mut stack);
                }
            }
            i = (end + 1).min(html.len());
            text_start = i;
        } else if rest[1..].starts_with(|c: char| c.is_ascii_alphabetic()) {
            flush_text(&mut stack, &html[text_start..i]);
            let (tag, attrs, self_closing, next) = start_tag(html, i);
            i = next;
            while stack.len() > 1 && closes(&stack.last().unwrap().0, &tag) {
                close_top(&mut stack);
            }
            if VOID.contains(&tag.as_str()) || self_closing {
                stack.last_mut().unwrap().2.push(Node::Element {
                    tag,
                    attrs,
                    children: Vec::new(),
                });
            } else if RAW_TEXT.contains(&tag.as_str()) {
                let close = format!("</{tag}");
                let lower = html[i..].to_ascii_lowercase();
                let end = lower.find(&close).map_or(html.len(), |e| i + e);
                let raw = &html[i..end];
                let mut children = Vec::new();
                if !raw.is_empty() {
                    let t = if tag == "script" || tag == "style" {
                        raw.to_string()
                    } else {
                        decode_entities(raw)
                    };
                    children.push(Node::Text(t));
                }
                stack.last_mut().unwrap().2.push(Node::Element {
                    tag,
                    attrs,
                    children,
                });
                i = html[end..].find('>').map_or(html.len(), |e| end + e + 1);
            } else {
                stack.push((tag, attrs, Vec::new()));
            }
            text_start = i;
        } else {
            i += 1;
        }
    }
    flush_text(&mut stack, &html[text_start..]);
    while stack.len() > 1 {
        close_top(&mut stack);
    }
    stack.pop().unwrap().2
}

/// Parses a start tag at `i` (which points at `<`). Returns tag, attributes, self-closing flag
/// and the index after `>`.
fn start_tag(html: &str, i: usize) -> (String, Vec<(String, String)>, bool, usize) {
    let b = html.as_bytes();
    let mut j = i + 1;
    while j < b.len() && !b[j].is_ascii_whitespace() && b[j] != b'>' && b[j] != b'/' {
        j += 1;
    }
    let tag = html[i + 1..j].to_ascii_lowercase();
    let mut attrs: Vec<(String, String)> = Vec::new();
    let mut self_closing = false;
    loop {
        while j < b.len() && b[j].is_ascii_whitespace() {
            j += 1;
        }
        if j >= b.len() {
            break;
        }
        if b[j] == b'>' {
            j += 1;
            break;
        }
        if b[j] == b'/' {
            j += 1;
            if j < b.len() && b[j] == b'>' {
                self_closing = true;
                j += 1;
                break;
            }
            continue;
        }
        let ns = j;
        while j < b.len() && !b[j].is_ascii_whitespace() && !matches!(b[j], b'=' | b'>') {
            if b[j] == b'/' && j + 1 < b.len() && b[j + 1] == b'>' {
                break;
            }
            j += 1;
        }
        let name = html[ns..j].to_ascii_lowercase();
        while j < b.len() && b[j].is_ascii_whitespace() {
            j += 1;
        }
        let mut value = String::new();
        if j < b.len() && b[j] == b'=' {
            j += 1;
            while j < b.len() && b[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < b.len() && (b[j] == b'"' || b[j] == b'\'') {
                let q = b[j];
                let vs = j + 1;
                j = vs;
                while j < b.len() && b[j] != q {
                    j += 1;
                }
                value = decode_entities(&html[vs..j]);
                j = (j + 1).min(b.len());
            } else {
                let vs = j;
                while j < b.len() && !b[j].is_ascii_whitespace() && b[j] != b'>' {
                    j += 1;
                }
                value = decode_entities(&html[vs..j]);
            }
        }
        if !name.is_empty() && !attrs.iter().any(|(n, _)| *n == name) {
            attrs.push((name, value));
        }
    }
    (tag, attrs, self_closing, j)
}

fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(p) = rest.find('&') {
        out.push_str(&rest[..p]);
        rest = &rest[p..];
        let end = rest[1..]
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '#'))
            .map(|e| e + 1);
        let (name, consumed) = match end {
            Some(e) if rest[e..].starts_with(';') => (&rest[1..e], e + 1),
            Some(e) => (&rest[1..e], e),
            None => (&rest[1..], rest.len()),
        };
        let decoded = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some('\u{a0}'),
            n if n.starts_with("#x") || n.starts_with("#X") => u32::from_str_radix(&n[2..], 16)
                .ok()
                .and_then(char::from_u32),
            n if n.starts_with('#') => n[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[consumed..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Serializes nodes as JSON: text is a string, a comment is `["#comment", data]`, an
/// element is `[tag, {attrs}, [children]]`.
pub fn to_json(nodes: &[Node]) -> String {
    let mut out = String::new();
    write_list(&mut out, nodes);
    out
}

fn write_list(out: &mut String, nodes: &[Node]) {
    out.push('[');
    for (i, n) in nodes.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        match n {
            Node::Text(t) => write_str(out, t),
            Node::Comment(c) => {
                out.push_str("[\"#comment\",");
                write_str(out, c);
                out.push(']');
            }
            Node::Element {
                tag,
                attrs,
                children,
            } => {
                out.push('[');
                write_str(out, tag);
                out.push_str(",{");
                for (j, (k, v)) in attrs.iter().enumerate() {
                    if j > 0 {
                        out.push(',');
                    }
                    write_str(out, k);
                    out.push(':');
                    write_str(out, v);
                }
                out.push_str("},");
                write_list(out, children);
                out.push(']');
            }
        }
    }
    out.push(']');
}

pub fn write_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || c == '\u{2028}' || c == '\u{2029}' => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_tree_with_raw_text_and_void_elements() {
        let html = "<!DOCTYPE html><html><head><script>if (a < b) x()</script></head>\
                    <body><p>one<p>two &amp; <b>three</b><br><input value='v'></body></html>";
        let json = to_json(&parse(html));
        assert_eq!(
            json,
            r#"[["html",{},[["head",{},[["script",{},["if (a < b) x()"]]]],["body",{},[["p",{},["one"]],["p",{},["two & ",["b",{},["three"]],["br",{},[]],["input",{"value":"v"},[]]]]]]]]]"#
        );
    }

    #[test]
    fn handles_tables_comments_and_unclosed_tags() {
        let html = "<table><tr><td>a<td>b<tr><td>c</table><!-- note --><?lit$1$><div x=1 y>";
        let json = to_json(&parse(html));
        assert_eq!(
            json,
            r##"[["table",{},[["tr",{},[["td",{},["a"]],["td",{},["b"]]]],["tr",{},[["td",{},["c"]]]]]],["#comment"," note "],["#comment","?lit$1$"],["div",{"x":"1","y":""},[]]]"##
        );
    }

    #[test]
    fn decodes_entities() {
        assert_eq!(
            decode_entities("a&lt;b&#x41;&#66;&bogus;&"),
            "a<bAB&bogus;&"
        );
    }
}
