use super::*;
static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[test]
fn native_review_rejects_stale_revision_active_action_and_source_replacement() {
    let ctx = egui::Context::default();
    let mut app = DperMeasureApp::new_with_services(ctx, false);
    let path = std::env::temp_dir().join(format!("native-review-{}-{}.pdf", std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup { fn drop(&mut self) { let _ = std::fs::remove_file(&self.0); } }
    let _cleanup = Cleanup(path.clone());
    std::fs::write(&path, b"review source bytes, parser not used").unwrap();
    let mut plan = crate::dper_drawing::parse_text_plan("Diametre 4 mm", "review.pdf");
    plan.source_file = Some(path.display().to_string());
    plan.source_fingerprint = Some(crate::dper_source::fingerprint_file(&path).unwrap());
    app.install_inspection_plan(plan);
    let id = app.inspection.plan.as_ref().unwrap().characteristics[0].id.clone();
    let revision = app.document_revision;
    app.review_native_feature(&id, true, revision).unwrap();
    assert!(app.inspection.plan.as_ref().unwrap().characteristics[0].confirmed);
    assert!(app.review_native_feature(&id, false, revision).is_err());
    let revision = app.document_revision;
    app.pending.push(crate::geometry::Point { x: 1.0, y: 1.0 });
    assert!(app.review_native_feature(&id, false, revision).is_err());
    app.pending.clear();
    assert!(app.review_native_binding("absent", true, revision).is_err());
    let key = app.inspection_presentation_key();
    let metadata = std::fs::metadata(&path).unwrap();
    std::fs::write(&path, vec![b'x'; metadata.len() as usize]).unwrap();
    std::fs::File::options().write(true).open(&path).unwrap().set_times(
        std::fs::FileTimes::new().set_modified(metadata.modified().unwrap())).unwrap();
    assert_ne!(app.inspection_presentation_key(), key, "Visible evidence key must detect same-size/date PDF replacement");
    assert!(app.review_native_feature(&id, false, revision).is_err());
    assert_eq!(app.document_revision, revision);
    assert!(app.inspection.plan.as_ref().unwrap().characteristics[0].confirmed);
}
