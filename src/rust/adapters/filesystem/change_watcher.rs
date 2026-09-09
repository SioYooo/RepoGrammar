//! Native change hints; neither delivery nor silence proves index freshness.

use super::discovery::{
    is_default_excluded_directory_name, is_repogrammar_state_directory_name,
    supported_language_for_path,
};
use super::git::GitContext;
use notify::event::{CreateKind, ModifyKind, RemoveKind};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeWatcherError {
    Unavailable,
    ReconciliationRequired,
}

impl std::fmt::Display for ChangeWatcherError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "native change watcher unavailable",
            Self::ReconciliationRequired => {
                "native change watcher lost events; reconciliation required"
            }
        })
    }
}

impl std::error::Error for ChangeWatcherError {}

#[derive(Default)]
struct IgnoreFilter {
    generation: u64,
    directories: BTreeSet<String>,
}

pub struct RepositoryChangeWatcher {
    root: PathBuf,
    ignored: Arc<Mutex<IgnoreFilter>>,
    _watcher: RecommendedWatcher,
    pending: Receiver<()>,
    failed: Arc<AtomicBool>,
}

impl RepositoryChangeWatcher {
    pub fn new(root: &Path) -> Result<Self, ChangeWatcherError> {
        let root = root
            .canonicalize()
            .map_err(|_| ChangeWatcherError::Unavailable)?;
        let (sender, pending) = mpsc::sync_channel(1);
        let failed = Arc::new(AtomicBool::new(false));
        let callback_failed = Arc::clone(&failed);
        let callback_root = root.clone();
        let ignored = Arc::new(Mutex::new(IgnoreFilter::default()));
        let callback_ignored = Arc::clone(&ignored);
        let mut watcher = RecommendedWatcher::new(
            move |event| {
                signal_filtered_change(
                    &callback_root,
                    event,
                    &sender,
                    &callback_failed,
                    &callback_ignored,
                );
            },
            notify::Config::default().with_follow_symlinks(false),
        )
        .map_err(|_| ChangeWatcherError::Unavailable)?;
        watcher
            .watch(&root, RecursiveMode::Recursive)
            .map_err(|_| ChangeWatcherError::Unavailable)?;
        let result = Self {
            root,
            ignored,
            _watcher: watcher,
            pending,
            failed,
        };
        result.refresh_ignore_filter();
        Ok(result)
    }

    /// Refresh advisory exclusions before periodic reconciliation. Failure leaves
    /// an empty filter; an event racing the query prevents publishing its result.
    pub fn refresh_ignore_filter(&self) {
        let generation = match self.ignored.lock() {
            Ok(mut filter) => {
                filter.directories.clear();
                filter.generation = filter.generation.wrapping_add(1);
                filter.generation
            }
            Err(_) => return,
        };
        let directories = GitContext::resolve(&self.root)
            .ok()
            .and_then(|git| git.ignored_untracked_directories().ok())
            .unwrap_or_default();
        if let Ok(mut filter) = self.ignored.lock() {
            if filter.generation == generation {
                filter.directories = directories;
            }
        }
    }

    /// Consume at most one coalesced hint. Errors require reconciliation and
    /// abandoning this watcher; callers retain periodic fingerprint fallback.
    pub fn wait(&self, timeout: Duration) -> Result<bool, ChangeWatcherError> {
        if self.failed.load(Ordering::Acquire) {
            return Err(ChangeWatcherError::ReconciliationRequired);
        }
        let result = match self.pending.recv_timeout(timeout) {
            Ok(()) => Ok(true),
            Err(RecvTimeoutError::Timeout) => Ok(false),
            Err(RecvTimeoutError::Disconnected) => Err(ChangeWatcherError::Unavailable),
        };
        if self.failed.load(Ordering::Acquire) {
            return Err(ChangeWatcherError::ReconciliationRequired);
        }
        result
    }
}

