use std::path::{Path, PathBuf};

use notwig_app::workspace::Workspace;
use notwig_app::workspace::project_registry;
use notwig_core::ids::{DeviceChain, Track};
use notwig_core::{Id, Meta, ProjectDoc, SampleRate};
use uuid::Uuid;

fn doc_named(name: &str) -> ProjectDoc {
    let meta = Meta {
        name: name.into(),
        created_utc: 0,
        modified_utc: 0,
        sample_rate: SampleRate::DEFAULT,
        tempo_bpm: 120.0,
        time_sig_num: 4,
        time_sig_den: 4,
    };
    ProjectDoc::new_template(
        meta,
        Id::<Track>::from_uuid(Uuid::new_v4()),
        Id::<DeviceChain>::from_uuid(Uuid::new_v4()),
    )
    .unwrap()
}

fn temp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("notwig-recovery-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn prepare_recovery(bundle: &Path, autosave_name: &str) {
    notwig_project::save(&doc_named("Saved"), bundle).unwrap();
    notwig_project::autosave(&doc_named(autosave_name), bundle, 1).unwrap();
    notwig_project::create_session_lock(bundle).unwrap();
}

#[test]
fn recovery_offered_only_for_lock_and_autosave() {
    let dir = temp_dir();
    let bundle = dir.join("Song.notwig");
    notwig_project::save(&doc_named("Saved"), &bundle).unwrap();
    notwig_project::autosave(&doc_named("Recovered"), &bundle, 1).unwrap();

    let without_lock = Workspace::open(&bundle).unwrap();
    assert!(!without_lock.recovery_offered);

    notwig_project::create_session_lock(&bundle).unwrap();
    let with_lock = Workspace::open(&bundle).unwrap();
    assert!(with_lock.recovery_offered);
    assert!(notwig_project::has_session_lock(&bundle));

    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn recover_latest_autosave_replaces_document_and_keeps_bundle() {
    let dir = temp_dir();
    let bundle = dir.join("Song.notwig");
    prepare_recovery(&bundle, "Recovered");

    let mut workspace = Workspace::open(&bundle).unwrap();
    assert_eq!(workspace.doc.meta.name, "Saved");
    assert!(workspace.recovery_offered);

    workspace.recover_latest_autosave().unwrap();
    assert_eq!(workspace.doc.meta.name, "Recovered");
    assert_eq!(workspace.bundle.as_deref(), Some(bundle.as_path()));
    assert!(workspace.dirty);
    assert!(!workspace.recovery_offered);
    assert!(notwig_project::has_session_lock(&bundle));

    workspace.undo().unwrap();
    assert_eq!(workspace.doc.meta.name, "Recovered");

    workspace.end_session();
    assert!(!notwig_project::has_session_lock(&bundle));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn discard_removes_autosaves_and_stale_lock() {
    let dir = temp_dir();
    let bundle = dir.join("Song.notwig");
    prepare_recovery(&bundle, "Recovered");

    let mut workspace = Workspace::open(&bundle).unwrap();
    assert!(workspace.recovery_offered);

    workspace.discard_recovery().unwrap();
    assert_eq!(workspace.doc.meta.name, "Saved");
    assert!(!workspace.recovery_offered);
    assert_eq!(notwig_project::list_autosaves(&bundle).len(), 0);
    assert!(!notwig_project::has_session_lock(&bundle));

    workspace.begin_session();
    assert!(notwig_project::has_session_lock(&bundle));
    workspace.end_session();
    assert!(!notwig_project::has_session_lock(&bundle));

    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn corrupt_autosave_keeps_recovery_available_for_discard() {
    let dir = temp_dir();
    let bundle = dir.join("Song.notwig");
    prepare_recovery(&bundle, "Recovered");
    std::fs::write(bundle.join("autosave/snapshot-0-9.json"), b"not json").unwrap();

    let mut workspace = Workspace::open(&bundle).unwrap();
    assert!(workspace.recovery_offered);

    assert!(workspace.recover_latest_autosave().is_err());
    assert_eq!(workspace.doc.meta.name, "Saved");
    assert!(workspace.recovery_offered);

    workspace.discard_recovery().unwrap();
    assert!(!workspace.recovery_offered);
    assert_eq!(notwig_project::list_autosaves(&bundle).len(), 0);

    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn registry_dedupes_and_filters_missing_bundles() {
    let dir = temp_dir();
    let registry = dir.join("projects.json");
    let first = dir.join("First.notwig");
    let second = dir.join("Second.notwig");
    notwig_project::save(&doc_named("First"), &first).unwrap();
    notwig_project::save(&doc_named("Second"), &second).unwrap();
    let first_normalized = std::fs::canonicalize(&first).unwrap();

    project_registry::register_bundle_at(&registry, &first).unwrap();
    project_registry::register_bundle_at(&registry, &second).unwrap();
    project_registry::register_bundle_at(&registry, &first).unwrap();

    let bundles = project_registry::registered_bundles_at(&registry);
    assert_eq!(bundles.len(), 2);
    assert!(bundles.contains(&first_normalized));

    std::fs::remove_dir_all(&second).unwrap();
    let bundles = project_registry::registered_bundles_at(&registry);
    assert_eq!(bundles, vec![first_normalized]);

    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn recovery_candidates_require_lock_and_autosave() {
    let dir = temp_dir();
    let stale = dir.join("Stale.notwig");
    prepare_recovery(&stale, "Stale autosave");

    let without_lock = dir.join("NoLock.notwig");
    notwig_project::save(&doc_named("NoLock"), &without_lock).unwrap();
    notwig_project::autosave(&doc_named("NoLock autosave"), &without_lock, 1).unwrap();

    let without_autosave = dir.join("NoAutosave.notwig");
    notwig_project::save(&doc_named("NoAutosave"), &without_autosave).unwrap();
    notwig_project::create_session_lock(&without_autosave).unwrap();

    let missing = dir.join("Missing.notwig");
    let candidates = project_registry::recovery_candidates_in(&[
        stale.clone(),
        without_lock,
        without_autosave,
        missing,
    ]);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].bundle, std::fs::canonicalize(&stale).unwrap());
    assert_eq!(candidates[0].project_name, "Stale");
    assert_eq!(candidates[0].autosave_count, 1);

    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn startup_candidate_recovery_loads_the_latest_autosave() {
    let dir = temp_dir();
    let bundle = dir.join("Song.notwig");
    prepare_recovery(&bundle, "Recovered");

    let mut workspace = Workspace::new_empty().unwrap();
    workspace.recover_candidate(&bundle).unwrap();

    assert_eq!(workspace.doc.meta.name, "Recovered");
    assert_eq!(workspace.bundle.as_deref(), Some(bundle.as_path()));
    assert!(workspace.dirty);
    assert!(!workspace.recovery_offered);
    assert!(notwig_project::has_session_lock(&bundle));

    workspace.end_session();
    assert!(!notwig_project::has_session_lock(&bundle));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn startup_candidate_discard_clears_lock_and_autosaves() {
    let dir = temp_dir();
    let bundle = dir.join("Song.notwig");
    prepare_recovery(&bundle, "Recovered");

    let mut workspace = Workspace::new_empty().unwrap();
    workspace.discard_candidate(&bundle).unwrap();

    assert_eq!(workspace.doc.meta.name, "Saved");
    assert_eq!(workspace.bundle.as_deref(), Some(bundle.as_path()));
    assert!(!workspace.recovery_offered);
    assert_eq!(notwig_project::list_autosaves(&bundle).len(), 0);
    assert!(!notwig_project::has_session_lock(&bundle));

    workspace.begin_session();
    assert!(notwig_project::has_session_lock(&bundle));
    workspace.end_session();
    std::fs::remove_dir_all(dir).unwrap();
}
