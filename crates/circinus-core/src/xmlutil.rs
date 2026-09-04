//! Lenient XML reading. RimWorld's loader is forgiving about casing and stray characters;
//! mods in the wild rely on that, so we retry with a sanitized copy before giving up.

use crate::{Error, Result};
use roxmltree::{Document, Node};
use std::path::Path;

/// Read a file as UTF-8 (BOM tolerated, invalid bytes replaced).
pub fn read_text(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)?;
    let s = String::from_utf8_lossy(&bytes);
    Ok(s.trim_start_matches('\u{feff}').to_string())
}

/// Sanitize text so that the common mistakes in mod XML parse: bare ampersands, control
/// characters, and a stray `<?xml` declaration that is not at the very start.
pub fn sanitize(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    let bytes: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == '&' {
            // keep well-formed entities, escape everything else
            let rest: String = bytes[i..bytes.len().min(i + 12)].iter().collect();
            let is_entity = rest.starts_with("&amp;")
                || rest.starts_with("&lt;")
                || rest.starts_with("&gt;")
                || rest.starts_with("&quot;")
                || rest.starts_with("&apos;")
                || (rest.starts_with("&#") && rest[2..].chars().take_while(|c| c.is_ascii_alphanumeric()).count() > 0 && rest[2..].contains(';'));
            if is_entity {
                out.push('&');
            } else {
                out.push_str("&amp;");
            }
        } else if c.is_control() && c != '\n' && c != '\r' && c != '\t' {
            // drop
        } else {
            out.push(c);
        }
        i += 1;
    }
    let trimmed = out.trim_start();
    if !trimmed.starts_with("<?xml") {
        if let Some(pos) = trimmed.find("<?xml") {
            if pos > 0 {
                let end = trimmed[pos..].find("?>").map(|e| pos + e + 2).unwrap_or(pos);
                let mut s = String::new();
                s.push_str(&trimmed[..pos]);
                s.push_str(&trimmed[end..]);
                return s;
            }
        }
    }
    out
}

/// Case-insensitive tag comparison; RimWorld is case-sensitive but mod authors are not.
pub fn tag_is(node: Node, name: &str) -> bool {
    node.is_element() && node.tag_name().name().eq_ignore_ascii_case(name)
}

pub fn child<'a, 'i>(node: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    node.children().find(|c| tag_is(*c, name))
}

pub fn child_text(node: Node, name: &str) -> Option<String> {
    child(node, name).map(|n| n.text().unwrap_or("").trim().to_string()).filter(|s| !s.is_empty())
}

/// `<name><li>a</li><li>b</li></name>` → ["a", "b"]. A bare text child is also accepted.
pub fn list(node: Node, name: &str) -> Vec<String> {
    match child(node, name) {
        None => Vec::new(),
        Some(n) => li_texts(n),
    }
}

pub fn li_texts(n: Node) -> Vec<String> {
    let items: Vec<String> = n
        .children()
        .filter(|c| tag_is(*c, "li"))
        .filter_map(|c| c.text().map(|t| t.trim().to_string()))
        .filter(|s| !s.is_empty())
        .collect();
    if items.is_empty() {
        if let Some(t) = n.text() {
            let t = t.trim();
            if !t.is_empty() {
                return vec![t.to_string()];
            }
        }
    }
    items
}

/// Escape for writing text content or attribute values.
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// Parse a document, retrying with a sanitized copy. Runs `f` with the parsed document so the
/// borrow of the (possibly sanitized) text stays inside this function.
pub fn with_document<T>(text: &str, path: &Path, f: impl FnOnce(&Document) -> Result<T>) -> Result<T> {
    match Document::parse(text) {
        Ok(doc) => f(&doc),
        Err(first) => {
            let cleaned = sanitize(text);
            match Document::parse(&cleaned) {
                Ok(doc) => f(&doc),
                Err(_) => Err(Error::Xml { path: path.display().to_string(), msg: first.to_string() }),
            }
        }
    }
}
