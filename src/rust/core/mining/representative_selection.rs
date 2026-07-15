//! Representative selection chooses evidence that explains a family compactly.

use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepresentativeSelectionPolicy {
    ClosestToTemplate,
    CoversKeyDifferences,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EvidenceCoverage {
    Canonical,
    Support,
    Variation,
    Exception,
}

impl EvidenceCoverage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Canonical => "canonical",
            Self::Support => "support",
            Self::Variation => "variation",
            Self::Exception => "exception",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceSelectionCandidate {
    pub stable_id: String,
    pub estimated_tokens: usize,
    pub coverage: BTreeSet<EvidenceCoverage>,
    pub source_order: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepresentativeEvidenceSelection {
    pub selected_ids: Vec<String>,
    pub covered: BTreeSet<EvidenceCoverage>,
    pub missing: BTreeSet<EvidenceCoverage>,
    pub estimated_tokens: usize,
    pub budget_satisfied: bool,
}

pub fn select_representative_evidence(
    candidates: &[EvidenceSelectionCandidate],
    required_coverage: &BTreeSet<EvidenceCoverage>,
    token_budget: Option<usize>,
) -> RepresentativeEvidenceSelection {
    let mut selected_ids = Vec::new();
    let mut selected_indexes = BTreeSet::new();
    let mut covered = BTreeSet::new();
    let mut estimated_tokens = 0usize;
    let mut budget_satisfied = true;

    while !required_coverage.is_subset(&covered) {
        let Some(index) = best_candidate(
            candidates,
            required_coverage,
            &covered,
            &selected_indexes,
            token_budget,
            estimated_tokens,
        ) else {
            break;
        };
        let candidate = &candidates[index];
        if let Some(budget) = token_budget {
            if estimated_tokens.saturating_add(candidate.estimated_tokens) > budget {
                budget_satisfied = false;
            }
        }
        selected_indexes.insert(index);
        selected_ids.push(candidate.stable_id.clone());
        estimated_tokens = estimated_tokens.saturating_add(candidate.estimated_tokens);
        covered.extend(candidate.coverage.intersection(required_coverage).copied());
    }

    let missing = required_coverage
        .difference(&covered)
        .copied()
        .collect::<BTreeSet<_>>();

    RepresentativeEvidenceSelection {
        selected_ids,
        covered,
        missing,
        estimated_tokens,
        budget_satisfied,
    }
}

fn best_candidate(
    candidates: &[EvidenceSelectionCandidate],
    required_coverage: &BTreeSet<EvidenceCoverage>,
    covered: &BTreeSet<EvidenceCoverage>,
    selected_indexes: &BTreeSet<usize>,
    token_budget: Option<usize>,
    used_tokens: usize,
) -> Option<usize> {
    let mut best: Option<usize> = None;
    for (index, candidate) in candidates.iter().enumerate() {
        if selected_indexes.contains(&index) {
            continue;
        }
        let marginal = marginal_coverage(candidate, required_coverage, covered);
        if marginal == 0 {
            continue;
        }
        let cost = candidate.estimated_tokens.max(1);
        if let Some(budget) = token_budget {
            let would_exceed = used_tokens.saturating_add(cost) > budget;
            let required_canonical = required_coverage.contains(&EvidenceCoverage::Canonical)
                && !covered.contains(&EvidenceCoverage::Canonical)
                && candidate.coverage.contains(&EvidenceCoverage::Canonical);
            if would_exceed && !required_canonical {
                continue;
            }
        }
        if best
            .map(|best_index| {
                let incumbent = &candidates[best_index];
                let incumbent_marginal = marginal_coverage(incumbent, required_coverage, covered);
                better_candidate(candidate, marginal, incumbent, incumbent_marginal)
            })
            .unwrap_or(true)
        {
            best = Some(index);
        }
    }
    best
}

fn marginal_coverage(
    candidate: &EvidenceSelectionCandidate,
    required_coverage: &BTreeSet<EvidenceCoverage>,
    covered: &BTreeSet<EvidenceCoverage>,
) -> usize {
    candidate
        .coverage
        .iter()
        .filter(|coverage| required_coverage.contains(coverage) && !covered.contains(coverage))
        .count()
}

fn better_candidate(
    candidate: &EvidenceSelectionCandidate,
    candidate_marginal: usize,
    incumbent: &EvidenceSelectionCandidate,
    incumbent_marginal: usize,
) -> bool {
    let candidate_cost = candidate.estimated_tokens.max(1);
    let incumbent_cost = incumbent.estimated_tokens.max(1);
    let left = candidate_marginal.saturating_mul(incumbent_cost);
    let right = incumbent_marginal.saturating_mul(candidate_cost);
    if left != right {
        return left > right;
    }
    if candidate_marginal != incumbent_marginal {
        return candidate_marginal > incumbent_marginal;
    }
    if candidate_cost != incumbent_cost {
        return candidate_cost < incumbent_cost;
    }
    candidate.source_order < incumbent.source_order
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(
        id: &str,
        estimated_tokens: usize,
        coverage: &[EvidenceCoverage],
        source_order: usize,
    ) -> EvidenceSelectionCandidate {
        EvidenceSelectionCandidate {
            stable_id: id.to_string(),
            estimated_tokens,
            coverage: coverage.iter().copied().collect(),
            source_order,
        }
    }

    #[test]
    fn greedy_selection_prefers_marginal_coverage_per_token() {
        let required = [
            EvidenceCoverage::Canonical,
            EvidenceCoverage::Support,
            EvidenceCoverage::Variation,
        ]
        .into_iter()
        .collect();
        let candidates = vec![
            candidate(
                "canonical-expensive",
                30,
                &[EvidenceCoverage::Canonical, EvidenceCoverage::Support],
                0,
            ),
            candidate("variation-cheap", 4, &[EvidenceCoverage::Variation], 1),
            candidate("canonical-cheap", 8, &[EvidenceCoverage::Canonical], 2),
            candidate("support-cheap", 5, &[EvidenceCoverage::Support], 3),
        ];

        let selection = select_representative_evidence(&candidates, &required, Some(20));

        assert_eq!(
            selection.selected_ids,
            vec!["variation-cheap", "support-cheap", "canonical-cheap"]
        );
        assert!(selection.missing.is_empty());
        assert!(selection.budget_satisfied);
        assert_eq!(selection.estimated_tokens, 17);
    }

    #[test]
    fn mandatory_canonical_seed_is_reported_when_budget_is_too_small() {
        let required = [EvidenceCoverage::Canonical, EvidenceCoverage::Support]
            .into_iter()
            .collect();
        let candidates = vec![candidate(
            "canonical",
            50,
            &[EvidenceCoverage::Canonical, EvidenceCoverage::Support],
            0,
        )];

        let selection = select_representative_evidence(&candidates, &required, Some(1));

        assert_eq!(selection.selected_ids, vec!["canonical"]);
        assert!(selection.missing.is_empty());
        assert!(!selection.budget_satisfied);
        assert_eq!(selection.estimated_tokens, 50);
    }

    #[test]
    fn canonical_seed_can_exceed_budget_after_other_coverage() {
        let required = [EvidenceCoverage::Canonical, EvidenceCoverage::Variation]
            .into_iter()
            .collect();
        let candidates = vec![
            candidate("variation", 1, &[EvidenceCoverage::Variation], 0),
            candidate("canonical", 50, &[EvidenceCoverage::Canonical], 1),
        ];

        let selection = select_representative_evidence(&candidates, &required, Some(10));

        assert_eq!(selection.selected_ids, vec!["variation", "canonical"]);
        assert!(selection.missing.is_empty());
        assert!(!selection.budget_satisfied);
    }

    #[test]
    fn incidental_canonical_coverage_does_not_bypass_budget_when_not_required() {
        // Canonical is NOT required. The only candidate covers the required
        // Support but also incidentally covers Canonical and exceeds the budget.
        // It must not be force-selected on the strength of that incidental
        // Canonical coverage; the mandatory-seed bypass applies only when
        // Canonical is actually required.
        let required = [EvidenceCoverage::Support].into_iter().collect();
        let candidates = vec![candidate(
            "expensive-incidental-canonical",
            100,
            &[EvidenceCoverage::Canonical, EvidenceCoverage::Support],
            0,
        )];

        let selection = select_representative_evidence(&candidates, &required, Some(10));

        assert!(selection.selected_ids.is_empty());
        assert_eq!(
            selection.missing,
            [EvidenceCoverage::Support].into_iter().collect()
        );
        assert!(selection.budget_satisfied);
        assert_eq!(selection.estimated_tokens, 0);
    }

    #[test]
    fn missing_coverage_is_explicit_when_no_candidate_can_cover_it() {
        let required = [EvidenceCoverage::Canonical, EvidenceCoverage::Exception]
            .into_iter()
            .collect();
        let candidates = vec![candidate("canonical", 8, &[EvidenceCoverage::Canonical], 0)];

        let selection = select_representative_evidence(&candidates, &required, None);

        assert_eq!(selection.selected_ids, vec!["canonical"]);
        assert_eq!(
            selection.missing,
            [EvidenceCoverage::Exception].into_iter().collect()
        );
        assert!(selection.budget_satisfied);
    }
}
