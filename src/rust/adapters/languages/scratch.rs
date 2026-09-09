//! Scratch 3 `.sb3` archive security preflight.
//!
//! This module is intentionally not wired into repository indexing. The
//! current source-store/parser port is UTF-8 text-only and the repository has
//! no reviewed ZIP/deflate dependency. The preflight nevertheless establishes
//! auditable archive and JSON limits for a future binary-input port. It can
//! inspect a stored (method 0) root `project.json` without extracting files;
//! common deflated projects return an explicit unsupported-compression result.
//! It never runs Scratch VM/extensions or downloads assets.

use crate::adapters::parsing::bounded_json::{has_duplicate_or_excess_members, BoundedJsonLimits};
use serde_json::Value;
use std::collections::BTreeSet;

const EOCD_SIGNATURE: u32 = 0x0605_4b50;
const CENTRAL_SIGNATURE: u32 = 0x0201_4b50;
const LOCAL_SIGNATURE: u32 = 0x0403_4b50;
const MAX_ARCHIVE_BYTES: usize = 64 * 1024 * 1024;
const MAX_ENTRIES: usize = 4_096;
const MAX_TOTAL_UNCOMPRESSED_BYTES: u64 = 256 * 1024 * 1024;
const MAX_ENTRY_UNCOMPRESSED_BYTES: u64 = 64 * 1024 * 1024;
const MAX_COMPRESSION_RATIO: u64 = 200;
const MAX_PATH_BYTES: usize = 512;
const MAX_PROJECT_JSON_BYTES: usize = 8 * 1024 * 1024;
const MAX_JSON_NODES: usize = 200_000;
const MAX_TARGETS: usize = 4_096;
const MAX_BLOCKS: usize = 100_000;
const MAX_EXTENSIONS: usize = 256;
const SCRATCH_JSON_LIMITS: BoundedJsonLimits = BoundedJsonLimits {
    max_depth: 128,
    max_members: 200_000,
    max_key_bytes: 512,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScratchProjectInventory {
    pub archive_entries: usize,
    pub total_uncompressed_bytes: u64,
    pub target_count: usize,
    pub block_count: usize,
    pub event_hat_count: usize,
    pub opcode_count: usize,
    pub declared_extension_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScratchArchiveError {
    ArchiveTooLarge,
    MalformedZip,
    MultiDiskOrZip64,
    EntryLimit,
    SizeLimit,
    CompressionRatio,
    EncryptedEntry,
    UnsupportedCompression,
    UnsafePath,
    SymlinkEntry,
    DuplicatePath,
    MissingOrDuplicateProjectJson,
    ProjectJsonTooLarge,
    ProjectJsonNotUtf8,
    MalformedOrOverBudgetJson,
    UnsupportedProjectShape,
}

#[derive(Debug, Clone)]
struct Entry<'a> {
    name: &'a str,
    method: u16,
    compressed_size: u64,
    uncompressed_size: u64,
    local_offset: usize,
    flags: u16,
}

pub fn inspect_sb3(bytes: &[u8]) -> Result<ScratchProjectInventory, ScratchArchiveError> {
    if bytes.len() > MAX_ARCHIVE_BYTES {
        return Err(ScratchArchiveError::ArchiveTooLarge);
    }
    let eocd_offset = find_eocd(bytes).ok_or(ScratchArchiveError::MalformedZip)?;
    let disk = read_u16(bytes, eocd_offset + 4)?;
    let central_disk = read_u16(bytes, eocd_offset + 6)?;
    let disk_entries = read_u16(bytes, eocd_offset + 8)?;
    let total_entries = read_u16(bytes, eocd_offset + 10)?;
    let central_size = read_u32(bytes, eocd_offset + 12)?;
    let central_offset = read_u32(bytes, eocd_offset + 16)?;
    let comment_len = usize::from(read_u16(bytes, eocd_offset + 20)?);
    if disk != 0 || central_disk != 0 || disk_entries != total_entries {
        return Err(ScratchArchiveError::MultiDiskOrZip64);
    }
    if total_entries == u16::MAX || central_size == u32::MAX || central_offset == u32::MAX {
        return Err(ScratchArchiveError::MultiDiskOrZip64);
    }
    if eocd_offset
        .checked_add(22)
        .and_then(|value| value.checked_add(comment_len))
        != Some(bytes.len())
    {
        return Err(ScratchArchiveError::MalformedZip);
    }
    let total_entries = usize::from(total_entries);
    if total_entries == 0 || total_entries > MAX_ENTRIES {
        return Err(ScratchArchiveError::EntryLimit);
    }
    let central_offset =
        usize::try_from(central_offset).map_err(|_| ScratchArchiveError::MalformedZip)?;
    let central_size =
        usize::try_from(central_size).map_err(|_| ScratchArchiveError::MalformedZip)?;
    if central_offset.checked_add(central_size) != Some(eocd_offset) {
        return Err(ScratchArchiveError::MalformedZip);
    }

    let mut cursor = central_offset;
    let mut names = BTreeSet::new();
    let mut entries = Vec::with_capacity(total_entries);
    let mut total_uncompressed = 0u64;
    let mut project_count = 0usize;
    for _ in 0..total_entries {
        if read_u32(bytes, cursor)? != CENTRAL_SIGNATURE {
            return Err(ScratchArchiveError::MalformedZip);
        }
        let version_made_by = read_u16(bytes, cursor + 4)?;
        let flags = read_u16(bytes, cursor + 8)?;
        let method = read_u16(bytes, cursor + 10)?;
        let compressed_size = u64::from(read_u32(bytes, cursor + 20)?);
        let uncompressed_size = u64::from(read_u32(bytes, cursor + 24)?);
        let name_len = usize::from(read_u16(bytes, cursor + 28)?);
        let extra_len = usize::from(read_u16(bytes, cursor + 30)?);
        let entry_comment_len = usize::from(read_u16(bytes, cursor + 32)?);
        let disk_start = read_u16(bytes, cursor + 34)?;
        let external_attributes = read_u32(bytes, cursor + 38)?;
        let local_offset = usize::try_from(read_u32(bytes, cursor + 42)?)
            .map_err(|_| ScratchArchiveError::MalformedZip)?;
        if disk_start != 0 {
            return Err(ScratchArchiveError::MultiDiskOrZip64);
        }
        if flags & 0x0001 != 0 {
            return Err(ScratchArchiveError::EncryptedEntry);
        }
        if !matches!(method, 0 | 8) {
            return Err(ScratchArchiveError::UnsupportedCompression);
        }
        if name_len == 0 || name_len > MAX_PATH_BYTES {
            return Err(ScratchArchiveError::UnsafePath);
        }
        let name_start = cursor
            .checked_add(46)
            .ok_or(ScratchArchiveError::MalformedZip)?;
        let name_end = name_start
            .checked_add(name_len)
            .ok_or(ScratchArchiveError::MalformedZip)?;
        let next = name_end
            .checked_add(extra_len)
            .and_then(|value| value.checked_add(entry_comment_len))
            .ok_or(ScratchArchiveError::MalformedZip)?;
        if next > eocd_offset {
            return Err(ScratchArchiveError::MalformedZip);
        }
        let name_bytes = bytes
            .get(name_start..name_end)
            .ok_or(ScratchArchiveError::MalformedZip)?;
        let name = std::str::from_utf8(name_bytes).map_err(|_| ScratchArchiveError::UnsafePath)?;
        validate_archive_path(name)?;
        if !names.insert(name) {
            return Err(ScratchArchiveError::DuplicatePath);
        }
        if version_made_by >> 8 == 3 && ((external_attributes >> 16) & 0o170000) == 0o120000 {
            return Err(ScratchArchiveError::SymlinkEntry);
        }
        if uncompressed_size > MAX_ENTRY_UNCOMPRESSED_BYTES {
            return Err(ScratchArchiveError::SizeLimit);
        }
        total_uncompressed = total_uncompressed
            .checked_add(uncompressed_size)
            .ok_or(ScratchArchiveError::SizeLimit)?;
        if total_uncompressed > MAX_TOTAL_UNCOMPRESSED_BYTES {
            return Err(ScratchArchiveError::SizeLimit);
        }
        if uncompressed_size > 0
            && (compressed_size == 0
                || uncompressed_size
                    > compressed_size
                        .checked_mul(MAX_COMPRESSION_RATIO)
                        .ok_or(ScratchArchiveError::CompressionRatio)?)
        {
            return Err(ScratchArchiveError::CompressionRatio);
        }
        if method == 0 && compressed_size != uncompressed_size {
            return Err(ScratchArchiveError::MalformedZip);
        }
        if name == "project.json" {
            project_count += 1;
        }
        entries.push(Entry {
            name,
            method,
            compressed_size,
            uncompressed_size,
            local_offset,
            flags,
        });
        cursor = next;
    }
    if cursor != eocd_offset || project_count != 1 {
        return Err(ScratchArchiveError::MissingOrDuplicateProjectJson);
    }
    let project = entries
        .iter()
        .find(|entry| entry.name == "project.json")
        .expect("project_count proved one project.json");
    if project.method != 0 {
        return Err(ScratchArchiveError::UnsupportedCompression);
    }
    if project.uncompressed_size > MAX_PROJECT_JSON_BYTES as u64 {
        return Err(ScratchArchiveError::ProjectJsonTooLarge);
    }
    let project_bytes = stored_entry_bytes(bytes, project, central_offset)?;
    let project_text =
        std::str::from_utf8(project_bytes).map_err(|_| ScratchArchiveError::ProjectJsonNotUtf8)?;
    let mut inventory = inspect_project_json(project_text)?;
    inventory.archive_entries = entries.len();
    inventory.total_uncompressed_bytes = total_uncompressed;
    Ok(inventory)
}

fn stored_entry_bytes<'a>(
    bytes: &'a [u8],
    entry: &Entry<'_>,
    central_offset: usize,
) -> Result<&'a [u8], ScratchArchiveError> {
    let offset = entry.local_offset;
    if read_u32(bytes, offset)? != LOCAL_SIGNATURE {
        return Err(ScratchArchiveError::MalformedZip);
    }
    let local_flags = read_u16(bytes, offset + 6)?;
    let local_method = read_u16(bytes, offset + 8)?;
    let name_len = usize::from(read_u16(bytes, offset + 26)?);
    let extra_len = usize::from(read_u16(bytes, offset + 28)?);
    if local_flags != entry.flags || local_method != entry.method {
        return Err(ScratchArchiveError::MalformedZip);
    }
    let name_start = offset
        .checked_add(30)
        .ok_or(ScratchArchiveError::MalformedZip)?;
    let name_end = name_start
        .checked_add(name_len)
        .ok_or(ScratchArchiveError::MalformedZip)?;
    if bytes.get(name_start..name_end) != Some(entry.name.as_bytes()) {
        return Err(ScratchArchiveError::MalformedZip);
    }
    let data_start = name_end
        .checked_add(extra_len)
        .ok_or(ScratchArchiveError::MalformedZip)?;
    let compressed_size =
        usize::try_from(entry.compressed_size).map_err(|_| ScratchArchiveError::SizeLimit)?;
    let data_end = data_start
        .checked_add(compressed_size)
        .ok_or(ScratchArchiveError::SizeLimit)?;
    if data_end > central_offset {
        return Err(ScratchArchiveError::MalformedZip);
    }
    bytes
        .get(data_start..data_end)
        .ok_or(ScratchArchiveError::MalformedZip)
}

