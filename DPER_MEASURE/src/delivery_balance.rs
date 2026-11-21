use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CheckStatus {
    Conforming,
    Nonconforming,
    Missing,
    ToReview,
    Stale,
}

impl CheckStatus {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Conforming => "Conforme",
            Self::Nonconforming => "Non conforme",
            Self::Missing => "Manquant",
            Self::ToReview => "À confirmer",
            Self::Stale => "À reprendre",
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
#[derive(serde::Serialize)]
pub(crate) struct CheckResult {
    pub(crate) status: CheckStatus,
    pub(crate) reason: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DeliveryStatus {
    Conforming,
    Nonconforming,
    Incomplete,
}

impl DeliveryStatus {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Conforming => "Conforme",
            Self::Nonconforming => "Non conforme",
            Self::Incomplete => "À compléter ou à confirmer",
        }
    }
}

#[derive(serde::Serialize)]
pub(crate) struct PartBalance {
    pub(crate) status: DeliveryStatus,
    pub(crate) checks: BTreeMap<String, CheckResult>,
}

#[derive(serde::Serialize)]
pub(crate) struct LotBalance {
    pub(crate) status: DeliveryStatus,
    pub(crate) present: BTreeMap<String, PartBalance>,
    pub(crate) missing: BTreeSet<String>,
}

fn status(states: impl Iterator<Item = DeliveryStatus>, complete: bool) -> DeliveryStatus {
    let mut all_conforming = complete;
    for state in states {
        match state {
            DeliveryStatus::Nonconforming => return DeliveryStatus::Nonconforming,
            DeliveryStatus::Incomplete => all_conforming = false,
            DeliveryStatus::Conforming => {}
        }
    }
    if all_conforming { DeliveryStatus::Conforming } else { DeliveryStatus::Incomplete }
}

pub(crate) fn part_balance(required: &BTreeSet<String>, retained: &BTreeMap<String, CheckResult>) -> PartBalance {
    let checks: BTreeMap<_, _> = required.iter().map(|id| {
        let result = match retained.get(id) {
            Some(result) => CheckResult { status: result.status, reason: result.reason.clone() },
            None => CheckResult { status: CheckStatus::Missing, reason: "Aucun résultat retenu pour ce contrôle obligatoire.".into() },
        };
        (id.clone(), result)
    }).collect();
    let status = status(checks.values().map(|check| match check.status {
        CheckStatus::Conforming => DeliveryStatus::Conforming,
        CheckStatus::Nonconforming => DeliveryStatus::Nonconforming,
        CheckStatus::Missing | CheckStatus::ToReview | CheckStatus::Stale => DeliveryStatus::Incomplete,
    }), !required.is_empty());
    PartBalance { status, checks }
}

pub(crate) fn lot_balance(expected: &BTreeSet<String>, mut actual: BTreeMap<String, PartBalance>) -> LotBalance {
    actual.retain(|part, _| expected.contains(part));
    let missing = expected.iter().filter(|part| !actual.contains_key(*part)).cloned().collect::<BTreeSet<_>>();
    let status = status(actual.values().map(|part| part.status), !expected.is_empty() && missing.is_empty());
    LotBalance { status, present: actual, missing }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llt_exp_005_01_counts_required_checks_and_refuses_an_incomplete_lot() {
        use CheckStatus::{Conforming, Missing, Nonconforming, Stale, ToReview};
        let required: BTreeSet<_> = ["ok", "nok", "missing", "stale", "uncertain"]
            .map(str::to_owned).into_iter().collect();
        let mut retained: BTreeMap<_, _> = [("ok", Conforming), ("nok", Nonconforming),
            ("stale", Stale), ("uncertain", ToReview)].map(|(id, status)| {
                (id.to_owned(), CheckResult { status, reason: status.label().into() })
            }).into_iter().collect();
        let part = part_balance(&required, &retained);
        for (id, expected) in [("ok", Conforming), ("nok", Nonconforming),
            ("missing", Missing), ("stale", Stale), ("uncertain", ToReview)]
        {
            assert_eq!(part.checks[id].status, expected);
            assert!(!part.checks[id].reason.is_empty());
        }
        assert_eq!(part.status, DeliveryStatus::Nonconforming);
        retained.remove("nok");
        assert_eq!(part_balance(&required, &retained).status, DeliveryStatus::Incomplete);
        assert_eq!(part_balance(&BTreeSet::new(), &retained).status, DeliveryStatus::Incomplete);

        let required = BTreeSet::from(["ok".into()]);
        let expected = BTreeSet::from(["P1".into(), "P2".into()]);
        let complete = || part_balance(&required, &retained);
        let lot = lot_balance(&expected, BTreeMap::from([("P1".into(), complete()), ("P2".into(), complete())]));
        assert_eq!(lot.status, DeliveryStatus::Conforming);
        let lot = lot_balance(&expected, BTreeMap::from([("P1".into(), complete()), ("OTHER".into(), complete())]));
        assert_eq!(lot.status, DeliveryStatus::Incomplete);
        assert_eq!(lot.present.len(), 1);
        assert_eq!(lot.missing, BTreeSet::from(["P2".into()]));
    }
}

