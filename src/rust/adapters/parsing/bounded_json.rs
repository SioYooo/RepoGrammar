//! Shared non-executing admission gate for JSON project metadata.
//!
//! `serde_json::Value` intentionally keeps the last value for duplicate object
//! members. Project manifests must reject that ambiguity before normal JSON
//! decoding, while also bounding nesting, total object members, and decoded key
//! bytes.

use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BoundedJsonLimits {
    pub max_depth: usize,
    pub max_members: usize,
    pub max_key_bytes: usize,
}

pub(crate) fn has_duplicate_or_excess_members(
    text: &str,
    limits: BoundedJsonLimits,
) -> Result<bool, ()> {
    let mut scanner = JsonMemberScanner {
        text,
        bytes: text.as_bytes(),
        cursor: 0,
        member_count: 0,
        limits,
    };
    let duplicate = scanner.scan_value(0)?;
    scanner.skip_whitespace();
    (scanner.cursor == scanner.bytes.len())
        .then_some(duplicate)
        .ok_or(())
}

struct JsonMemberScanner<'a> {
    text: &'a str,
    bytes: &'a [u8],
    cursor: usize,
    member_count: usize,
    limits: BoundedJsonLimits,
}

impl JsonMemberScanner<'_> {
    fn scan_value(&mut self, depth: usize) -> Result<bool, ()> {
        if depth > self.limits.max_depth {
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
        if depth > self.limits.max_depth {
            return Err(());
        }
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
            if self.member_count > self.limits.max_members || key.len() > self.limits.max_key_bytes
            {
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
        if depth > self.limits.max_depth {
            return Err(());
        }
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

    const LIMITS: BoundedJsonLimits = BoundedJsonLimits {
        max_depth: 3,
        max_members: 3,
        max_key_bytes: 4,
    };

    #[test]
    fn rejects_decoded_duplicate_members_and_accepts_unique_json() {
        assert_eq!(
            has_duplicate_or_excess_members(r#"{"a":1,"b":{"c":2}}"#, LIMITS),
            Ok(false)
        );
        assert_eq!(
            has_duplicate_or_excess_members(r#"{"a":1,"\u0061":2}"#, LIMITS),
            Ok(true)
        );
    }

    #[test]
    fn depth_member_and_decoded_key_limits_are_inclusive() {
        assert_eq!(
            has_duplicate_or_excess_members(r#"[[[null]]]"#, LIMITS),
            Ok(false)
        );
        assert!(has_duplicate_or_excess_members(r#"[[[[null]]]]"#, LIMITS).is_err());
        assert!(has_duplicate_or_excess_members(r#"[[[[]]]]"#, LIMITS).is_err());
        assert_eq!(
            has_duplicate_or_excess_members(r#"{"a":1,"b":2,"c":3}"#, LIMITS),
            Ok(false)
        );
        assert!(has_duplicate_or_excess_members(r#"{"a":1,"b":2,"c":3,"d":4}"#, LIMITS).is_err());
        assert_eq!(
            has_duplicate_or_excess_members(r#"{"four":1}"#, LIMITS),
            Ok(false)
        );
        assert!(has_duplicate_or_excess_members(r#"{"fives":1}"#, LIMITS).is_err());
    }
}
