//! Native inspection of PDF vector proposals. Installation and listing use the
//! same application requests as MCP; extraction never owns application state.
use crate::{
    dper_pdf_geometry::{DrawingGeometryCatalog, VectorPrimitive},
    mcp_server::{DrawingRequest, GeometryPageArgs, ListGeometryArgs},
    DperMeasureApp,
};
use eframe::egui;
use serde::Deserialize;
use std::{collections::HashMap, path::Path, sync::mpsc, time::Duration};

#[derive(Default)]
pub(crate) struct DrawingGeometryUi {
    page: u32,
    kind: usize,
    offset: usize,
    source_identity: String,
    status: String,
    job: Option<GeometryJob>,
    presentation_key: Option<PresentationKey>,
    listing: Option<Result<serde_json::Value, String>>,
    details: HashMap<String, Result<VectorPrimitive, String>>,
}

#[derive(PartialEq, Eq)]
struct PresentationKey {
    revision: u64,
    source_metadata: String,
    generation: u64,
    page: u32,
    kind: usize,
    offset: usize,
}

struct GeometryJob {
    expected_revision: u64,
    page: u32,
    receiver: mpsc::Receiver<Result<DrawingGeometryCatalog, String>>,
}

#[derive(Deserialize)]
struct GeometrySource {
    path: String,
    document_fingerprint: String,
    page: u32,
}

const PAGE_SIZE: usize = 20;
const KINDS: [(&str, Option<&str>); 5] = [
    ("Toutes", None),
    ("Lignes", Some("line")),
    ("Polylignes", Some("polyline")),
    ("Polygones fermés", Some("closed_polygon")),
    ("Cercles proposés", Some("circle_candidate")),
];

/// Used by both UI and MCP after off-thread extraction. The application checks
/// the live source again when installing, as well as the expected revision.
pub(crate) fn extract_current_catalog(
    path: &Path,
    expected_fingerprint: &str,
    page: u32,
) -> Result<DrawingGeometryCatalog, String> {
    let catalog = crate::dper_pdf_geometry::analyze_pdf_geometry(path, page)?;
    validate_extracted_catalog(catalog, expected_fingerprint, page)
}

fn validate_extracted_catalog(
    catalog: DrawingGeometryCatalog,
    expected_fingerprint: &str,
    page: u32,
) -> Result<DrawingGeometryCatalog, String> {
    if catalog.document_fingerprint != expected_fingerprint || catalog.geometry.page != page {
        return Err("Le fichier PDF ou sa page a changé depuis l'analyse du plan. Réanalysez la source avant d'installer le catalogue.".into());
    }
    Ok(catalog)
}

impl DperMeasureApp {
    fn native_geometry_listing(&mut self) -> Result<serde_json::Value, String> {
        let key = PresentationKey {
            revision: self.document_revision,
            source_metadata: self.drawing_source_cache_key(),
            generation: self.drawing_geometry_generation,
            page: self.drawing_geometry_ui.page,
            kind: self.drawing_geometry_ui.kind,
            offset: self.drawing_geometry_ui.offset,
        };
        if self.drawing_geometry_ui.presentation_key.as_ref() != Some(&key) {
            let listing = self.handle_drawing_request(DrawingRequest::List(ListGeometryArgs {
                page: key.page, offset: Some(key.offset), limit: Some(PAGE_SIZE),
                kind: KINDS[key.kind].1.map(str::to_owned), geometry_id: None,
            }));
            self.drawing_geometry_ui.presentation_key = Some(key);
            self.drawing_geometry_ui.listing = Some(listing);
            self.drawing_geometry_ui.details.clear();
        }
        self.drawing_geometry_ui.listing.as_ref().unwrap().clone()
    }

    fn start_drawing_geometry_analysis(&mut self, ctx: &egui::Context) {
        if self.drawing_geometry_ui.job.is_some() {
            return;
        }
        let page = self.drawing_geometry_ui.page;
        let expected_revision = self.document_revision;
        let result = self.handle_drawing_request(DrawingRequest::Source(GeometryPageArgs {
            page,
            expected_revision,
        })).and_then(|value| serde_json::from_value::<GeometrySource>(value)
            .map_err(|error| format!("Source PDF invalide : {error}")));
        let source = match result {
            Ok(source) if !source.path.trim().is_empty()
                && !source.document_fingerprint.trim().is_empty() && source.page == page => source,
            Ok(_) => {
                self.drawing_geometry_ui.status = "Source PDF ou page incompatible. Réimportez le plan.".into();
                return;
            }
            Err(error) => {
                self.drawing_geometry_ui.status = error;
                return;
            }
        };
        let (sender, receiver) = mpsc::channel();
        let repaint = ctx.clone();
        let spawned = std::thread::Builder::new().name("drawing-geometry".into()).spawn(move || {
            let result = extract_current_catalog(
                Path::new(&source.path), &source.document_fingerprint, source.page,
            );
            let _ = sender.send(result);
            repaint.request_repaint();
        });
        match spawned {
            Ok(_) => {
                self.drawing_geometry_ui.job = Some(GeometryJob { expected_revision, page, receiver });
                self.drawing_geometry_ui.status = format!("Analyse de la page {page} en cours…");
            }
            Err(error) => self.drawing_geometry_ui.status = format!("Analyse impossible : {error}"),
        }
    }