#[derive(Default)]
pub(crate) struct DeliveryReview {
    expected: BTreeMap<String, crate::part_identity::PartIdentity>,
    required_by_part: BTreeMap<String, BTreeSet<String>>,
    checks_by_part: BTreeMap<String, BTreeMap<String, CheckResult>>,
    documents_by_part: BTreeMap<String, BTreeSet<String>>,
    registry_error: Option<String>,
    incomplete_scan: bool,
    lot: String,
}

impl DeliveryReview {
    pub(crate) fn for_lot(root: &std::path::Path, lot: &str) -> Self {
        if lot.is_empty() { return Self::default(); }
        let mut review = Self { lot: lot.to_owned(), ..Default::default() };
        match crate::part_identity::PartIdentity::expected_parts(root, lot) {
            Ok(expected) => review.expected = expected,
            Err(reason) => review.registry_error = Some(reason),
        }
        review
    }

    pub(crate) fn collect(&mut self, session: &crate::storage::Session, path: &std::path::Path) {
        let base = path.parent().unwrap_or_else(|| std::path::Path::new("."));
        let source_error = verify_manual_source(session, base).err();
        self.collect_verified(session, source_error.as_deref());
    }

    pub(crate) fn collect_verified(&mut self, session: &crate::storage::Session, source_error: Option<&str>) {
        let Some(identity) = session.part_identity.as_ref() else { return; };
        let Some(expected) = self.expected.get(&identity.part_id) else { return; };
        if expected != identity { return; }
        let required = self.required_by_part.entry(identity.part_id.clone()).or_default();
        let checks = self.checks_by_part.entry(identity.part_id.clone()).or_default();
        if let Some(document) = session.reference_document.as_ref() {
            if let Some(fingerprint) = &document.source_fingerprint {
                self.documents_by_part.entry(identity.part_id.clone()).or_default().insert(fingerprint.clone());
            }
            for dimension in &document.dimensions {
                let id = format!("{} · {}", dimension.label, dimension.id);
                required.insert(id.clone());
                let comparison = crate::reference::compare_dimension(dimension, &session.measurements);
                let status = comparison.status;
                let result = match (status, source_error) {
                    (crate::reference::ComparisonStatus::NotAssociated, _) => continue,
                    (_, Some(reason)) => CheckResult { status: CheckStatus::Stale, reason: reason.into() },
                    (crate::reference::ComparisonStatus::InTolerance | crate::reference::ComparisonStatus::OutOfTolerance, None)
                        if !verified_measurement_calibration(session, dimension.measurement_id) => CheckResult {
                            status: CheckStatus::ToReview,
                            reason: "Étalonnage courant applicable à la mesure à confirmer.".into(),
                        },
                    (crate::reference::ComparisonStatus::InTolerance, None) => CheckResult {
                        status: CheckStatus::Conforming, reason: status.label().into(),
                    },
                    (crate::reference::ComparisonStatus::OutOfTolerance, None) => CheckResult {
                        status: CheckStatus::Nonconforming, reason: status.label().into(),
                    },
                    (_, None) => CheckResult { status: CheckStatus::ToReview, reason: status.label().into() },
                };
                add_result(checks, id, result);
            }
        }
        if let Some(workspace) = session.automated_inspection.as_ref().filter(|ws| ws.plan.is_some()) {
            // An imported vision decision is historical until reassessed in the application.
            for row in crate::drawing_control_table::rows(workspace) {
                let id = format!("{} · {}", row.label, row.id);
                required.insert(id.clone());
                if row.status == "Exclue" || row.target.as_ref().is_none() {
                    add_result(checks, id, CheckResult { status: CheckStatus::ToReview, reason: row.reason });
                    continue;
                }
                if workspace.records.iter().any(|record| record.measurement_id.is_some()
                    && record.capture_id == session.capture_id && record.withdrawal.is_none()
                    && record.occurrence.as_ref().map_or(record.feature_id.as_str(), |proof| proof.member.id.as_str()) == row.id)
                {
                    add_result(checks, id, CheckResult { status: CheckStatus::Stale,
                        reason: "Observation du projet à réévaluer avant décision.".into() });
                }
            }
        }
    }

    pub(crate) fn mark_scan_incomplete(&mut self) { self.incomplete_scan = true; }