fn signal_filtered_change(
    root: &Path,
    event: notify::Result<Event>,
    sender: &SyncSender<()>,
    failed: &AtomicBool,
    ignored: &Mutex<IgnoreFilter>,
) {
    if let Ok(event) = &event {
        if !event.need_rescan() && !matches!(event.kind, EventKind::Access(_)) {
            if !event.paths.is_empty()
                && event.paths.iter().all(|path| {
                    path.strip_prefix(root).ok().is_some_and(|relative| {
                        relative.components().any(|component| {
                            is_repogrammar_state_directory_name(component.as_os_str().to_str())
                        })
                    })
                })
            {
                return;
            }

            if event.paths.is_empty()
                || event
                    .paths
                    .iter()
                    .any(|path| ignore_policy_path(root, path))
            {
                if let Ok(mut filter) = ignored.lock() {
                    filter.directories.clear();
                    filter.generation = filter.generation.wrapping_add(1);
                }
                let _ = sender.try_send(());
                return;
            }
            if let Ok(filter) = ignored.lock() {
                if !event.paths.is_empty()
                    && event.paths.iter().all(|path| {
                        path.strip_prefix(root).ok().is_some_and(|relative| {
                            relative
                                .ancestors()
                                .filter_map(Path::to_str)
                                .any(|ancestor| filter.directories.contains(ancestor))
                        })
                    })
                {
                    return;
                }
            }
        }
    }
    signal_change(root, event, sender, failed);
}

fn ignore_policy_path(root: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return true;
    };
    relative
        .components()
        .any(|component| component.as_os_str() == ".git")
        || matches!(
            relative.file_name().and_then(|name| name.to_str()),
            Some(".gitignore" | ".gitattributes")
        )
}

fn signal_change(
    root: &Path,
    event: notify::Result<Event>,
    sender: &SyncSender<()>,
    failed: &AtomicBool,
) {
    match event {
        Ok(event) if event.need_rescan() => failed.store(true, Ordering::Release),
        Err(_) => failed.store(true, Ordering::Release),
        Ok(event) if !relevant_event(root, &event) => return,
        Ok(_) => {}
    }
    // One queued token already requests a whole-repository reconciliation.
    // Full is intentional coalescing; no paths or event payloads are retained.
    let _ = sender.try_send(());
}

