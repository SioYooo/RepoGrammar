//! Opt-in, session-local diagnostic wall time. No source or global state.
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostPhase {
    Discovery,
    Hashing,
    Context,
    Analyzer,
    Normalization,
    Family,
    Persistence,
    Validation,
    Activation,
    Other,
}

impl HostPhase {
    pub fn work_scope(self) -> Option<&'static str> {
        Some(match self {
            Self::Discovery => "visited_directory_entries",
            Self::Hashing => "sha256_invocations",
            Self::Context => "bounded_source_reads",
            Self::Analyzer => "parser_provider_session_operations",
            Self::Normalization => "normalization_record_visits",
            Self::Family => "constructed_claims",
            Self::Persistence => "write_session_logical_rows",
            Self::Validation => "generation_validation_calls",
            Self::Activation => "activation_calls",
            Self::Other => return None,
        })
    }
    pub const ALL: [Self; 10] = [
        Self::Discovery,
        Self::Hashing,
        Self::Context,
        Self::Analyzer,
        Self::Normalization,
        Self::Family,
        Self::Persistence,
        Self::Validation,
        Self::Activation,
        Self::Other,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Discovery => "discovery",
            Self::Hashing => "hashing_fingerprinting",
            Self::Context => "frontend_context_preparation",
            Self::Analyzer => "analyzer_execution",
            Self::Normalization => "fact_normalization",
            Self::Family => "family_construction",
            Self::Persistence => "sqlite_persistence",
            Self::Validation => "integrity_foreign_key_validation",
            Self::Activation => "generation_activation",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PhaseMeasurement {
    pub wall: Duration,
    pub calls: u64,
    pub work_items: u64,
    pub bytes: u64,
}

#[derive(Debug)]
struct State {
    python_sessions: u64,
    root_wall: Duration,
    owner: std::thread::ThreadId,
    write_stats: Option<[u64; 3]>,
    rows: [PhaseMeasurement; 10],
    stack: Vec<(HostPhase, Duration)>,
}

#[derive(Debug, Clone)]
pub struct HostProfile(Option<Arc<Mutex<State>>>);
impl Default for HostProfile {
    fn default() -> Self {
        Self(Some(Arc::new(Mutex::new(State {
            python_sessions: 0,
            root_wall: Duration::ZERO,
            owner: std::thread::current().id(),
            write_stats: None,
            rows: [PhaseMeasurement::default(); 10],
            stack: Vec::with_capacity(16),
        }))))
    }
}

impl HostProfile {
    pub fn note_python_session(&self) {
        if let Some(p) = &self.0 {
            p.lock().expect("host profile lock").python_sessions += 1;
        }
    }
    pub fn python_sessions(&self) -> Option<u64> {
        self.0
            .as_ref()
            .map(|p| p.lock().expect("host profile lock").python_sessions)
    }
    pub fn disabled() -> Self {
        Self(None)
    }
    pub fn write_stats(&self, rows: u64, transactions: u64, checkpoints: u64) {
        let Some(inner) = &self.0 else {
            return;
        };
        let mut state = inner.lock().expect("host profile lock");
        state.write_stats = Some([rows, transactions, checkpoints]);
        state.rows[HostPhase::Persistence as usize].work_items += rows;
    }
    pub fn snapshot_write_stats(&self) -> Option<[u64; 3]> {
        self.0
            .as_ref()?
            .lock()
            .expect("host profile lock")
            .write_stats
    }
    pub fn root_wall(&self) -> Duration {
        self.0
            .as_ref()
            .map(|p| p.lock().expect("host profile lock").root_wall)
            .unwrap_or_default()
    }
    pub fn span(&self, phase: HostPhase) -> Option<HostSpan> {
        let inner = self.0.as_ref()?;
        {
            let mut state = inner.lock().expect("host profile lock");
            assert_eq!(
                state.owner,
                std::thread::current().id(),
                "host profile is single-thread only"
            );
            assert!(state.stack.len() < 16, "host profile depth bound");
            state.stack.push((phase, Duration::ZERO));
        }
        Some(HostSpan {
            state: Arc::clone(inner),
            phase,
            start: Instant::now(),
        })
    }
    pub fn work(&self, phase: HostPhase, items: u64, bytes: u64) {
        let Some(inner) = &self.0 else {
            return;
        };
        let mut state = inner.lock().expect("host profile lock");
        let row = &mut state.rows[phase as usize];
        row.work_items += items;
        row.bytes += bytes;
    }
    pub fn snapshot(&self) -> [PhaseMeasurement; 10] {
        let Some(inner) = &self.0 else {
            return [PhaseMeasurement::default(); 10];
        };
        let state = inner.lock().expect("host profile lock");
        assert!(state.stack.is_empty(), "unfinished host profile span");
        state.rows
    }
}

pub struct HostSpan {
    state: Arc<Mutex<State>>,
    phase: HostPhase,
    start: Instant,
}
impl Drop for HostSpan {
    fn drop(&mut self) {
        let elapsed = self.start.elapsed();
        let mut state = self.state.lock().expect("host profile lock");
        let (phase, child) = state.stack.pop().expect("host profile span");
        assert_eq!(phase, self.phase, "host profile spans must close in order");
        let row = &mut state.rows[phase as usize];
        row.wall += elapsed
            .checked_sub(child)
            .expect("host profile child bound");
        row.calls += 1;
        if let Some((_, parent_child)) = state.stack.last_mut() {
            *parent_child += elapsed;
        } else {
            state.root_wall += elapsed;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_spans_partition_wall_and_preserve_actual_counters() {
        let profile = HostProfile::default();
        let total = Instant::now();
        {
            let _outer = profile.span(HostPhase::Other);
            {
                let _inner = profile.span(HostPhase::Hashing);
                profile.work(HostPhase::Hashing, 2, 17);
            }
        }
        let rows = profile.snapshot();
        assert_eq!(rows[HostPhase::Hashing as usize].calls, 1);
        assert_eq!(rows[HostPhase::Hashing as usize].work_items, 2);
        assert_eq!(rows[HostPhase::Hashing as usize].bytes, 17);
        assert!(rows.iter().map(|r| r.wall).sum::<Duration>() <= total.elapsed());
        assert_eq!(
            rows.iter().map(|r| r.wall).sum::<Duration>(),
            profile.root_wall()
        );
    }

    #[test]
    fn disabled_profile_and_error_unwind_do_not_create_fake_work() {
        let disabled = HostProfile::disabled();
        assert!(disabled.span(HostPhase::Analyzer).is_none());
        disabled.work(HostPhase::Analyzer, 100, 200);
        assert!(disabled.snapshot_write_stats().is_none());
        let profile = HostProfile::default();
        let failure = || -> Result<(), ()> {
            let _span = profile.span(HostPhase::Analyzer);
            Err(())
        };
        assert!(failure().is_err());
        assert_eq!(profile.snapshot()[HostPhase::Analyzer as usize].calls, 1);
    }

    #[test]
    fn cross_thread_span_is_rejected_before_stack_mutation() {
        let profile = HostProfile::default();
        assert!(std::thread::spawn(move || {
            let _span = profile.span(HostPhase::Other);
        })
        .join()
        .is_err());
    }
}