    pub(crate) fn balance(&self) -> LotBalance {
        let expected = self.expected.keys().cloned().collect();
        let actual = self.required_by_part.iter().filter_map(|(part, required)| {
            let mut balance = part_balance(required, self.checks_by_part.get(part)?);
            if self.documents_by_part.get(part).is_some_and(|documents| documents.len() > 1) {
                for check in balance.checks.values_mut() {
                    if matches!(check.status, CheckStatus::Conforming | CheckStatus::Nonconforming) {
                        check.status = CheckStatus::ToReview;
                        check.reason = "Plusieurs documents différents portent la même révision déclarée.".into();
                    }
                }
                balance.status = DeliveryStatus::Incomplete;
            }
            Some((part.clone(), balance))
        }).collect();
        let mut balance = lot_balance(&expected, actual);
        if self.incomplete_scan && balance.status == DeliveryStatus::Conforming {
            balance.status = DeliveryStatus::Incomplete;
        }
        balance
    }

    pub(crate) fn show(&self, ui: &mut egui::Ui) {
        if self.lot.is_empty() { return; }
        if let Some(error) = &self.registry_error {
            ui.label(error);
            return;
        }
        if self.expected.is_empty() {
            ui.weak("Déclarez les pièces attendues dans le registre du lot pour établir un bilan.");
            return;
        }
        let balance = self.balance();
        ui.collapsing(format!("Bilan lot {} · {}", self.lot, balance.status.label()), |ui| {
            ui.label(format!("{} pièce(s) dans le bilan · {} manquante(s)", balance.present.len(), balance.missing.len()));
            if self.incomplete_scan { ui.label("Recherche partielle : le bilan de lot doit être vérifié."); }
            for id in &balance.missing { ui.label(format!("Pièce {id} · manquante")); }
            for (id, part) in &balance.present {
                ui.collapsing(format!("Pièce {id} · {}", part.status.label()), |ui| {
                    if part.checks.is_empty() { ui.label("Aucun contrôle obligatoire identifié."); }
                    for (check_id, result) in &part.checks {
                        ui.label(format!("{check_id} · {} · {}", result.status.label(), result.reason));
                    }
                });
            }
        });
    }
}

fn add_result(checks: &mut BTreeMap<String, CheckResult>, id: String, result: CheckResult) {
    use std::collections::btree_map::Entry;
    match checks.entry(id) {
        Entry::Vacant(entry) => { entry.insert(result); }
        Entry::Occupied(mut entry) if entry.get().status != result.status => {
            entry.insert(CheckResult { status: CheckStatus::ToReview,
                reason: "Plusieurs captures présentent des résultats différents. Revue nécessaire.".into() });
        }
        Entry::Occupied(_) => {}
    }
}

fn verify_manual_source(session: &crate::storage::Session, folder: &std::path::Path) -> Result<(), String> {
    let source = crate::reference::resolve_source(&session.image_file, folder);
    let image = image::open(source).map_err(|_| "Image source manquante ou illisible.")?.to_rgb8();
    verify_manual_capture(session, folder, image.width(), image.height(), image.as_raw())
}

pub(crate) fn verify_manual_capture(session: &crate::storage::Session, folder: &std::path::Path,
    width: u32, height: u32, pixels: &[u8]) -> Result<(), String> {
    if session.source_image_fingerprint.is_none() {
        return Err("Identité de l’image source non vérifiée.".into());
    }
    session.verify_source_pixels(width, height, pixels)?;
    if session.calibration.as_ref().is_some_and(|profile| profile.resolution != [width, height]) {
        return Err("La résolution ne correspond plus au profil d’étalonnage.".into());
    }
    let document = session.reference_document.as_ref().ok_or("Document de référence absent.")?;
    let reference = crate::reference::resolve_source(&document.source_file, folder);
    let expected = document.source_fingerprint.as_deref().ok_or("Identité du document de référence non vérifiée.")?;
    if crate::dper_source::fingerprint_file(&reference)?.as_str() != expected {
        return Err("Le document de référence a changé depuis son import.".into());
    }
    let quality = crate::quality::assess_image_quality_with_correction(
        pixels, width, height, &session.quality_thresholds, session.display_correction,
    )?;
    if !quality.is_exploitable { return Err("Qualité d’image non exploitable.".into()); }
    if let Some(reason) = crate::folder_export::session_protocol_notice(session) { return Err(reason); }
    Ok(())
}

