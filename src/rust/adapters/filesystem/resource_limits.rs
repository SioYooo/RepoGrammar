use crate::ports::file_discovery::{
    FileDiscoveryError, FileDiscoveryLimitExceeded, FileDiscoveryLimitKind,
    DEFAULT_MAX_ACCEPTED_BYTES, DEFAULT_MAX_ACCEPTED_FILES, DEFAULT_MAX_DIRECTORY_DEPTH,
    DEFAULT_MAX_REPORTED_SKIPPED_PATHS, DEFAULT_MAX_VISITED_ENTRIES,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DiscoveryLimits {
    pub accepted_files: u64,
    pub accepted_bytes: u64,
    pub reported_skipped_paths: u64,
    pub visited_entries: u64,
    pub directory_depth: u64,
}

impl Default for DiscoveryLimits {
    fn default() -> Self {
        Self {
            accepted_files: DEFAULT_MAX_ACCEPTED_FILES,
            accepted_bytes: DEFAULT_MAX_ACCEPTED_BYTES,
            reported_skipped_paths: DEFAULT_MAX_REPORTED_SKIPPED_PATHS,
            visited_entries: DEFAULT_MAX_VISITED_ENTRIES,
            directory_depth: DEFAULT_MAX_DIRECTORY_DEPTH,
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct DiscoveryResourceBudget {
    limits: DiscoveryLimits,
    accepted_files: u64,
    accepted_bytes: u64,
    reported_skipped_paths: u64,
    visited_entries: u64,
}

impl DiscoveryResourceBudget {
    pub fn new(limits: DiscoveryLimits) -> Self {
        Self {
            limits,
            ..Self::default()
        }
    }

    pub fn check_directory_depth(&self, depth: u64) -> Result<(), FileDiscoveryError> {
        ensure_within_limit(
            FileDiscoveryLimitKind::DirectoryDepth,
            self.limits.directory_depth,
            depth,
        )
    }

    pub fn record_visited_entry(&mut self) -> Result<(), FileDiscoveryError> {
        self.visited_entries = checked_observed(
            FileDiscoveryLimitKind::VisitedEntries,
            self.limits.visited_entries,
            self.visited_entries,
            1,
        )?;
        Ok(())
    }

    pub fn record_skipped_path(&mut self) -> Result<(), FileDiscoveryError> {
        self.reported_skipped_paths = checked_observed(
            FileDiscoveryLimitKind::ReportedSkippedPaths,
            self.limits.reported_skipped_paths,
            self.reported_skipped_paths,
            1,
        )?;
        Ok(())
    }

    pub fn record_accepted_file(&mut self, size_bytes: u64) -> Result<(), FileDiscoveryError> {
        let next_files = checked_observed(
            FileDiscoveryLimitKind::AcceptedFiles,
            self.limits.accepted_files,
            self.accepted_files,
            1,
        )?;
        let next_bytes = checked_observed(
            FileDiscoveryLimitKind::AcceptedBytes,
            self.limits.accepted_bytes,
            self.accepted_bytes,
            size_bytes,
        )?;
        self.accepted_files = next_files;
        self.accepted_bytes = next_bytes;
        Ok(())
    }
}

fn checked_observed(
    kind: FileDiscoveryLimitKind,
    limit: u64,
    current: u64,
    increment: u64,
) -> Result<u64, FileDiscoveryError> {
    let observed = current.checked_add(increment).ok_or({
        FileDiscoveryError::ResourceLimitExceeded(FileDiscoveryLimitExceeded {
            kind,
            limit,
            observed: u64::MAX,
        })
    })?;
    ensure_within_limit(kind, limit, observed)?;
    Ok(observed)
}

fn ensure_within_limit(
    kind: FileDiscoveryLimitKind,
    limit: u64,
    observed: u64,
) -> Result<(), FileDiscoveryError> {
    if observed > limit {
        return Err(FileDiscoveryError::ResourceLimitExceeded(
            FileDiscoveryLimitExceeded {
                kind,
                limit,
                observed,
            },
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> DiscoveryLimits {
        DiscoveryLimits {
            accepted_files: 1,
            accepted_bytes: 2,
            reported_skipped_paths: 1,
            visited_entries: 1,
            directory_depth: 1,
        }
    }

    #[test]
    fn every_budget_is_inclusive_and_reports_limit_plus_one() {
        let mut budget = DiscoveryResourceBudget::new(limits());
        budget.check_directory_depth(1).expect("exact depth");
        assert_limit(
            budget.check_directory_depth(2),
            FileDiscoveryLimitKind::DirectoryDepth,
            1,
            2,
        );

        budget.record_visited_entry().expect("exact visited entry");
        assert_limit(
            budget.record_visited_entry(),
            FileDiscoveryLimitKind::VisitedEntries,
            1,
            2,
        );

        budget.record_skipped_path().expect("exact skipped path");
        assert_limit(
            budget.record_skipped_path(),
            FileDiscoveryLimitKind::ReportedSkippedPaths,
            1,
            2,
        );

        budget
            .record_accepted_file(2)
            .expect("exact accepted file and bytes");
        assert_limit(
            budget.record_accepted_file(0),
            FileDiscoveryLimitKind::AcceptedFiles,
            1,
            2,
        );

        let mut byte_budget = DiscoveryResourceBudget::new(DiscoveryLimits {
            accepted_files: 2,
            ..limits()
        });
        byte_budget.record_accepted_file(2).expect("exact bytes");
        assert_limit(
            byte_budget.record_accepted_file(1),
            FileDiscoveryLimitKind::AcceptedBytes,
            2,
            3,
        );
        byte_budget
            .record_accepted_file(0)
            .expect("byte failure must not consume the file slot");
    }

    fn assert_limit(
        result: Result<(), FileDiscoveryError>,
        kind: FileDiscoveryLimitKind,
        limit: u64,
        observed: u64,
    ) {
        assert_eq!(
            result,
            Err(FileDiscoveryError::ResourceLimitExceeded(
                FileDiscoveryLimitExceeded {
                    kind,
                    limit,
                    observed,
                }
            ))
        );
    }
}
