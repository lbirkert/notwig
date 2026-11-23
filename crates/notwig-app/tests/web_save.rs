use std::collections::BTreeMap;

use notwig_app::bundle_write::{BundleWriteStep, save_as_steps, write_order};
use notwig_project::BundleFiles;

fn sample_files() -> BundleFiles {
    let mut files = BundleFiles::default();
    files.insert("project.json", b"new manifest".to_vec());
    files.insert("assets/a.wav", vec![1, 2, 3]);
    files.insert("state/device.json", vec![4, 5, 6]);
    files
}

#[derive(Default)]
struct FakeStorage {
    target: BTreeMap<String, Vec<u8>>,
    staging: BTreeMap<String, Vec<u8>>,
    fail: Option<BundleWriteStep>,
}

impl FakeStorage {
    fn apply(&mut self, files: &BundleFiles) -> Result<(), String> {
        for step in save_as_steps(files) {
            if self.fail.as_ref() == Some(&step) {
                return Err("injected storage failure".into());
            }
            let (destination, path) = match &step {
                BundleWriteStep::Staging(path) => (&mut self.staging, path),
                BundleWriteStep::Commit(path) => (&mut self.target, path),
            };
            destination.insert(path.clone(), files.get(path).unwrap_or_default().to_vec());
        }
        Ok(())
    }
}

#[test]
fn write_order_puts_manifest_last() {
    let order = write_order(&sample_files());
    assert_eq!(order.len(), 3);
    assert_eq!(order.last().map(String::as_str), Some("project.json"));
}

#[test]
fn save_as_stages_the_complete_bundle_before_committing() {
    let steps = save_as_steps(&sample_files());
    let first_commit = steps
        .iter()
        .position(|step| matches!(step, BundleWriteStep::Commit(_)))
        .unwrap();
    assert!(
        steps[..first_commit]
            .iter()
            .all(|step| matches!(step, BundleWriteStep::Staging(_)))
    );
    let staged: Vec<&str> = steps
        .iter()
        .filter_map(|step| match step {
            BundleWriteStep::Staging(path) => Some(path.as_str()),
            BundleWriteStep::Commit(_) => None,
        })
        .collect();
    assert_eq!(staged.last().copied(), Some("project.json"));
    assert!(matches!(
        steps.last(),
        Some(BundleWriteStep::Commit(path)) if path == "project.json"
    ));
}

#[test]
fn failure_during_staging_leaves_existing_project_untouched() {
    let files = sample_files();
    let mut storage = FakeStorage::default();
    storage
        .target
        .insert("project.json".into(), b"old manifest".to_vec());
    storage.fail = Some(BundleWriteStep::Staging("state/device.json".into()));

    assert!(storage.apply(&files).is_err());
    assert_eq!(
        storage.target.get("project.json").map(Vec::as_slice),
        Some(b"old manifest".as_slice())
    );
    assert_eq!(storage.target.len(), 1);
}

#[test]
fn failed_commit_keeps_old_manifest_until_the_last_write() {
    let files = sample_files();
    let mut storage = FakeStorage::default();
    storage
        .target
        .insert("project.json".into(), b"old manifest".to_vec());
    storage.fail = Some(BundleWriteStep::Commit("assets/a.wav".into()));

    assert!(storage.apply(&files).is_err());
    assert_eq!(
        storage.target.get("project.json").map(Vec::as_slice),
        Some(b"old manifest".as_slice())
    );
}
