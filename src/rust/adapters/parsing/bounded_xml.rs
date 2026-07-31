//! Small non-validating XML reader for bounded project-metadata inventory.
//!
//! This reader intentionally implements only the XML surface needed by static
//! project metadata. It never resolves external entities, DTDs, XInclude,
//! schemas, imports, properties, conditions, or tasks. Unknown/entity-bearing
//! input fails closed instead of falling through to an ambient XML runtime.

use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy)]
pub(crate) struct BoundedXmlLimits {
    pub max_depth: usize,
    pub max_nodes: usize,
    pub max_attributes_per_node: usize,
    pub max_name_bytes: usize,
    pub max_value_bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BoundedXmlError {
    Malformed,
    ResourceLimit,
}

#[derive(Debug, Clone)]
pub(crate) struct XmlNode {
    name: String,
    attributes: BTreeMap<String, String>,
    text: String,
    parent: Option<usize>,
    children: Vec<usize>,
}

impl XmlNode {
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes.get(name).map(String::as_str)
    }

    pub(crate) fn text_trimmed(&self) -> &str {
        self.text.trim()
    }

    pub(crate) fn parent(&self) -> Option<usize> {
        self.parent
    }

    pub(crate) fn children(&self) -> &[usize] {
        &self.children
    }
}

#[derive(Debug, Clone)]
pub(crate) struct XmlDocument {
    nodes: Vec<XmlNode>,
    root: usize,
}

impl XmlDocument {
    pub(crate) fn root(&self) -> &XmlNode {
        &self.nodes[self.root]
    }

    pub(crate) fn nodes(&self) -> &[XmlNode] {
        &self.nodes
    }

    pub(crate) fn node(&self, index: usize) -> &XmlNode {
        &self.nodes[index]
    }
}