fn inspect_project_json(text: &str) -> Result<ScratchProjectInventory, ScratchArchiveError> {
    if text.len() > MAX_PROJECT_JSON_BYTES {
        return Err(ScratchArchiveError::ProjectJsonTooLarge);
    }
    let Ok(false) = has_duplicate_or_excess_members(text, SCRATCH_JSON_LIMITS) else {
        return Err(ScratchArchiveError::MalformedOrOverBudgetJson);
    };
    let root = serde_json::from_str::<Value>(text)
        .map_err(|_| ScratchArchiveError::MalformedOrOverBudgetJson)?;
    if count_json_nodes(&root)? > MAX_JSON_NODES {
        return Err(ScratchArchiveError::MalformedOrOverBudgetJson);
    }
    let root = root
        .as_object()
        .ok_or(ScratchArchiveError::UnsupportedProjectShape)?;
    let targets = root
        .get("targets")
        .and_then(Value::as_array)
        .ok_or(ScratchArchiveError::UnsupportedProjectShape)?;
    if targets.len() > MAX_TARGETS {
        return Err(ScratchArchiveError::UnsupportedProjectShape);
    }
    let extensions = match root.get("extensions") {
        None => &[][..],
        Some(Value::Array(values)) if values.len() <= MAX_EXTENSIONS => values.as_slice(),
        _ => return Err(ScratchArchiveError::UnsupportedProjectShape),
    };
    if !extensions.iter().all(|value| {
        value.as_str().is_some_and(|value| {
            !value.is_empty()
                && value.len() <= 128
                && value.is_ascii()
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        })
    }) {
        return Err(ScratchArchiveError::UnsupportedProjectShape);
    }

    let mut block_count = 0usize;
    let mut event_hat_count = 0usize;
    let mut opcode_count = 0usize;
    for target in targets {
        let target = target
            .as_object()
            .ok_or(ScratchArchiveError::UnsupportedProjectShape)?;
        let blocks = target
            .get("blocks")
            .and_then(Value::as_object)
            .ok_or(ScratchArchiveError::UnsupportedProjectShape)?;
        block_count = block_count
            .checked_add(blocks.len())
            .ok_or(ScratchArchiveError::UnsupportedProjectShape)?;
        if block_count > MAX_BLOCKS {
            return Err(ScratchArchiveError::UnsupportedProjectShape);
        }
        for block in blocks.values() {
            let Some(block) = block.as_object() else {
                return Err(ScratchArchiveError::UnsupportedProjectShape);
            };
            let Some(opcode) = block.get("opcode").and_then(Value::as_str) else {
                return Err(ScratchArchiveError::UnsupportedProjectShape);
            };
            if opcode.is_empty()
                || opcode.len() > 128
                || !opcode.is_ascii()
                || !opcode
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
            {
                return Err(ScratchArchiveError::UnsupportedProjectShape);
            }
            opcode_count += 1;
            if opcode.starts_with("event_") && block.get("topLevel") == Some(&Value::Bool(true)) {
                event_hat_count += 1;
            }
        }
    }
    Ok(ScratchProjectInventory {
        archive_entries: 0,
        total_uncompressed_bytes: 0,
        target_count: targets.len(),
        block_count,
        event_hat_count,
        opcode_count,
        declared_extension_count: extensions.len(),
    })
}

