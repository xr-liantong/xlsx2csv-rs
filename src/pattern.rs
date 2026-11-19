//! The slice of Python `re` that `-I` / `-E` need, and nothing more.
//!
//! Upstream runs `re.match(pattern, sheetname)`, i.e. the pattern is anchored
//! at the start but may stop anywhere. Supported here: literals, `.`,
//! `[...]` / `[^...]` with ranges, `*`, `+`, `?`, `^` (no-op — matching is
//! already anchored), `$`, and backslash escapes of a single character.
//! Groups, alternation and repetition counts are rejected with an error, so a
//! pattern we cannot honour never silently changes the sheet list.

use crate::errors::PortError;

#[derive(Debug, Clone)]
enum Node {
    Char(char),
    Any,
    Class {
        negated: bool,
        ranges: Vec<(char, char)>,
    },
    Star(Box<Node>),
    Plus(Box<Node>),
    Opt(Box<Node>),
    End,
}

#[derive(Debug, Clone)]
pub struct Pattern {
    nodes: Vec<Node>,
}

impl Pattern {
    pub fn compile(source: &str) -> Result<Self, PortError> {
        let chars: Vec<char> = source.chars().collect();
        let mut nodes = Vec::new();
        let mut i = 0;
        while i < chars.len() {
            let node = match chars[i] {
                '^' if i == 0 => {
                    i += 1;
                    continue;
                }
                '$' => Node::End,
                '.' => Node::Any,
                '[' => {
                    let (class, next) = parse_class(&chars, i)?;
                    i = next;
                    nodes.push(class);
                    continue;
                }
                '(' | ')' | '|' | '{' | '}' => {
                    return Err(unsupported(source, chars[i]));
                }
                '*' | '+' | '?' => {
                    let prev = nodes.pop().ok_or_else(|| unsupported(source, chars[i]))?;
                    match chars[i] {
                        '*' => Node::Star(Box::new(prev)),
                        '+' => Node::Plus(Box::new(prev)),
                        _ => Node::Opt(Box::new(prev)),
                    }
                }
                '\\' => {
                    i += 1;
                    match chars.get(i) {
                        Some(c) if !c.is_alphanumeric() => Node::Char(*c),
                        _ => return Err(unsupported(source, chars[i - 1])),
                    }
                }
                c => Node::Char(c),
            };
            nodes.push(node);
            i += 1;
        }
        Ok(Pattern { nodes })
    }

    /// `re.match`: anchored at the start, free to end early unless `$` says not.
    pub fn matches(&self, text: &str) -> bool {
        let chars: Vec<char> = text.chars().collect();
        match_seq(&self.nodes, &chars, 0)
    }
}

fn unsupported(source: &str, at: char) -> PortError {
    PortError::Pattern(format!(
        "unsupported regular expression {source:?} in -I/-E (near {at:?}); \
         supported: literals, '.', '*', '+', '?', '[...]', '$'"
    ))
}

fn parse_class(chars: &[char], start: usize) -> Result<(Node, usize), PortError> {
    let mut i = start + 1;
    let negated = chars.get(i) == Some(&'^');
    if negated {
        i += 1;
    }
    let mut ranges: Vec<(char, char)> = Vec::new();
    let mut first = true;
    loop {
        let Some(&c) = chars.get(i) else {
            return Err(PortError::Pattern(
                "unterminated character class in -I/-E pattern".to_string(),
            ));
        };
        if c == ']' && !first {
            i += 1;
            break;
        }
        first = false;
        let (lo, next) = if c == '\\' {
            match chars.get(i + 1) {
                Some(&e) => (e, i + 2),
                None => {
                    return Err(PortError::Pattern(
                        "trailing backslash in -I/-E pattern".to_string(),
                    ))
                }
            }
        } else {
            (c, i + 1)
        };
        if chars.get(next) == Some(&'-') && chars.get(next + 1).is_some_and(|h| *h != ']') {
            let hi = chars[next + 1];
            ranges.push((lo, hi));
            i = next + 2;
        } else {
            ranges.push((lo, lo));
            i = next;
        }
    }
    Ok((Node::Class { negated, ranges }, i))
}