fn verified_measurement_calibration(session: &crate::storage::Session, id: Option<u64>) -> bool {
    let measurement = session.measurements.iter().find(|candidate| Some(candidate.id) == id);
    let Some((measurement, profile)) = measurement.zip(session.calibration.as_ref()) else { return false; };
    profile.validation_status == "verified"
        && measurement.capture_id == session.capture_id
        && measurement.calibration_id.as_deref() == Some(profile.id.as_str())
        && profile.device_id == session.device_id
        && profile.optical_setup == session.optical_setup
        && measurement.mm_per_pixel_x.or(measurement.mm_per_pixel) == profile.scale_x_mm_per_pixel.or(Some(profile.mm_per_pixel))
        && measurement.mm_per_pixel_y.or(measurement.mm_per_pixel) == profile.scale_y_mm_per_pixel.or(Some(profile.mm_per_pixel))
}

#[cfg(test)]
#[test]
fn hlt_exp_005_lot_search_reports_nonconforming_and_incomplete_parts() {
    use crate::{part_identity::PartIdentity, reference::{ReferenceDimension, ReferenceDocument}};
    let workspace = tempfile::tempdir().unwrap();
    let root = workspace.path();
    let source = root.join("source.png");
    let image = image::RgbImage::from_fn(32, 24, |x, _| {
        image::Rgb([if x % 8 < 4 { 65 } else { 190 }; 3])
    });
    image.save(&source).unwrap();
    let document_path = root.join("reference.png");
    image.save(&document_path).unwrap();
    assert!(crate::quality::assess_image_quality(image.as_raw(), 32, 24, &Default::default()).unwrap().is_exploitable);
    for (part, length, associated) in [("P1", 10.0, true), ("P2", 16.0, true), ("P3", 10.0, false)] {
        let identity = PartIdentity::new("LOT", part, "Exigences-R1", "Plan-R1").unwrap();
        identity.create(root).unwrap();
        let mut session: crate::storage::Session = serde_json::from_value(serde_json::json!({
            "schema_version": crate::storage::CURRENT_SCHEMA_VERSION,
            "part_identity": identity,
            "image_file": "source.png",
            "capture_id": part,
            "device_id": "test-camera",
            "optical_setup": "test-optic",
            "calibration": {
                "id": "fixture-scale", "device": "test-camera", "device_id": "test-camera",
                "optical_setup": "test-optic", "resolution": [32,24], "mm_per_pixel": 0.1,
                "reference_mm": 1.0, "reference_px": 10.0, "validation_status": "verified"
            },
            "measurements": [{
                "id": 1, "kind": "Distance", "label": "Longueur", "capture_id": part, "calibration_id": "fixture-scale",
                "mm_per_pixel": 0.1, "expanded_uncertainty_mm": 0.02,
                "points": [{"x":2.0,"y":3.0},{"x":2.0+length,"y":3.0}]
            }]
        })).unwrap();
        session.source_image_fingerprint = Some(format!("{:016x}",
            crate::dper_inspection::fingerprint_pixels(32, 24, image.as_raw())));
        session.reference_document = Some(ReferenceDocument::new(&document_path,
            "1 ±0.1 mm".into(), vec![ReferenceDimension {
                id: "length".into(), label: "Longueur".into(), nominal_mm: 1.0,
                tolerance_minus_mm: Some(0.1), tolerance_plus_mm: Some(0.1),
                source_text: "1 ±0.1 mm".into(), confirmed: true,
                measurement_id: associated.then_some(1), ocr_page: None, ocr_bbox: None, ocr_confidence: None,
            }]));
        std::fs::write(root.join(format!("{part}.dperproj")), serde_json::to_vec(&session).unwrap()).unwrap();
    }

    let (_, _, _, review) = crate::scan_project_search(root, "", "LOT", "").unwrap();
    let balance = review.balance();
    assert_eq!(balance.status, DeliveryStatus::Nonconforming);
    assert_eq!(balance.present["P1"].status, DeliveryStatus::Conforming);
    assert_eq!(balance.present["P2"].status, DeliveryStatus::Nonconforming);
    assert_eq!(balance.present["P3"].status, DeliveryStatus::Incomplete);
    assert_eq!(balance.present["P3"].checks["Longueur · length"].status, CheckStatus::Missing);
    assert!(balance.missing.is_empty());
    std::fs::remove_file(root.join("P2.dperproj")).unwrap();
    let (_, _, _, review) = crate::scan_project_search(root, "", "LOT", "").unwrap();
    let balance = review.balance();
    assert_eq!(balance.status, DeliveryStatus::Incomplete);
    assert_eq!(balance.missing, BTreeSet::from(["P2".into()]));
    let changed_reference = image::RgbImage::from_pixel(32, 24, image::Rgb([180; 3]));
    changed_reference.save(document_path).unwrap();
    let (_, _, _, review) = crate::scan_project_search(root, "", "LOT", "").unwrap();
    assert_eq!(review.balance().present["P1"].checks["Longueur · length"].status, CheckStatus::Stale);
}