pub(crate) fn parse_bounded_xml(
    input: &str,
    limits: BoundedXmlLimits,
) -> Result<XmlDocument, BoundedXmlError> {
    if limits.max_depth == 0
        || limits.max_nodes == 0
        || limits.max_attributes_per_node == 0
        || limits.max_name_bytes == 0
        || limits.max_value_bytes == 0
    {
        return Err(BoundedXmlError::ResourceLimit);
    }
    if input
        .chars()
        .any(|character| !is_valid_xml_character(character))
    {
        return Err(BoundedXmlError::Malformed);
    }
    let bytes = input.as_bytes();
    let mut cursor = if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        3
    } else {
        0
    };
    let mut nodes = Vec::<XmlNode>::new();
    let mut stack = Vec::<usize>::new();
    let mut root = None;
    let mut saw_xml_declaration = false;

    while cursor < bytes.len() {
        if bytes[cursor] != b'<' {
            let start = cursor;
            while cursor < bytes.len() && bytes[cursor] != b'<' {
                cursor += 1;
            }
            let raw = &input[start..cursor];
            if let Some(node) = stack.last().copied() {
                let decoded = decode_xml_value(raw, limits.max_value_bytes)?;
                append_bounded(&mut nodes[node].text, &decoded, limits.max_value_bytes)?;
            } else if !raw.trim().is_empty() {
                return Err(BoundedXmlError::Malformed);
            }
            continue;
        }

        if bytes[cursor..].starts_with(b"<!--") {
            let Some(relative_end) = input[cursor + 4..].find("-->") else {
                return Err(BoundedXmlError::Malformed);
            };
            let comment = &input[cursor + 4..cursor + 4 + relative_end];
            if comment.contains("--") {
                return Err(BoundedXmlError::Malformed);
            }
            cursor += 4 + relative_end + 3;
            continue;
        }
        if bytes[cursor..].starts_with(b"<?") {
            let Some(relative_end) = input[cursor + 2..].find("?>") else {
                return Err(BoundedXmlError::Malformed);
            };
            let body = input[cursor + 2..cursor + 2 + relative_end].trim();
            if saw_xml_declaration || root.is_some() || !body.starts_with("xml") {
                return Err(BoundedXmlError::Malformed);
            }
            let suffix = &body[3..];
            if !suffix.is_empty()
                && !suffix
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_whitespace)
            {
                return Err(BoundedXmlError::Malformed);
            }
            saw_xml_declaration = true;
            cursor += 2 + relative_end + 2;
            continue;
        }
        if bytes[cursor..].starts_with(b"<!") {
            return Err(BoundedXmlError::Malformed);
        }
        if bytes[cursor..].starts_with(b"</") {
            cursor += 2;
            skip_ascii_whitespace(bytes, &mut cursor);
            let name = parse_name(input, &mut cursor, limits.max_name_bytes)?;
            skip_ascii_whitespace(bytes, &mut cursor);
            if bytes.get(cursor) != Some(&b'>') {
                return Err(BoundedXmlError::Malformed);
            }
            cursor += 1;
            let Some(open) = stack.pop() else {
                return Err(BoundedXmlError::Malformed);
            };
            if nodes[open].name != name {
                return Err(BoundedXmlError::Malformed);
            }
            continue;
        }

        cursor += 1;
        let name = parse_name(input, &mut cursor, limits.max_name_bytes)?;
        let mut attributes = BTreeMap::new();
        loop {
            skip_ascii_whitespace(bytes, &mut cursor);
            match bytes.get(cursor) {
                Some(b'>') => {
                    cursor += 1;
                    break;
                }
                Some(b'/') if bytes.get(cursor + 1) == Some(&b'>') => {
                    cursor += 2;
                    break;
                }
                Some(_) => {}
                None => return Err(BoundedXmlError::Malformed),
            }
            if attributes.len() == limits.max_attributes_per_node {
                return Err(BoundedXmlError::ResourceLimit);
            }
            let key = parse_name(input, &mut cursor, limits.max_name_bytes)?;
            skip_ascii_whitespace(bytes, &mut cursor);
            if bytes.get(cursor) != Some(&b'=') {
                return Err(BoundedXmlError::Malformed);
            }
            cursor += 1;
            skip_ascii_whitespace(bytes, &mut cursor);
            let Some(quote @ (b'\'' | b'"')) = bytes.get(cursor).copied() else {
                return Err(BoundedXmlError::Malformed);
            };
            cursor += 1;
            let value_start = cursor;
            while cursor < bytes.len() && bytes[cursor] != quote {
                if bytes[cursor] == b'<' {
                    return Err(BoundedXmlError::Malformed);
                }
                cursor += 1;
            }
            if bytes.get(cursor) != Some(&quote) {
                return Err(BoundedXmlError::Malformed);
            }
            let value = decode_xml_value(&input[value_start..cursor], limits.max_value_bytes)?;
            cursor += 1;
            if attributes.insert(key, value).is_some() {
                return Err(BoundedXmlError::Malformed);
            }
        }

        if stack.len() == limits.max_depth || nodes.len() == limits.max_nodes {
            return Err(BoundedXmlError::ResourceLimit);
        }
        let parent = stack.last().copied();
        if parent.is_none() && root.is_some() {
            return Err(BoundedXmlError::Malformed);
        }
        let index = nodes.len();
        nodes.push(XmlNode {
            name,
            attributes,
            text: String::new(),
            parent,
            children: Vec::new(),
        });
        if let Some(parent) = parent {
            nodes[parent].children.push(index);
        } else {
            root = Some(index);
        }
        let self_closing = bytes.get(cursor.saturating_sub(2)) == Some(&b'/');
        if !self_closing {
            stack.push(index);
        }
    }

    if !stack.is_empty() {
        return Err(BoundedXmlError::Malformed);
    }
    let root = root.ok_or(BoundedXmlError::Malformed)?;
    Ok(XmlDocument { nodes, root })
}

fn parse_name(
    input: &str,
    cursor: &mut usize,
    max_name_bytes: usize,
) -> Result<String, BoundedXmlError> {
    let bytes = input.as_bytes();
    let start = *cursor;
    let Some(first) = bytes.get(*cursor).copied() else {
        return Err(BoundedXmlError::Malformed);
    };
    if !is_name_start(first) {
        return Err(BoundedXmlError::Malformed);
    }
    *cursor += 1;
    while bytes
        .get(*cursor)
        .is_some_and(|byte| is_name_continue(*byte))
    {
        *cursor += 1;
    }
    if *cursor - start > max_name_bytes {
        return Err(BoundedXmlError::ResourceLimit);
    }
    Ok(input[start..*cursor].to_string())
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || matches!(byte, b'_' | b':')
}

fn is_name_continue(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit() || matches!(byte, b'.' | b'-')
}