fn class_hit(negated: bool, ranges: &[(char, char)], c: char) -> bool {
    let inside = ranges.iter().any(|(lo, hi)| *lo <= c && c <= *hi);
    inside != negated
}

fn match_one(node: &Node, text: &[char], pos: usize) -> Option<usize> {
    match node {
        Node::Char(want) => (text.get(pos) == Some(want)).then_some(pos + 1),
        Node::Any => (pos < text.len()).then_some(pos + 1),
        Node::Class { negated, ranges } => match text.get(pos) {
            Some(&c) if class_hit(*negated, ranges, c) => Some(pos + 1),
            _ => None,
        },
        _ => None,
    }
}

fn with(node: &Node, tail: &[Node]) -> Vec<Node> {
    let mut v = vec![node.clone()];
    v.extend_from_slice(tail);
    v
}

fn match_seq(nodes: &[Node], text: &[char], pos: usize) -> bool {
    let Some((head, tail)) = nodes.split_first() else {
        return true;
    };
    match head {
        Node::End => pos == text.len() && match_seq(tail, text, pos),
        Node::Char(_) | Node::Any | Node::Class { .. } => match match_one(head, text, pos) {
            Some(next) => match_seq(tail, text, next),
            None => false,
        },
        Node::Opt(inner) => {
            if match_seq(&with(inner, tail), text, pos) {
                return true;
            }
            match_seq(tail, text, pos)
        }
        Node::Plus(inner) => match match_one(inner, text, pos) {
            Some(next) => match_seq(&with(&Node::Star(inner.clone()), tail), text, next),
            None => false,
        },
        Node::Star(inner) => {
            if let Some(next) = match_one(inner, text, pos) {
                if next > pos && match_seq(&with(head, tail), text, next) {
                    return true;
                }
            }
            match_seq(tail, text, pos)
        }
    }
}

/// True when `patterns` is empty, or at least one non-empty pattern matches.
/// Mirrors upstream's `include = pattern and len(pattern) > 0 and re.match(...)`.
pub fn any_matches(patterns: &[String], text: &str) -> Result<bool, PortError> {
    for pattern in patterns {
        if pattern.is_empty() {
            continue;
        }
        if Pattern::compile(pattern)?.matches(text) {
            return Ok(true);
        }
    }
    Ok(patterns.iter().all(|p| p.is_empty()))
}

/// Convenience for the exclude list: an empty pattern is skipped, matching
/// upstream's `pattern and len(pattern) > 0 and re.match(...)`.
pub fn none_matches(patterns: &[String], text: &str) -> Result<bool, PortError> {
    for pattern in patterns {
        if !pattern.is_empty() && Pattern::compile(pattern)?.matches(text) {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn m(pattern: &str, text: &str) -> bool {
        Pattern::compile(pattern).unwrap().matches(text)
    }

    #[test]
    fn anchored_at_start_like_re_match() {
        assert!(m("^.*$", "anything"));
        assert!(m("S1", "S1"));
        assert!(!m("S1", "xS1"));
        assert!(m("S", "S1 extra"));
        assert!(m("^S2", "S2 name"));
    }

    #[test]
    fn classes_and_quantifiers() {
        assert!(m("[0-9]+", "42ab"));
        assert!(!m("^[^0-9]+$", "a1"));
        assert!(m("^[^0-9]+$", "abc"));
        assert!(m("a?b", "b"));
        assert!(m("a+b", "aaab"));
        assert!(!m("^a+b$", "ab_"));
    }

    #[test]
    fn rejects_what_it_cannot_honour() {
        assert!(Pattern::compile("(a|b)").is_err());
        assert!(Pattern::compile("a{2}").is_err());
        assert!(Pattern::compile("\\d+").is_err());
    }

    #[test]
    fn list_helpers_follow_upstream() {
        assert!(any_matches(&["^.*$".to_string()], "any").unwrap());
        assert!(!any_matches(&["^S".to_string()], "Data").unwrap());
        assert!(none_matches(&["".to_string()], "any").unwrap());
        assert!(!none_matches(&["^S".to_string()], "S1").unwrap());
    }
}