fn relevant_event(root: &Path, event: &Event) -> bool {
    if matches!(event.kind, EventKind::Access(_)) {
        return false;
    }
    event.paths.is_empty()
        || event.paths.iter().any(|path| {
            let Ok(relative) = path.strip_prefix(root) else {
                return true; // Unknown roots cannot establish irrelevance.
            };
            let excluded = relative.components().any(|component| match component {
                Component::Normal(name) => {
                    is_repogrammar_state_directory_name(name.to_str())
                        || is_default_excluded_directory_name(name.to_str())
                }
                _ => false,
            });
            if excluded {
                return false;
            }
            if matches!(
                event.kind,
                EventKind::Create(CreateKind::File)
                    | EventKind::Remove(RemoveKind::File)
                    | EventKind::Modify(ModifyKind::Data(_))
            ) {
                return relative
                    .to_str()
                    .is_none_or(|path| supported_language_for_path(path).is_some())
                    || ignore_policy_path(root, path);
            }
            true
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempWorkspace;
    use notify::event::{AccessKind, Flag, RenameMode};

    #[test]
    fn filters_state_build_and_read_noise_but_keeps_renames_and_unknowns() {
        let root = Path::new("/repo");
        for path in [
            ".repogrammar/logs/daemon.log",
            ".repogrammar-custom/config.json",
            "src/node_modules/pkg/a.py",
            "target/a.rs",
        ] {
            let event = Event::new(EventKind::Create(CreateKind::Any)).add_path(root.join(path));
            assert!(!relevant_event(root, &event), "{path}");
        }
        for path in [
            "src/new_folder",
            "src/app.py",
            ".gitignore",
            ".build/kept.ts",
        ] {
            let event = Event::new(EventKind::Create(CreateKind::Any)).add_path(root.join(path));
            assert!(relevant_event(root, &event), "{path}");
        }
        let rename = Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Both)))
            .add_path(root.join("target/temporary.py"))
            .add_path(root.join("src/new.py"));
        assert!(relevant_event(root, &rename));
        assert!(relevant_event(root, &Event::new(EventKind::Any)));
        assert!(!relevant_event(
            root,
            &Event::new(EventKind::Access(AccessKind::Read)).add_path(root.join("app.py"))
        ));
    }

    #[test]
    fn ignored_directory_filter_invalidates_on_policy_and_index_changes() {
        let root = Path::new("/repo");
        let ignored = Mutex::new(IgnoreFilter {
            generation: 0,
            directories: BTreeSet::from(["scratch".to_string()]),
        });
        let (sender, receiver) = mpsc::sync_channel(1);
        let failed = AtomicBool::new(false);
        let source =
            Event::new(EventKind::Create(CreateKind::File)).add_path(root.join("scratch/app.py"));
        signal_filtered_change(root, Ok(source.clone()), &sender, &failed, &ignored);
        assert!(receiver.try_recv().is_err());
        let sibling = Event::new(EventKind::Create(CreateKind::File))
            .add_path(root.join("scratch-other/app.py"));
        signal_filtered_change(root, Ok(sibling), &sender, &failed, &ignored);
        receiver.try_recv().expect("sibling is not ignored");
        for policy in [".git/index", ".git/info/exclude", ".gitignore"] {
            ignored.lock().unwrap().directories.insert("scratch".into());
            signal_filtered_change(
                root,
                Ok(Event::new(EventKind::Any).add_path(root.join(policy))),
                &sender,
                &failed,
                &ignored,
            );
            receiver.try_recv().expect("policy wakeup");
            assert!(ignored.lock().unwrap().directories.is_empty());
            signal_filtered_change(root, Ok(source.clone()), &sender, &failed, &ignored);
            receiver.try_recv().expect("source no longer suppressed");
        }
        let state = Event::new(EventKind::Any).add_path(root.join(".repogrammar/.gitignore"));
        signal_filtered_change(root, Ok(state), &sender, &failed, &ignored);
        assert!(receiver.try_recv().is_err());
        let unsupported = Event::new(EventKind::Modify(ModifyKind::Data(
            notify::event::DataChange::Any,
        )))
        .add_path(root.join(".codegraph/graph.db"));
        assert!(!relevant_event(root, &unsupported));
        assert!(relevant_event(
            root,
            &Event::new(EventKind::Remove(RemoveKind::Folder)).add_path(root.join("old.folder"))
        ));
    }

    #[test]
    fn bounded_coalescing_preserves_failure_and_overflow_signals() {
        let (sender, receiver) = mpsc::sync_channel(1);
        let failed = AtomicBool::new(false);
        for _ in 0..10_000 {
            signal_change(
                Path::new("/repo"),
                Ok(Event::new(EventKind::Any)),
                &sender,
                &failed,
            );
        }
        assert!(!failed.load(Ordering::Acquire));
        receiver.try_recv().expect("one coalesced event");
        assert!(receiver.try_recv().is_err());
        signal_change(
            Path::new("/repo"),
            Ok(Event::new(EventKind::Any)),
            &sender,
            &failed,
        );
        signal_change(
            Path::new("/repo"),
            Ok(Event::new(EventKind::Any).set_flag(Flag::Rescan)),
            &sender,
            &failed,
        );
        assert!(failed.load(Ordering::Acquire));
        failed.store(false, Ordering::Release);
        signal_change(
            Path::new("/repo"),
            Err(notify::Error::generic("test")),
            &sender,
            &failed,
        );
        assert!(failed.load(Ordering::Acquire));
    }

    #[test]
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn native_watcher_observes_a_real_source_creation() {
        let workspace = TempWorkspace::new("native-change-watcher");
        let watcher = RepositoryChangeWatcher::new(workspace.path()).expect("native watcher");
        std::fs::write(workspace.path().join("app.py"), "x = 1\n").expect("write source");
        assert!(watcher
            .wait(Duration::from_secs(10))
            .expect("source change hint"));
    }
}