fn skip_ascii_whitespace(bytes: &[u8], cursor: &mut usize) {
    while bytes.get(*cursor).is_some_and(u8::is_ascii_whitespace) {
        *cursor += 1;
    }
}

fn append_bounded(
    target: &mut String,
    value: &str,
    max_value_bytes: usize,
) -> Result<(), BoundedXmlError> {
    let Some(new_len) = target.len().checked_add(value.len()) else {
        return Err(BoundedXmlError::ResourceLimit);
    };
    if new_len > max_value_bytes {
        return Err(BoundedXmlError::ResourceLimit);
    }
    target.push_str(value);
    Ok(())
}

fn decode_xml_value(input: &str, max_value_bytes: usize) -> Result<String, BoundedXmlError> {
    if input.len() > max_value_bytes {
        return Err(BoundedXmlError::ResourceLimit);
    }
    let mut output = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(position) = rest.find('&') {
        append_bounded(&mut output, &rest[..position], max_value_bytes)?;
        let entity = &rest[position + 1..];
        let Some(end) = entity.find(';') else {
            return Err(BoundedXmlError::Malformed);
        };
        let token = &entity[..end];
        let character = match token {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            value if value.starts_with("#x") => u32::from_str_radix(&value[2..], 16)
                .ok()
                .and_then(char::from_u32)
                .ok_or(BoundedXmlError::Malformed)?,
            value if value.starts_with('#') => value[1..]
                .parse::<u32>()
                .ok()
                .and_then(char::from_u32)
                .ok_or(BoundedXmlError::Malformed)?,
            _ => return Err(BoundedXmlError::Malformed),
        };
        if !is_valid_xml_character(character) {
            return Err(BoundedXmlError::Malformed);
        }
        let mut encoded = [0_u8; 4];
        append_bounded(
            &mut output,
            character.encode_utf8(&mut encoded),
            max_value_bytes,
        )?;
        rest = &entity[end + 1..];
    }
    append_bounded(&mut output, rest, max_value_bytes)?;
    Ok(output)
}

fn is_valid_xml_character(character: char) -> bool {
    matches!(
        character as u32,
        0x9 | 0xa | 0xd | 0x20..=0xd7ff | 0xe000..=0xfffd | 0x10000..=0x10ffff
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMITS: BoundedXmlLimits = BoundedXmlLimits {
        max_depth: 4,
        max_nodes: 4,
        max_attributes_per_node: 2,
        max_name_bytes: 32,
        max_value_bytes: 64,
    };

    #[test]
    fn reads_bounded_elements_attributes_comments_and_entities() {
        let parsed = parse_bounded_xml(
            r#"<?xml version="1.0"?><Project><!--safe--><Item Include="A&amp;B">1&lt;2</Item></Project>"#,
            LIMITS,
        )
        .expect("bounded XML");
        assert_eq!(parsed.root().name(), "Project");
        let item = parsed.node(parsed.root().children()[0]);
        assert_eq!(item.attribute("Include"), Some("A&B"));
        assert_eq!(item.text_trimmed(), "1<2");
        assert_eq!(item.parent(), Some(0));
    }

    #[test]
    fn rejects_doctype_external_entities_and_malformed_nesting() {
        for input in [
            "<!DOCTYPE Project [<!ENTITY x SYSTEM 'file:///etc/passwd'>]><Project>&x;</Project>",
            "<Project>&custom;</Project>",
            "<Project><Item></Project>",
            "<Project A='1' A='2'/>",
            "<Project/><Other/>",
            "<Project>&#0;</Project>",
            "<Project>\0</Project>",
        ] {
            assert_eq!(
                parse_bounded_xml(input, LIMITS).expect_err("must reject"),
                BoundedXmlError::Malformed,
                "{input}"
            );
        }
    }

    #[test]
    fn inclusive_limits_accept_exact_and_reject_plus_one() {
        assert!(parse_bounded_xml("<A><B><C><D/></C></B></A>", LIMITS).is_ok());
        assert!(matches!(
            parse_bounded_xml("<A><B><C><D><E/></D></C></B></A>", LIMITS),
            Err(BoundedXmlError::ResourceLimit)
        ));
        assert!(parse_bounded_xml("<A a='1' b='2'/>", LIMITS).is_ok());
        assert!(matches!(
            parse_bounded_xml("<A a='1' b='2' c='3'/>", LIMITS),
            Err(BoundedXmlError::ResourceLimit)
        ));
    }
}
