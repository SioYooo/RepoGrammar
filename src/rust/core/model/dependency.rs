//! Language-neutral dependency inventory and reviewed library-contract types.
//!
//! A manifest declaration, a lockfile resolution, a semantic-provider result,
//! and a reviewed library behavior contract are different evidence levels.
//! These owned types keep those levels distinct so adapters can inventory any
//! third-party package without pretending that package presence proves runtime
//! behavior.

use super::{Evidence, TypedUnknown};

const MAX_PACKAGE_NAME_CHARS: usize = 512;
const MAX_VERSION_TEXT_CHARS: usize = 256;
const MAX_CONTRACT_ID_CHARS: usize = 160;
const MAX_CONTRACT_EXACT_VERSIONS: usize = 64;
const MAX_LIBRARY_CONTRACTS: usize = 4_096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DependencyEcosystem {
    Pypi,
    Npm,
    Maven,
    Nuget,
    Cargo,
    GoModules,
    Composer,
    RubyGems,
    SwiftPackageManager,
    Cran,
    Bioconductor,
    DelphiPackage,
    Alire,
    Fpm,
    MatlabAddOn,
    SqlExtension,
    ScratchExtension,
    Vcpkg,
    Conan,
    NativeSystem,
}

impl DependencyEcosystem {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pypi => "pypi",
            Self::Npm => "npm",
            Self::Maven => "maven",
            Self::Nuget => "nuget",
            Self::Cargo => "cargo",
            Self::GoModules => "go_modules",
            Self::Composer => "composer",
            Self::RubyGems => "rubygems",
            Self::SwiftPackageManager => "swift_package_manager",
            Self::Cran => "cran",
            Self::Bioconductor => "bioconductor",
            Self::DelphiPackage => "delphi_package",
            Self::Alire => "alire",
            Self::Fpm => "fpm",
            Self::MatlabAddOn => "matlab_add_on",
            Self::SqlExtension => "sql_extension",
            Self::ScratchExtension => "scratch_extension",
            Self::Vcpkg => "vcpkg",
            Self::Conan => "conan",
            Self::NativeSystem => "native_system",
        }
    }

    pub fn parse_str(value: &str) -> Result<Self, String> {
        match value {
            "pypi" => Ok(Self::Pypi),
            "npm" => Ok(Self::Npm),
            "maven" => Ok(Self::Maven),
            "nuget" => Ok(Self::Nuget),
            "cargo" => Ok(Self::Cargo),
            "go_modules" => Ok(Self::GoModules),
            "composer" => Ok(Self::Composer),
            "rubygems" => Ok(Self::RubyGems),
            "swift_package_manager" => Ok(Self::SwiftPackageManager),
            "cran" => Ok(Self::Cran),
            "bioconductor" => Ok(Self::Bioconductor),
            "delphi_package" => Ok(Self::DelphiPackage),
            "alire" => Ok(Self::Alire),
            "fpm" => Ok(Self::Fpm),
            "matlab_add_on" => Ok(Self::MatlabAddOn),
            "sql_extension" => Ok(Self::SqlExtension),
            "scratch_extension" => Ok(Self::ScratchExtension),
            "vcpkg" => Ok(Self::Vcpkg),
            "conan" => Ok(Self::Conan),
            "native_system" => Ok(Self::NativeSystem),
            _ => Err(format!("unsupported dependency ecosystem {value}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PackageIdentity {
    pub ecosystem: DependencyEcosystem,
    pub name: String,
}

impl PackageIdentity {
    pub fn new(ecosystem: DependencyEcosystem, name: impl Into<String>) -> Result<Self, String> {
        Ok(Self {
            ecosystem,
            name: validate_untrusted_text("package name", name, MAX_PACKAGE_NAME_CHARS)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DependencyVersion(String);

impl DependencyVersion {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        validate_untrusted_text("dependency version", value, MAX_VERSION_TEXT_CHARS).map(Self)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DependencyScope {
    Runtime,
    Development,
    Test,
    Build,
    Unknown,
}

/// Whether repository evidence proves a package is direct, transitive, or
/// leaves that relationship unresolved. Lockfiles commonly enumerate a graph
/// without identifying which pins were declared by the root manifest, so a
/// boolean would incorrectly collapse `Unknown` into `Transitive`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DependencyDirectness {
    Direct,
    Transitive,
    Unknown,
}

impl DependencyDirectness {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Transitive => "transitive",
            Self::Unknown => "unknown",
        }
    }

    pub fn parse_str(value: &str) -> Result<Self, String> {
        match value {
            "direct" => Ok(Self::Direct),
            "transitive" => Ok(Self::Transitive),
            "unknown" => Ok(Self::Unknown),
            _ => Err(format!("unsupported dependency directness {value}")),
        }
    }
}

impl DependencyScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Runtime => "runtime",
            Self::Development => "development",
            Self::Test => "test",
            Self::Build => "build",
            Self::Unknown => "unknown",
        }
    }

    pub fn parse_str(value: &str) -> Result<Self, String> {
        match value {
            "runtime" => Ok(Self::Runtime),
            "development" => Ok(Self::Development),
            "test" => Ok(Self::Test),
            "build" => Ok(Self::Build),
            "unknown" => Ok(Self::Unknown),
            _ => Err(format!("unsupported dependency scope {value}")),
        }
    }
}

/// Strongest evidence that established a dependency record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DependencyEvidenceLevel {
    ManifestDeclared,
    LockfileResolved,
    ProviderResolved,
}

impl DependencyEvidenceLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ManifestDeclared => "manifest_declared",
            Self::LockfileResolved => "lockfile_resolved",
            Self::ProviderResolved => "provider_resolved",
        }
    }

    pub fn parse_str(value: &str) -> Result<Self, String> {
        match value {
            "manifest_declared" => Ok(Self::ManifestDeclared),
            "lockfile_resolved" => Ok(Self::LockfileResolved),
            "provider_resolved" => Ok(Self::ProviderResolved),
            _ => Err(format!("unsupported dependency evidence level {value}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyRecord {
    pub package: PackageIdentity,
    pub requirement: Option<DependencyVersion>,
    pub resolved_version: Option<DependencyVersion>,
    pub scope: DependencyScope,
    pub optional: bool,
    pub directness: DependencyDirectness,
    pub evidence_level: DependencyEvidenceLevel,
    pub evidence: Evidence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencySnapshot {
    pub dependencies: Vec<DependencyRecord>,
    pub unknowns: Vec<TypedUnknown>,
}

impl DependencySnapshot {
    pub fn new(
        dependencies: impl IntoIterator<Item = DependencyRecord>,
        unknowns: impl IntoIterator<Item = TypedUnknown>,
    ) -> Result<Self, String> {
        let mut dependencies = dependencies.into_iter().collect::<Vec<_>>();
        dependencies
            .sort_by(|left, right| dependency_sort_key(left).cmp(&dependency_sort_key(right)));
        for pair in dependencies.windows(2) {
            if dependency_sort_key(&pair[0]) == dependency_sort_key(&pair[1]) {
                return Err("dependency snapshot records must be unique".to_string());
            }
        }
        let mut unknowns = unknowns.into_iter().collect::<Vec<_>>();
        unknowns.sort_by(|left, right| unknown_sort_key(left).cmp(&unknown_sort_key(right)));
        unknowns.dedup_by(|left, right| unknown_sort_key(left) == unknown_sort_key(right));
        Ok(Self {
            dependencies,
            unknowns,
        })
    }
}

type DependencySortKey<'a> = (
    DependencyEcosystem,
    &'a str,
    DependencyScope,
    bool,
    DependencyDirectness,
    DependencyEvidenceLevel,
    Option<&'a str>,
    Option<&'a str>,
    &'a str,
);

fn dependency_sort_key(record: &DependencyRecord) -> DependencySortKey<'_> {
    (
        record.package.ecosystem,
        &record.package.name,
        record.scope,
        record.optional,
        record.directness,
        record.evidence_level,
        record.requirement.as_ref().map(DependencyVersion::as_str),
        record
            .resolved_version
            .as_ref()
            .map(DependencyVersion::as_str),
        &record.evidence.provenance.path,
    )
}

type UnknownSortKey<'a> = (&'static str, &'static str, &'a str, Option<&'a str>);

fn unknown_sort_key(unknown: &TypedUnknown) -> UnknownSortKey<'_> {
    (
        unknown.class.as_protocol_str(),
        unknown.reason.as_protocol_str(),
        &unknown.affected_claim,
        unknown.recovery.as_deref(),
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalSymbolId {
    pub package: PackageIdentity,
    pub qualified_name: String,
}

impl ExternalSymbolId {
    pub fn new(
        package: PackageIdentity,
        qualified_name: impl Into<String>,
    ) -> Result<Self, String> {
        Ok(Self {
            package,
            qualified_name: validate_untrusted_text(
                "external symbol qualified name",
                qualified_name,
                MAX_PACKAGE_NAME_CHARS,
            )?,
        })
    }
}

impl DependencyRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        package: PackageIdentity,
        requirement: Option<DependencyVersion>,
        resolved_version: Option<DependencyVersion>,
        scope: DependencyScope,
        optional: bool,
        directness: DependencyDirectness,
        evidence_level: DependencyEvidenceLevel,
        evidence: Evidence,
    ) -> Result<Self, String> {
        if evidence_level == DependencyEvidenceLevel::LockfileResolved && resolved_version.is_none()
        {
            return Err("lockfile-resolved dependency must include a resolved version".to_string());
        }
        Ok(Self {
            package,
            requirement,
            resolved_version,
            scope,
            optional,
            directness,
            evidence_level,
            evidence,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LibraryContractId(String);

impl LibraryContractId {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.is_empty()
            || value.len() > MAX_CONTRACT_ID_CHARS
            || !value.chars().all(|character| {
                character.is_ascii_lowercase()
                    || character.is_ascii_digit()
                    || matches!(character, '.' | '_' | '-')
            })
        {
            return Err(
                "library contract id must be a bounded lowercase source-free token".to_string(),
            );
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LibraryCapability {
    SymbolIdentity,
    CallSemantics,
    FrameworkRole,
    ConfigurationModel,
    DataflowEffect,
}

/// A finite, exact-version admission boundary for one reviewed contract.
///
/// Version ordering and range syntax differ by ecosystem. The language-neutral
/// core therefore does not compare opaque version strings or guess range
/// semantics. A contract explicitly lists the resolved versions its evidence
/// reviewed; wider ecosystem-native ranges require a separately qualified
/// matcher at an adapter boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryContractVersionSet(Vec<DependencyVersion>);

impl LibraryContractVersionSet {
    pub fn new(versions: impl IntoIterator<Item = DependencyVersion>) -> Result<Self, String> {
        let mut versions = versions.into_iter().collect::<Vec<_>>();
        if versions.is_empty() {
            return Err("library contract must admit at least one exact version".to_string());
        }
        if versions.len() > MAX_CONTRACT_EXACT_VERSIONS {
            return Err(
                "library contract exact-version set exceeds its resource limit".to_string(),
            );
        }
        versions.sort();
        versions.dedup();
        Ok(Self(versions))
    }

    pub fn versions(&self) -> &[DependencyVersion] {
        &self.0
    }

    pub fn admits(&self, version: &DependencyVersion) -> bool {
        self.0.binary_search(version).is_ok()
    }
}

/// A reviewed, versioned behavior contract for one package.
///
/// Package inventory never creates one of these automatically. Contract packs
/// are explicit reviewed inputs, and their monotonically increasing revision is
/// part of cache/provenance decisions at the application boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryContract {
    pub id: LibraryContractId,
    pub revision: u32,
    pub package: PackageIdentity,
    pub versions: LibraryContractVersionSet,
    pub capabilities: Vec<LibraryCapability>,
}

impl LibraryContract {
    pub fn new(
        id: LibraryContractId,
        revision: u32,
        package: PackageIdentity,
        versions: LibraryContractVersionSet,
        capabilities: impl IntoIterator<Item = LibraryCapability>,
    ) -> Result<Self, String> {
        if revision == 0 {
            return Err("library contract revision must be greater than zero".to_string());
        }
        let mut capabilities = capabilities.into_iter().collect::<Vec<_>>();
        capabilities.sort();
        capabilities.dedup();
        if capabilities.is_empty() {
            return Err("library contract must declare at least one capability".to_string());
        }
        Ok(Self {
            id,
            revision,
            package,
            versions,
            capabilities,
        })
    }
}

/// Result of consulting reviewed contracts for one dependency and capability.
///
/// A match is still only contract evidence. The caller must also prove the
/// source anchor and any package-qualified symbol obligation required by the
/// family; this result alone never establishes behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryContractResolution<'a> {
    Matched(&'a LibraryContract),
    NoContract,
    InsufficientDependencyEvidence,
}

/// Deterministic snapshot of reviewed library contracts.
///
/// The registry rejects duplicate ids and overlapping package/version/
/// capability claims. This makes lookup fail closed at construction time
/// instead of selecting whichever conflicting contract happened to be first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryContractRegistry {
    contracts: Vec<LibraryContract>,
}

impl LibraryContractRegistry {
    pub fn new(contracts: impl IntoIterator<Item = LibraryContract>) -> Result<Self, String> {
        let mut contracts = contracts.into_iter().collect::<Vec<_>>();
        if contracts.len() > MAX_LIBRARY_CONTRACTS {
            return Err("library contract registry exceeds its resource limit".to_string());
        }
        contracts.sort_by(|left, right| {
            (left.id.as_str(), left.revision).cmp(&(right.id.as_str(), right.revision))
        });

        for pair in contracts.windows(2) {
            if pair[0].id == pair[1].id {
                return Err("library contract ids must be unique within a registry".to_string());
            }
        }
        for (index, left) in contracts.iter().enumerate() {
            for right in &contracts[index + 1..] {
                if contracts_overlap(left, right) {
                    return Err(
                        "library contracts must not overlap package, version, and capability"
                            .to_string(),
                    );
                }
            }
        }

        Ok(Self { contracts })
    }

    pub fn contracts(&self) -> &[LibraryContract] {
        &self.contracts
    }

    pub fn resolve<'a>(
        &'a self,
        dependency: &DependencyRecord,
        capability: LibraryCapability,
    ) -> LibraryContractResolution<'a> {
        if dependency.evidence_level == DependencyEvidenceLevel::ManifestDeclared {
            return LibraryContractResolution::InsufficientDependencyEvidence;
        }
        let Some(version) = dependency.resolved_version.as_ref() else {
            return LibraryContractResolution::InsufficientDependencyEvidence;
        };
        self.contracts
            .iter()
            .find(|contract| {
                contract.package == dependency.package
                    && contract.versions.admits(version)
                    && contract.capabilities.binary_search(&capability).is_ok()
            })
            .map(LibraryContractResolution::Matched)
            .unwrap_or(LibraryContractResolution::NoContract)
    }
}

fn contracts_overlap(left: &LibraryContract, right: &LibraryContract) -> bool {
    left.package == right.package
        && left
            .versions
            .versions()
            .iter()
            .any(|version| right.versions.admits(version))
        && left
            .capabilities
            .iter()
            .any(|capability| right.capabilities.binary_search(capability).is_ok())
}

fn validate_untrusted_text(
    field: &'static str,
    value: impl Into<String>,
    max_chars: usize,
) -> Result<String, String> {
    let value = value.into();
    if value.trim().is_empty()
        || value.chars().count() > max_chars
        || value.chars().any(char::is_control)
    {
        Err(format!(
            "{field} must be non-empty bounded text without controls"
        ))
    } else {
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{
        CodeUnitId, ContentHash, Provenance, RepositoryRevision, SourceRange,
    };

    fn evidence() -> Evidence {
        Evidence::new(
            CodeUnitId::new("unit:Cargo.toml").expect("unit"),
            SourceRange::new(0, 12).expect("range"),
            Provenance::new(
                "Cargo.toml",
                ContentHash::new(format!("sha256:{}", "a".repeat(64))).expect("hash"),
                RepositoryRevision::new("revision").expect("revision"),
            )
            .expect("provenance"),
            "bounded manifest declaration",
        )
        .expect("evidence")
    }

    #[test]
    fn ecosystem_and_evidence_tokens_are_stable_and_source_free() {
        assert_eq!(DependencyEcosystem::Pypi.as_str(), "pypi");
        assert_eq!(
            DependencyEcosystem::SwiftPackageManager.as_str(),
            "swift_package_manager"
        );
        assert_eq!(DependencyScope::Development.as_str(), "development");
        assert_eq!(
            DependencyEvidenceLevel::ProviderResolved.as_str(),
            "provider_resolved"
        );
    }

    #[test]
    fn persisted_dependency_tokens_round_trip_and_reject_unknown_values() {
        let ecosystems = [
            DependencyEcosystem::Pypi,
            DependencyEcosystem::Npm,
            DependencyEcosystem::Maven,
            DependencyEcosystem::Nuget,
            DependencyEcosystem::Cargo,
            DependencyEcosystem::GoModules,
            DependencyEcosystem::Composer,
            DependencyEcosystem::RubyGems,
            DependencyEcosystem::SwiftPackageManager,
            DependencyEcosystem::Cran,
            DependencyEcosystem::Bioconductor,
            DependencyEcosystem::DelphiPackage,
            DependencyEcosystem::Alire,
            DependencyEcosystem::Fpm,
            DependencyEcosystem::MatlabAddOn,
            DependencyEcosystem::SqlExtension,
            DependencyEcosystem::ScratchExtension,
            DependencyEcosystem::Vcpkg,
            DependencyEcosystem::Conan,
            DependencyEcosystem::NativeSystem,
        ];
        for ecosystem in ecosystems {
            assert_eq!(
                DependencyEcosystem::parse_str(ecosystem.as_str()),
                Ok(ecosystem)
            );
        }

        let scopes = [
            DependencyScope::Runtime,
            DependencyScope::Development,
            DependencyScope::Test,
            DependencyScope::Build,
            DependencyScope::Unknown,
        ];
        for scope in scopes {
            assert_eq!(DependencyScope::parse_str(scope.as_str()), Ok(scope));
        }

        let evidence_levels = [
            DependencyEvidenceLevel::ManifestDeclared,
            DependencyEvidenceLevel::LockfileResolved,
            DependencyEvidenceLevel::ProviderResolved,
        ];
        for evidence_level in evidence_levels {
            assert_eq!(
                DependencyEvidenceLevel::parse_str(evidence_level.as_str()),
                Ok(evidence_level)
            );
        }

        assert!(DependencyEcosystem::parse_str("unknown_ecosystem").is_err());
        assert!(DependencyScope::parse_str("production").is_err());
        assert!(DependencyEvidenceLevel::parse_str("guessed").is_err());
    }

    #[test]
    fn package_and_version_text_is_bounded_at_the_untrusted_boundary() {
        assert!(PackageIdentity::new(DependencyEcosystem::Cargo, "serde").is_ok());
        assert!(PackageIdentity::new(DependencyEcosystem::Cargo, "\n").is_err());
        assert!(DependencyVersion::new("^1.0").is_ok());
        assert!(DependencyVersion::new("x".repeat(MAX_VERSION_TEXT_CHARS + 1)).is_err());
    }

    #[test]
    fn lockfile_evidence_requires_a_resolved_version() {
        let package = PackageIdentity::new(DependencyEcosystem::Cargo, "serde").expect("package");
        assert!(DependencyRecord::new(
            package,
            Some(DependencyVersion::new("^1").expect("requirement")),
            None,
            DependencyScope::Runtime,
            false,
            DependencyDirectness::Direct,
            DependencyEvidenceLevel::LockfileResolved,
            evidence(),
        )
        .is_err());
    }

    #[test]
    fn library_contracts_are_explicit_versioned_and_deterministic() {
        let versions = LibraryContractVersionSet::new([
            DependencyVersion::new("1.0.188").expect("version"),
            DependencyVersion::new("1.0.188").expect("duplicate version"),
        ])
        .expect("versions");
        let contract = LibraryContract::new(
            LibraryContractId::new("rust.serde.model").expect("id"),
            1,
            PackageIdentity::new(DependencyEcosystem::Cargo, "serde").expect("package"),
            versions,
            [
                LibraryCapability::FrameworkRole,
                LibraryCapability::SymbolIdentity,
                LibraryCapability::FrameworkRole,
            ],
        )
        .expect("contract");
        assert_eq!(contract.id.as_str(), "rust.serde.model");
        assert_eq!(contract.versions.versions().len(), 1);
        assert_eq!(
            contract.capabilities,
            vec![
                LibraryCapability::SymbolIdentity,
                LibraryCapability::FrameworkRole
            ]
        );
        assert!(LibraryContractId::new("Rust.Serde").is_err());
        assert!(LibraryContract::new(
            LibraryContractId::new("rust.empty").expect("id"),
            0,
            PackageIdentity::new(DependencyEcosystem::Cargo, "serde").expect("package"),
            LibraryContractVersionSet::new([DependencyVersion::new("1.0.188").expect("version"),])
                .expect("versions"),
            [],
        )
        .is_err());
        assert!(LibraryContractVersionSet::new([]).is_err());
    }

    #[test]
    fn library_contract_registry_requires_resolved_exact_versions() {
        let package = PackageIdentity::new(DependencyEcosystem::Cargo, "serde").expect("package");
        let contract = LibraryContract::new(
            LibraryContractId::new("rust.serde.symbols").expect("id"),
            2,
            package.clone(),
            LibraryContractVersionSet::new([DependencyVersion::new("1.0.188").expect("version")])
                .expect("versions"),
            [LibraryCapability::SymbolIdentity],
        )
        .expect("contract");
        let registry = LibraryContractRegistry::new([contract]).expect("registry");

        let locked = DependencyRecord::new(
            package.clone(),
            Some(DependencyVersion::new("^1").expect("requirement")),
            Some(DependencyVersion::new("1.0.188").expect("version")),
            DependencyScope::Runtime,
            false,
            DependencyDirectness::Unknown,
            DependencyEvidenceLevel::LockfileResolved,
            evidence(),
        )
        .expect("locked dependency");
        assert!(matches!(
            registry.resolve(&locked, LibraryCapability::SymbolIdentity),
            LibraryContractResolution::Matched(found)
                if found.id.as_str() == "rust.serde.symbols"
        ));
        assert_eq!(
            registry.resolve(&locked, LibraryCapability::CallSemantics),
            LibraryContractResolution::NoContract
        );

        let manifest_only = DependencyRecord::new(
            package,
            Some(DependencyVersion::new("1.0.188").expect("requirement")),
            None,
            DependencyScope::Runtime,
            false,
            DependencyDirectness::Direct,
            DependencyEvidenceLevel::ManifestDeclared,
            evidence(),
        )
        .expect("manifest dependency");
        assert_eq!(
            registry.resolve(&manifest_only, LibraryCapability::SymbolIdentity),
            LibraryContractResolution::InsufficientDependencyEvidence
        );
    }

    #[test]
    fn library_contract_registry_rejects_ambiguous_claims() {
        let package = PackageIdentity::new(DependencyEcosystem::Cargo, "serde").expect("package");
        let contract = |id: &str| {
            LibraryContract::new(
                LibraryContractId::new(id).expect("id"),
                1,
                package.clone(),
                LibraryContractVersionSet::new([
                    DependencyVersion::new("1.0.188").expect("version")
                ])
                .expect("versions"),
                [LibraryCapability::FrameworkRole],
            )
            .expect("contract")
        };
        assert!(LibraryContractRegistry::new([
            contract("rust.serde.role-a"),
            contract("rust.serde.role-b"),
        ])
        .is_err());
    }

    #[test]
    fn dependency_snapshots_sort_and_reject_duplicate_inventory_records() {
        let record = DependencyRecord::new(
            PackageIdentity::new(DependencyEcosystem::Cargo, "serde").expect("package"),
            Some(DependencyVersion::new("^1").expect("requirement")),
            None,
            DependencyScope::Runtime,
            false,
            DependencyDirectness::Direct,
            DependencyEvidenceLevel::ManifestDeclared,
            evidence(),
        )
        .expect("record");
        let unknown = TypedUnknown::new(
            crate::core::model::UnknownClass::Recoverable,
            crate::core::model::UnknownReasonCode::MissingDependency,
            "dependency:serde",
            Some("configure a provider".to_string()),
        )
        .expect("unknown");
        let snapshot = DependencySnapshot::new([record.clone()], [unknown.clone(), unknown])
            .expect("snapshot");
        assert_eq!(snapshot.dependencies[0].package.name, "serde");
        assert_eq!(snapshot.unknowns.len(), 1);
        assert!(DependencySnapshot::new([record.clone(), record], []).is_err());
    }

    #[test]
    fn dependency_directness_is_a_closed_three_state_contract() {
        for directness in [
            DependencyDirectness::Direct,
            DependencyDirectness::Transitive,
            DependencyDirectness::Unknown,
        ] {
            assert_eq!(
                DependencyDirectness::parse_str(directness.as_str()),
                Ok(directness)
            );
        }
        assert!(DependencyDirectness::parse_str("indirect").is_err());
    }

    #[test]
    fn external_symbol_identity_is_package_qualified_and_bounded() {
        let package = PackageIdentity::new(DependencyEcosystem::Cargo, "serde").expect("package");
        let symbol = ExternalSymbolId::new(package, "serde::Serialize").expect("symbol");
        assert_eq!(symbol.qualified_name, "serde::Serialize");
        assert!(ExternalSymbolId::new(
            PackageIdentity::new(DependencyEcosystem::Cargo, "serde").expect("package"),
            "bad\nsymbol",
        )
        .is_err());
    }
}
