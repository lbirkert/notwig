use notwig_project::BundleFiles;

const PROJECT_MANIFEST: &str = "project.json";

/// One step in a staged browser bundle write.
///
/// Staging steps must all run before any commit step. Within both phases the
/// manifest is written last so an existing `project.json` keeps referring to
/// the previous document until every other file is in place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BundleWriteStep {
    Staging(String),
    Commit(String),
}

/// Package-relative paths in write order, with `project.json` last.
#[must_use]
pub fn write_order(files: &BundleFiles) -> Vec<String> {
    let mut paths: Vec<String> = files.files.keys().cloned().collect();
    paths.sort_by(|left, right| {
        (left == PROJECT_MANIFEST)
            .cmp(&(right == PROJECT_MANIFEST))
            .then_with(|| left.cmp(right))
    });
    paths
}

/// Staged write plan for Save As: write the complete bundle to staging first,
/// then commit it to the target, still writing `project.json` last.
#[must_use]
pub fn save_as_steps(files: &BundleFiles) -> Vec<BundleWriteStep> {
    let order = write_order(files);
    let mut steps = Vec::with_capacity(order.len() * 2);
    steps.extend(order.iter().cloned().map(BundleWriteStep::Staging));
    steps.extend(order.iter().cloned().map(BundleWriteStep::Commit));
    steps
}

#[must_use]
pub fn bytes<'a>(files: &'a BundleFiles, path: &str) -> &'a [u8] {
    files.get(path).unwrap_or_default()
}