    pub(crate) fn poll_drawing_geometry_analysis(&mut self, ctx: &egui::Context) {
        let Some(job) = self.drawing_geometry_ui.job.as_ref() else { return; };
        let result = match job.receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => {
                // Keep polling even when the plan panel is closed.
                ctx.request_repaint_after(Duration::from_millis(100));
                return;
            }
            Err(mpsc::TryRecvError::Disconnected) => Err("Analyse vectorielle interrompue. Réessayez.".into()),
        };
        let job = self.drawing_geometry_ui.job.take().unwrap();
        self.drawing_geometry_ui.status = match result.and_then(|catalog| {
            self.handle_drawing_request(DrawingRequest::InstallCatalog {
                catalog, expected_revision: job.expected_revision,
            })
        }) {
            Ok(summary) => format!("Page {} analysée : {} proposition(s).", job.page,
                summary["primitive_count"].as_u64().unwrap_or(0)),
            Err(error) => format!("Analyse non installée : {error}"),
        };
        self.drawing_geometry_ui.offset = 0;
    }

    pub(crate) fn drawing_geometry_panel(&mut self, ui: &mut egui::Ui) {
        let Some(plan) = &self.inspection.plan else { return; };
        let identity = format!("{:?}:{:?}", plan.source_file, plan.source_fingerprint);
        if self.drawing_geometry_ui.source_identity != identity {
            self.drawing_geometry_ui.source_identity = identity;
            self.drawing_geometry_ui.page = 0;
            self.drawing_geometry_ui.offset = 0;
            self.drawing_geometry_ui.status.clear();
        }
        let is_pdf = plan.source_file.as_ref().is_some_and(|path| path.to_lowercase().ends_with(".pdf"));
        let pages = plan.page_geometry.iter().map(|page| page.page).collect::<Vec<_>>();
        if !pages.contains(&self.drawing_geometry_ui.page) {
            self.drawing_geometry_ui.page = pages.first().copied().unwrap_or(0);
            self.drawing_geometry_ui.offset = 0;
        }
        egui::CollapsingHeader::new("Géométries proposées du PDF")
            .default_open(true).show(ui, |ui| {
                ui.weak("Repère : points du PDF. Ces propositions localisent le dessin ; elles ne sont ni des mesures photo, ni une calibration, ni une conformité.");
                ui.weak("Pas d'appariement automatique. Les extrémités de lignes et d'arcs ne sont pas mesurées.");
                if !is_pdf || pages.is_empty() {
                    ui.label("Importez un PDF avec des pages reconnues pour analyser ses vecteurs. Une image ou un scan peut ne contenir aucun vecteur exploitable.");
                    return;
                }
                let busy = self.drawing_geometry_ui.job.is_some();
                let old_page = self.drawing_geometry_ui.page;
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("drawing_geometry_page")
                        .selected_text(format!("Page {}", self.drawing_geometry_ui.page))
                        .show_ui(ui, |ui| {
                            for page in &pages {
                                ui.selectable_value(&mut self.drawing_geometry_ui.page, *page, format!("Page {page}"));
                            }
                        });
                    if ui.add_enabled(!busy, egui::Button::new("Analyser / réanalyser la page")).clicked() {
                        self.start_drawing_geometry_analysis(ui.ctx());
                    }
                    if busy { ui.spinner(); }
                });
                if old_page != self.drawing_geometry_ui.page { self.drawing_geometry_ui.offset = 0; }
                if !self.drawing_geometry_ui.status.is_empty() { ui.label(&self.drawing_geometry_ui.status); }
                ui.weak("Catalogue de session : trois pages au maximum. Après rechargement ou éviction, réanalysez la page.");
                let old_kind = self.drawing_geometry_ui.kind;
                egui::ComboBox::from_id_salt("drawing_geometry_kind")
                    .selected_text(KINDS[old_kind].0).show_ui(ui, |ui| {
                        for (index, (label, _)) in KINDS.iter().enumerate() {
                            ui.selectable_value(&mut self.drawing_geometry_ui.kind, index, *label);
                        }
                    });
                if old_kind != self.drawing_geometry_ui.kind { self.drawing_geometry_ui.offset = 0; }
                let page = self.drawing_geometry_ui.page;
                let listing = self.native_geometry_listing();
                let listing = match listing {
                    Ok(value) => value,
                    Err(error) => { ui.label(error); return; }
                };
                if let Some(catalog) = self.drawing_geometry_cache.iter().find(|catalog| catalog.geometry.page == page) {
                    ui.label(format!("Page : {:.2} × {:.2} pt · {} élément(s) texte/image ignoré(s) · {} tracé(s) non pris en charge",
                        catalog.geometry.width_pt, catalog.geometry.height_pt,
                        catalog.geometry.ignored_text_or_image_count, catalog.geometry.unsupported_path_count));
                    for warning in &catalog.geometry.warnings { ui.label(warning); }
                }
                let total = listing["total"].as_u64().unwrap_or(0) as usize;
                let offset = self.drawing_geometry_ui.offset;
                ui.label(format!("{} proposition(s) · affichage {}–{}", total,
                    if total == 0 { 0 } else { offset + 1 }, (offset + PAGE_SIZE).min(total)));
                ui.horizontal(|ui| {
                    if ui.add_enabled(offset > 0, egui::Button::new("Précédent")).clicked() {
                        self.drawing_geometry_ui.offset = offset.saturating_sub(PAGE_SIZE);
                    }
                    if ui.add_enabled(listing["next_offset"].is_u64(), egui::Button::new("Suivant")).clicked() {
                        self.drawing_geometry_ui.offset = listing["next_offset"].as_u64().unwrap_or(0) as usize;
                    }
                });
                if let Some(items) = listing["items"].as_array() {
                    for item in items {
                        let Some(id) = item["id"].as_str() else { continue; };
                        ui.push_id((page, id), |ui| {
                            let kind = item["kind"].as_str().unwrap_or("");
                            let label = KINDS.iter().find(|(_, value)| *value == Some(kind))
                                .map(|(label, _)| *label).unwrap_or("Géométrie");
                            ui.collapsing(format!("{id} · {label}"), |ui| {
                                if !self.drawing_geometry_ui.details.contains_key(id) {
                                    let detail = self.handle_drawing_request(DrawingRequest::List(ListGeometryArgs {
                                        page, offset: None, limit: Some(1), kind: None, geometry_id: Some(id.into()),
                                    })).and_then(|value| serde_json::from_value::<VectorPrimitive>(value["items"][0].clone())
                                        .map_err(|error| format!("Détail de géométrie invalide : {error}")));
                                    self.drawing_geometry_ui.details.insert(id.into(), detail);
                                }
                                match &self.drawing_geometry_ui.details[id] {
                                    Ok(primitive) => {
                                        ui.label(format!("Boîte PDF : ({:.3}, {:.3})–({:.3}, {:.3}) pt · {} points · fermé : {}",
                                            primitive.bbox.x_min, primitive.bbox.y_min, primitive.bbox.x_max,
                                            primitive.bbox.y_max, primitive.points.len(), primitive.closed));
                                        if let Some(circle) = &primitive.circle {
                                            ui.label(format!("Cercle proposé : centre ({:.3}, {:.3}) pt · rayon {:.3} pt · RMS {:.4} pt · {} échantillons",
                                                circle.center.x, circle.center.y, circle.radius_pt, circle.rms_residual_pt, circle.sample_count));
                                        }
                                        ui.label(format!("Source : élément {} #{} · sous-tracé {} · identifiant {:?}", primitive.source.element_name,
                                            primitive.source.element_index, primitive.source.subpath_index, primitive.source.path_id));
                                        ui.collapsing("Provenance exacte et transformations", |ui| {
                                            ui.label(format!("Transformation : {:?}", primitive.source.transform));
                                            for transform in &primitive.source.transform_chain { ui.label(transform); }
                                            ui.label(&primitive.source.element_attributes);
                                            ui.label(&primitive.source.path_data);
                                            ui.label(format!("Style : {:?}", primitive.style));
                                        });
                                    }
                                    Err(error) => { ui.label(error); }
                                }
                            });
                        });
                    }
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    static NEXT_FIXTURE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    fn catalog(fingerprint: &str, page: u32) -> DrawingGeometryCatalog {
        DrawingGeometryCatalog {
            document_fingerprint: fingerprint.into(),
            geometry: crate::dper_pdf_geometry::parse_svg_geometry(
                "<svg width=\"100pt\" height=\"100pt\" viewBox=\"0 0 100 100\"><circle cx=\"50\" cy=\"50\" r=\"10\"/></svg>", page,
            ).unwrap(),
        }
    }

    #[test]
    fn extraction_result_must_match_requested_source_and_page() {
        assert!(validate_extracted_catalog(catalog("source-a", 1), "source-a", 1).is_ok());
        assert!(validate_extracted_catalog(catalog("source-b", 1), "source-a", 1).is_err());
        assert!(validate_extracted_catalog(catalog("source-a", 2), "source-a", 1).is_err());
    }

    struct Fixture {
        app: DperMeasureApp,
        path: std::path::PathBuf,
        fingerprint: String,
        ctx: egui::Context,
    }

    impl Fixture {
        fn new() -> Self {
            let ctx = egui::Context::default();
            let mut app = DperMeasureApp::new_with_services(ctx.clone(), false);
            let sequence = NEXT_FIXTURE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!("{}-{}-{sequence}.pdf",
                crate::new_id("dper-native-vectors"), std::process::id()));
            // Installation guards verify bytes; SVG geometry below is independently
            // supplied so these tests do not require Poppler or a live MCP server.
            std::fs::write(&path, b"native vector guard fixture").unwrap();
            let fingerprint = crate::dper_source::fingerprint_file(&path).unwrap();
            let mut plan = crate::dper_drawing::parse_text_plan("Diametre 20 mm", "fixture.pdf");
            plan.source_file = Some(path.display().to_string());
            plan.source_fingerprint = Some(fingerprint.clone());
            plan.page_geometry = vec![crate::dper_pdf_layout::PdfPageGeometry {
                page: 1, width_pt: 100.0, height_pt: 100.0,
            }];
            app.install_inspection_plan(plan);
            Self { app, path, fingerprint, ctx }
        }

        fn complete_worker(&mut self, expected_revision: u64) {
            let (sender, receiver) = mpsc::channel();
            sender.send(Ok(catalog(&self.fingerprint, 1))).unwrap();
            self.app.drawing_geometry_ui.job = Some(GeometryJob { expected_revision, page: 1, receiver });
            self.app.poll_drawing_geometry_analysis(&self.ctx);
        }

        fn render(&mut self) {
            let output = self.ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| self.app.drawing_geometry_panel(ui));
            });
            assert!(!output.shapes.is_empty());
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) { let _ = std::fs::remove_file(&self.path); }
    }

    #[test]
    fn completed_native_worker_renders_proposals_without_measurement_mutation() {
        let mut fixture = Fixture::new();
        fixture.render(); // No current catalog: analysis-required state.
        let revision = fixture.app.document_revision;
        fixture.complete_worker(revision);
        assert!(fixture.app.drawing_geometry_ui.job.is_none());
        assert_eq!(fixture.app.drawing_geometry_cache.len(), 1, "{}", fixture.app.drawing_geometry_ui.status);
        fixture.render();
        fixture.app.drawing_geometry_ui.kind = 1; // Empty line filter, circle-only fixture.
        fixture.render();
        assert!(fixture.app.measurements.is_empty());
        assert!(fixture.app.calibration.is_none());
        assert_eq!(fixture.app.document_revision, revision);
        let plan = fixture.app.inspection.plan.as_ref().unwrap().clone();
        fixture.app.install_inspection_plan(plan); // Reload/reimport of same source clears catalogs.
        fixture.render();
        assert!(fixture.app.drawing_geometry_ui.listing.as_ref().unwrap().is_err());
    }

    #[test]
    fn completed_worker_rejects_stale_revision_and_changed_live_source() {
        let mut fixture = Fixture::new();
        let revision = fixture.app.document_revision;
        fixture.app.document_revision += 1;
        fixture.complete_worker(revision);
        assert!(fixture.app.drawing_geometry_cache.is_empty());
        assert!(fixture.app.drawing_geometry_ui.status.contains("non installée"));
        std::fs::write(&fixture.path, b"source changed after extraction").unwrap();
        let revision = fixture.app.document_revision;
        fixture.complete_worker(revision);
        assert!(fixture.app.drawing_geometry_cache.is_empty());
        assert!(fixture.app.drawing_geometry_ui.status.contains("non installée"));
        assert!(fixture.app.measurements.is_empty());
    }

    #[test]
    fn listing_refreshes_after_shared_install_without_revision_change_and_external_edit() {
        let mut fixture = Fixture::new();
        fixture.app.drawing_geometry_ui.page = 1;
        assert!(fixture.app.native_geometry_listing().is_err());
        let revision = fixture.app.document_revision;
        fixture.complete_worker(revision);
        let listing = fixture.app.native_geometry_listing().unwrap();
        assert!(listing["total"].as_u64().unwrap() > 0);
        let mut replacement = catalog(&fixture.fingerprint, 1);
        replacement.geometry.primitives.clear();
        fixture.app.handle_drawing_request(DrawingRequest::InstallCatalog {
            catalog: replacement, expected_revision: revision,
        }).unwrap();
        assert_eq!(fixture.app.document_revision, revision);
        assert_eq!(fixture.app.native_geometry_listing().unwrap()["total"], 0);
        std::fs::write(&fixture.path, b"externally edited source with different length").unwrap();
        assert!(fixture.app.native_geometry_listing().is_err());
    }
}
