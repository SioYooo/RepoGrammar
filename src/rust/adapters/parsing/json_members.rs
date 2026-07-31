//! Bounded JSON member scanning shared by static configuration parsers.
//!
//! `serde_json::Value` stores objects as maps, so duplicate member names would
//! otherwise be overwritten before a configuration parser could reject them.

use std::collections::BTreeSet;

pub(crate) const JSON_MEMBER_LIMIT: usize = 8_192;
pub(crate) const JSON_KEY_LIMIT: usize = 256;
pub(crate) const JSON_DEPTH_LIMIT: usize = 128;

/// Returns `Ok(false)` only for a complete JSON value whose decoded object keys
/// are unique and whose member, key, and nesting limits are respected.
/// `Ok(true)` identifies a duplicate key; malformed or excessive input is
/// represented by `Err(())`. Callers fail closed for either non-false result.
pub(crate) fn has_duplicate_or_excess_members(text: &str) -> Result<bool, ()> {
    JsonMemberScanner::scan(text)
}

struct JsonMemberScanner<'a> {
    text: &'a str,
    bytes: &'a [u8],
    cursor: usize,
    member_count: usize,
}

impl<'a> JsonMemberScanner<'a> {
    fn scan(text: &'a str) -> Result<bool, ()> {
        let mut scanner = Self {
            text,
            bytes: text.as_bytes(),
            cursor: 0,
            member_count: 0,
        };
        let duplicate = scanner.scan_value(0)?;
        scanner.skip_whitespace();
        (scanner.cursor == scanner.bytes.len())
            .then_some(duplicate)
            .ok_or(())
    }

    fn scan_value(&mut self, depth: usize) -> Result<bool, ()> {
        if depth > JSON_DEPTH_LIMIT {
            return Err(());
        }
        self.skip_whitespace();
        match self.bytes.get(self.cursor).copied() {
            Some(b'{') => self.scan_object(depth + 1),
            Some(b'[') => self.scan_array(depth + 1),
            Some(b'"') => {
                self.scan_string()?;
                Ok(false)
            }
            Some(_) => {
                let start = self.cursor;
                while self.bytes.get(self.cursor).is_some_and(|byte| {
                    !byte.is_ascii_whitespace() && !matches!(byte, b',' | b']' | b'}')
                }) {
                    self.cursor += 1;
                }
                (self.cursor > start).then_some(false).ok_or(())
            }
            None => Err(()),
        }
    }

    fn scan_object(&mut self, depth: usize) -> Result<bool, ()> {
        self.cursor += 1;
        self.skip_whitespace();
        if self.consume(b'}') {
            return Ok(false);
        }
        let mut keys = BTreeSet::new();
        let mut duplicate = false;
        loop {
            let (start, end) = self.scan_string()?;
            let key = serde_json::from_str::<String>(&self.text[start..end]).map_err(|_| ())?;
            self.member_count += 1;
            if self.member_count > JSON_MEMBER_LIMIT || key.len() > JSON_KEY_LIMIT {
                return Err(());
            }
            duplicate |= !keys.insert(key);
            self.skip_whitespace();
            if !self.consume(b':') {
                return Err(());
            }
            duplicate |= self.scan_value(depth)?;
            self.skip_whitespace();
            if self.consume(b'}') {
                return Ok(duplicate);
            }
            if !self.consume(b',') {
                return Err(());
            }
            self.skip_whitespace();
        }
    }

    fn scan_array(&mut self, depth: usize) -> Result<bool, ()> {
        self.cursor += 1;
        self.skip_whitespace();
        if self.consume(b']') {
            return Ok(false);
        }
        let mut duplicate = false;
        loop {
            duplicate |= self.scan_value(depth)?;
            self.skip_whitespace();
            if self.consume(b']') {
                return Ok(duplicate);
            }
            if !self.consume(b',') {
                return Err(());
            }
            self.skip_whitespace();
        }
    }

    fn scan_string(&mut self) -> Result<(usize, usize), ()> {
        let start = self.cursor;
        if !self.consume(b'"') {
            return Err(());
        }
        while let Some(byte) = self.bytes.get(self.cursor).copied() {
            match byte {
                b'"' => {
                    self.cursor += 1;
                    return Ok((start, self.cursor));
                }
                b'\\' => {
                    self.cursor += 1;
                    if self.bytes.get(self.cursor).is_none() {
                        return Err(());
                    }
                    self.cursor += 1;
                }
                0x00..=0x1f => return Err(()),
                _ => self.cursor += 1,
            }
        }
        Err(())
    }

    fn skip_whitespace(&mut self) {
        while self
            .bytes
            .get(self.cursor)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.cursor += 1;
        }
    }

    fn consume(&mut self, expected: u8) -> bool {
        if self.bytes.get(self.cursor) == Some(&expected) {
            self.cursor += 1;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoded_duplicate_keys_and_bounds_fail_closed() {
        assert_eq!(has_duplicate_or_excess_members(r#"{"a":1}"#), Ok(false));
        assert_eq!(
            has_duplicate_or_excess_members(r#"{"a":1,"\u0061":2}"#),
            Ok(true)
        );
        assert!(has_duplicate_or_excess_members(&format!(
            "{{\"{}\":1}}",
            "a".repeat(JSON_KEY_LIMIT + 1)
        ))
        .is_err());

        let maximum_members = (0..JSON_MEMBER_LIMIT)
            .map(|index| format!(r#""k{index}":null"#))
            .collect::<Vec<_>>()
            .join(",");
        assert_eq!(
            has_duplicate_or_excess_members(&format!("{{{maximum_members}}}")),
            Ok(false)
        );
        assert!(has_duplicate_or_excess_members(&format!(
            "{{{maximum_members},\"overflow\":null}}"
        ))
        .is_err());
    }

    #[test]
    fn nesting_limit_is_exact() {
        let maximum_depth = format!(
            "{}null{}",
            "[".repeat(JSON_DEPTH_LIMIT),
            "]".repeat(JSON_DEPTH_LIMIT)
        );
        assert_eq!(has_duplicate_or_excess_members(&maximum_depth), Ok(false));
        let excessive_depth = format!(
            "{}null{}",
            "[".repeat(JSON_DEPTH_LIMIT + 1),
            "]".repeat(JSON_DEPTH_LIMIT + 1)
        );
        assert!(has_duplicate_or_excess_members(&excessive_depth).is_err());
    }
}