fn count_json_nodes(value: &Value) -> Result<usize, ScratchArchiveError> {
    let mut stack = vec![value];
    let mut count = 0usize;
    while let Some(value) = stack.pop() {
        count = count
            .checked_add(1)
            .ok_or(ScratchArchiveError::MalformedOrOverBudgetJson)?;
        if count > MAX_JSON_NODES {
            return Ok(count);
        }
        match value {
            Value::Array(values) => stack.extend(values),
            Value::Object(values) => stack.extend(values.values()),
            _ => {}
        }
    }
    Ok(count)
}

fn validate_archive_path(path: &str) -> Result<(), ScratchArchiveError> {
    if path.starts_with('/')
        || path.starts_with('\\')
        || path.contains('\\')
        || path.contains('\0')
        || path.as_bytes().get(1).is_some_and(|byte| *byte == b':')
        || path
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
    {
        Err(ScratchArchiveError::UnsafePath)
    } else {
        Ok(())
    }
}

fn find_eocd(bytes: &[u8]) -> Option<usize> {
    let min = bytes.len().saturating_sub(65_557);
    let mut cursor = bytes.len().checked_sub(22)?;
    loop {
        if bytes
            .get(cursor..cursor + 4)
            .is_some_and(|slice| slice == EOCD_SIGNATURE.to_le_bytes())
        {
            return Some(cursor);
        }
        if cursor == min {
            return None;
        }
        cursor -= 1;
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, ScratchArchiveError> {
    let end = offset
        .checked_add(2)
        .ok_or(ScratchArchiveError::MalformedZip)?;
    let slice = bytes
        .get(offset..end)
        .ok_or(ScratchArchiveError::MalformedZip)?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, ScratchArchiveError> {
    let end = offset
        .checked_add(4)
        .ok_or(ScratchArchiveError::MalformedZip)?;
    let slice = bytes
        .get(offset..end)
        .ok_or(ScratchArchiveError::MalformedZip)?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct TestEntry<'a> {
        name: &'a str,
        data: &'a [u8],
        method: u16,
        flags: u16,
        external_attributes: u32,
        declared_compressed: Option<u32>,
        declared_uncompressed: Option<u32>,
    }

    fn stored<'a>(name: &'a str, data: &'a [u8]) -> TestEntry<'a> {
        TestEntry {
            name,
            data,
            method: 0,
            flags: 0,
            external_attributes: 0,
            declared_compressed: None,
            declared_uncompressed: None,
        }
    }

    fn archive(entries: &[TestEntry<'_>]) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut central = Vec::new();
        for entry in entries {
            let offset = u32::try_from(bytes.len()).expect("test archive offset");
            let compressed = entry
                .declared_compressed
                .unwrap_or(u32::try_from(entry.data.len()).expect("test data size"));
            let uncompressed = entry
                .declared_uncompressed
                .unwrap_or(u32::try_from(entry.data.len()).expect("test data size"));
            bytes.extend_from_slice(&LOCAL_SIGNATURE.to_le_bytes());
            bytes.extend_from_slice(&20u16.to_le_bytes());
            bytes.extend_from_slice(&entry.flags.to_le_bytes());
            bytes.extend_from_slice(&entry.method.to_le_bytes());
            bytes.extend_from_slice(&[0; 4]);
            bytes.extend_from_slice(&0u32.to_le_bytes());
            bytes.extend_from_slice(&compressed.to_le_bytes());
            bytes.extend_from_slice(&uncompressed.to_le_bytes());
            bytes.extend_from_slice(&(entry.name.len() as u16).to_le_bytes());
            bytes.extend_from_slice(&0u16.to_le_bytes());
            bytes.extend_from_slice(entry.name.as_bytes());
            bytes.extend_from_slice(entry.data);

            central.extend_from_slice(&CENTRAL_SIGNATURE.to_le_bytes());
            central.extend_from_slice(&0x0314u16.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&entry.flags.to_le_bytes());
            central.extend_from_slice(&entry.method.to_le_bytes());
            central.extend_from_slice(&[0; 4]);
            central.extend_from_slice(&0u32.to_le_bytes());
            central.extend_from_slice(&compressed.to_le_bytes());
            central.extend_from_slice(&uncompressed.to_le_bytes());
            central.extend_from_slice(&(entry.name.len() as u16).to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&entry.external_attributes.to_le_bytes());
            central.extend_from_slice(&offset.to_le_bytes());
            central.extend_from_slice(entry.name.as_bytes());
        }
        let central_offset = u32::try_from(bytes.len()).expect("central offset");
        let central_size = u32::try_from(central.len()).expect("central size");
        bytes.extend_from_slice(&central);
        bytes.extend_from_slice(&EOCD_SIGNATURE.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        bytes.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        bytes.extend_from_slice(&central_size.to_le_bytes());
        bytes.extend_from_slice(&central_offset.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes
    }

    const PROJECT: &[u8] = br#"{"targets":[{"isStage":true,"blocks":{"a":{"opcode":"event_whenflagclicked","topLevel":true},"b":{"opcode":"motion_movesteps","topLevel":false}}}],"extensions":["pen"]}"#;

    #[test]
    fn stored_project_json_yields_only_bounded_source_free_counts() {
        let bytes = archive(&[
            stored("project.json", PROJECT),
            stored("asset.svg", b"<svg/>"),
        ]);
        assert_eq!(
            inspect_sb3(&bytes),
            Ok(ScratchProjectInventory {
                archive_entries: 2,
                total_uncompressed_bytes: (PROJECT.len() + 6) as u64,
                target_count: 1,
                block_count: 2,
                event_hat_count: 1,
                opcode_count: 2,
                declared_extension_count: 1,
            })
        );
        let debug = format!("{:?}", inspect_sb3(&bytes));
        assert!(!debug.contains("event_whenflagclicked"));
        assert!(!debug.contains("asset.svg"));
    }

    #[test]
    fn rejects_traversal_absolute_drive_backslash_duplicate_and_symlink_paths() {
        for path in [
            "../project.json",
            "/project.json",
            "C:/project.json",
            "dir\\project.json",
        ] {
            let bytes = archive(&[stored(path, PROJECT)]);
            assert_eq!(
                inspect_sb3(&bytes),
                Err(ScratchArchiveError::UnsafePath),
                "{path}"
            );
        }
        let bytes = archive(&[
            stored("project.json", PROJECT),
            stored("project.json", PROJECT),
        ]);
        assert_eq!(inspect_sb3(&bytes), Err(ScratchArchiveError::DuplicatePath));

        let mut symlink = stored("project.json", PROJECT);
        symlink.external_attributes = 0o120777 << 16;
        let bytes = archive(&[symlink]);
        assert_eq!(inspect_sb3(&bytes), Err(ScratchArchiveError::SymlinkEntry));
    }

    #[test]
    fn rejects_ratio_bombs_oversize_entries_and_deflated_project_json() {
        let mut bomb = stored("project.json", PROJECT);
        bomb.method = 8;
        bomb.declared_compressed = Some(1);
        bomb.declared_uncompressed = Some(201);
        let bytes = archive(&[bomb]);
        assert_eq!(
            inspect_sb3(&bytes),
            Err(ScratchArchiveError::CompressionRatio)
        );

        let mut oversized = stored("project.json", PROJECT);
        oversized.declared_compressed = Some((MAX_ENTRY_UNCOMPRESSED_BYTES + 1) as u32);
        oversized.declared_uncompressed = Some((MAX_ENTRY_UNCOMPRESSED_BYTES + 1) as u32);
        let bytes = archive(&[oversized]);
        assert_eq!(inspect_sb3(&bytes), Err(ScratchArchiveError::SizeLimit));

        let mut deflated = stored("project.json", PROJECT);
        deflated.method = 8;
        let bytes = archive(&[deflated]);
        assert_eq!(
            inspect_sb3(&bytes),
            Err(ScratchArchiveError::UnsupportedCompression)
        );
    }

    #[test]
    fn rejects_duplicate_deep_or_unsupported_project_json() {
        for project in [
            br#"{"targets":[],"targets":[],"extensions":[]}"#.as_slice(),
            br#"{"targets":[{"blocks":{"a":{"opcode":"bad/opcode"}}}],"extensions":[]}"#.as_slice(),
            br#"{"targets":"not-an-array","extensions":[]}"#.as_slice(),
        ] {
            let bytes = archive(&[stored("project.json", project)]);
            assert!(matches!(
                inspect_sb3(&bytes),
                Err(ScratchArchiveError::MalformedOrOverBudgetJson)
                    | Err(ScratchArchiveError::UnsupportedProjectShape)
            ));
        }
        let deep = format!("{}0{}", "[".repeat(129), "]".repeat(129));
        let bytes = archive(&[stored("project.json", deep.as_bytes())]);
        assert_eq!(
            inspect_sb3(&bytes),
            Err(ScratchArchiveError::MalformedOrOverBudgetJson)
        );
    }

    #[test]
    fn malformed_zip_and_missing_project_fail_closed() {
        assert_eq!(
            inspect_sb3(b"not zip"),
            Err(ScratchArchiveError::MalformedZip)
        );
        let bytes = archive(&[stored("asset.svg", b"<svg/>")]);
        assert_eq!(
            inspect_sb3(&bytes),
            Err(ScratchArchiveError::MissingOrDuplicateProjectJson)
        );

        let mut encrypted = stored("project.json", PROJECT);
        encrypted.flags = 1;
        assert_eq!(
            inspect_sb3(&archive(&[encrypted])),
            Err(ScratchArchiveError::EncryptedEntry)
        );

        let mut unsupported = stored("project.json", PROJECT);
        unsupported.method = 99;
        assert_eq!(
            inspect_sb3(&archive(&[unsupported])),
            Err(ScratchArchiveError::UnsupportedCompression)
        );

        let mut incoherent = archive(&[stored("project.json", PROJECT)]);
        incoherent[8] = 8;
        assert_eq!(
            inspect_sb3(&incoherent),
            Err(ScratchArchiveError::MalformedZip)
        );
    }
}
