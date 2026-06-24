//! Representative selection chooses evidence that explains a family compactly.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepresentativeSelectionPolicy {
    ClosestToTemplate,
    CoversKeyDifferences,
}
