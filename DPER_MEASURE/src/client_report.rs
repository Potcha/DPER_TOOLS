use crate::{
    delivery_balance::{CheckStatus, DeliveryReview, DeliveryStatus, LotBalance},
    report::FolderReportPhoto,
};
use serde::Serialize;
use std::{collections::BTreeMap, path::Path};

#[derive(Serialize)]
pub(crate) struct ClientReport {
    pub(crate) lots: BTreeMap<String, ClientLot>,
}

#[derive(Serialize)]
pub(crate) struct ClientLot {
    pub(crate) balance: LotBalance,
    pub(crate) parts: BTreeMap<String, ClientPart>,
}

#[derive(Serialize)]
pub(crate) struct ClientPart {
    pub(crate) identity: crate::part_identity::PartIdentity,
    pub(crate) status: DeliveryStatus,
    pub(crate) checks: BTreeMap<String, ClientCheck>,
}

#[derive(Serialize)]
pub(crate) struct ClientCheck {
    pub(crate) label: String,
    pub(crate) nominal: Option<f64>,
    pub(crate) unit: Option<String>,
    pub(crate) tolerance: Option<String>,
    pub(crate) status: CheckStatus,
    pub(crate) reason: String,
    pub(crate) observations: Vec<ClientObservation>,
}

#[derive(Serialize)]
pub(crate) struct ClientObservation {
    pub(crate) capture: String,
    pub(crate) value: Option<f64>,
    pub(crate) uncertainty: Option<f64>,
}

impl ClientReport {
    pub(crate) fn from_photos(root: &Path, photos: &[FolderReportPhoto]) -> Self {
        let mut lots = BTreeMap::<String, (DeliveryReview, BTreeMap<String, ClientPart>)>::new();
        for photo in photos {
            let session = &photo.session;
            let Some(identity) = session.part_identity.as_ref() else { continue; };
            let (review, parts) = lots.entry(identity.lot_id.clone())
                .or_insert_with(|| (DeliveryReview::for_lot(root, &identity.lot_id), BTreeMap::new()));
            review.collect_verified(session, photo.delivery_source_error.as_deref());
            let part = parts.entry(identity.part_id.clone()).or_insert_with(|| ClientPart {
                identity: identity.clone(), status: DeliveryStatus::Incomplete, checks: BTreeMap::new(),
            });
            if let Some(document) = &session.reference_document {
                for dimension in &document.dimensions {
                    let id = format!("{} · {}", dimension.label, dimension.id);
                    let comparison = crate::reference::compare_dimension(dimension, &session.measurements);
                    let tolerance = dimension.tolerance_bounds_mm().map(|(low, high)| format!("{low:.4} à {high:.4}"));
                    let check = part.checks.entry(id).or_insert_with(|| ClientCheck {
                        label: dimension.label.clone(), nominal: Some(dimension.nominal_mm), unit: Some("mm".into()),
                        tolerance, status: CheckStatus::ToReview, reason: "Contrôle à confirmer.".into(),
                        observations: Vec::new(),
                    });
                    if let Some(value) = comparison.measured_mm {
                        check.observations.push(ClientObservation {
                            capture: session.capture_id.clone(), value: Some(value), uncertainty: comparison.expanded_uncertainty_mm,
                        });
                    }
                }
            }
            if let Some(workspace) = session.automated_inspection.as_ref().filter(|ws| ws.plan.is_some()) {
                for row in crate::drawing_control_table::rows(workspace) {
                    let id = format!("{} · {}", row.label, row.id);
                    let check = part.checks.entry(id).or_insert_with(|| ClientCheck {
                        label: row.label.into(), nominal: row.nominal,
                        unit: (row.unit != "—").then(|| row.unit.to_owned()),
                        tolerance: row.tolerance.map(str::to_owned), status: CheckStatus::ToReview,
                        reason: row.reason, observations: Vec::new(),
                    });
                    for record in workspace.records.iter().filter(|record| record.measurement_id.is_some()
                        && record.capture_id == session.capture_id && record.withdrawal.is_none()
                        && record.occurrence.as_ref().map_or(record.feature_id.as_str(), |proof| proof.member.id.as_str()) == row.id)
                    {
                        check.observations.push(ClientObservation {
                            capture: session.capture_id.clone(), value: record.value_mm,
                            uncertainty: record.decision_evidence.as_ref().map(|evidence| evidence.basis.expanded_uncertainty_mm),
                        });
                    }
                }
            }
        }
        Self { lots: lots.into_iter().map(|(lot, (review, mut parts))| {
            let balance = review.balance();
            for (part_id, part) in &mut parts {
                if let Some(result) = balance.present.get(part_id) {
                    part.status = result.status;
                    for (id, check) in &mut part.checks {
                        if let Some(result) = result.checks.get(id) {
                            check.status = result.status;
                            check.reason.clone_from(&result.reason);
                        }
                    }
                } else {
                    for check in part.checks.values_mut() {
                        check.reason = "Pièce ou révision hors du registre du lot.".into();
                    }
                }
            }
            (lot, ClientLot { balance, parts })
        }).collect() }
    }
}

#[cfg(test)]
#[test]
fn llt_exp_006_and_hlt_exp_006_render_twenty_parts_with_index_and_all_controls() {
    use crate::{delivery_balance::{CheckStatus, DeliveryStatus}, part_identity::PartIdentity, report};
    let case = tempfile::tempdir().unwrap();
    let mut photos = Vec::new();
    for number in 1..=20 {
        let part_id = format!("P{number:02}");
        let identity = PartIdentity::new("LOT-CLIENT", &part_id, "CLIENT-R1", "PLAN-R1").unwrap();
        identity.create(case.path()).unwrap();
        let mut photo = report::test_support::dense_photo();
        photo.reference = None;
        photo.session.part_identity = Some(identity);
        photo.session.capture_id = format!("capture-{part_id}");
        photo.session.automated_inspection = None;
        photo.session.measurements.truncate(50);
        photo.session.calibration = Some(serde_json::from_value(serde_json::json!({
            "id": "scale", "device": "fixture", "device_id": "fixture",
            "optical_setup": "objectif", "resolution": [photo.width, photo.height],
            "mm_per_pixel": 0.1, "reference_mm": 1.0, "reference_px": 10.0, "validation_status": "verified"
        })).unwrap());
        photo.session.device_id = "fixture".into();
        photo.session.optical_setup = "objectif".into();
        let mut dimensions = Vec::new();
        for (index, measurement) in photo.session.measurements.iter_mut().enumerate() {
            let control = format!("C{:02}", index + 1);
            measurement.kind = crate::Tool::Distance;
            measurement.points = vec![crate::Point { x: 2.0, y: 3.0 }, crate::Point {
                x: if index == 4 && number == 2 { 16.0 } else { 12.0 }, y: 3.0,
            }];
            measurement.capture_id.clone_from(&photo.session.capture_id);
            measurement.calibration_id = Some("scale".into());
            measurement.mm_per_pixel = Some(0.1);
            measurement.mm_per_pixel_x = None;
            measurement.mm_per_pixel_y = None;
            measurement.expanded_uncertainty_mm = Some(0.02);
            measurement.origin = "manual".into();
            dimensions.push(crate::reference::ReferenceDimension {
                id: format!("{part_id}-{control}"), label: control, nominal_mm: 1.0,
                tolerance_minus_mm: Some(0.1), tolerance_plus_mm: Some(0.1), source_text: "1 ±0.1 mm".into(),
                confirmed: true, measurement_id: (number != 13 || index != 49).then_some(measurement.id),
                ocr_page: None, ocr_bbox: None, ocr_confidence: None,
            });
        }
        photo.session.reference_document = Some(crate::reference::ReferenceDocument::new(
            &case.path().join("plan.pdf"), String::new(), dimensions,
        ));
        photos.push(photo);
    }
    let data = ClientReport::from_photos(case.path(), &photos);
    let lot = &data.lots["LOT-CLIENT"];
    assert_eq!(lot.balance.status, DeliveryStatus::Nonconforming);
    assert_eq!(lot.parts.len(), 20);
    assert!(lot.parts.values().all(|part| part.checks.len() == 50));
    assert_eq!(lot.parts["P01"].status, DeliveryStatus::Conforming);
    assert_eq!(lot.parts["P02"].status, DeliveryStatus::Nonconforming);
    assert_eq!(lot.parts["P13"].status, DeliveryStatus::Incomplete);
    assert_eq!(lot.parts["P13"].checks["C50 · P13-C50"].status, CheckStatus::Missing);
    let pdf = case.path().join("client.pdf");
    let mut options = report::test_support::dense_options();
    options.client_report = Some(data);
    report::write_folder_pdf(&pdf, &options, &photos).unwrap();
    let binary = std::fs::read(&pdf).unwrap();
    let text = String::from_utf8_lossy(&binary);
    assert_eq!(text.matches("/Subtype /Link").count(), 21);
    assert!(text.contains("P13-C50"));
    assert!(text.matches("Statut et motif").count() > 20);
    if let Some(output) = std::env::var_os("DPER_TEST_CLIENT_PDF_OUTPUT") {
        let output = std::path::PathBuf::from(output);
        std::fs::create_dir_all(&output).unwrap();
        std::fs::copy(pdf, output.join("client.pdf")).unwrap();
        let manifest = serde_json::to_vec_pretty(&options.client_report).unwrap();
        std::fs::write(output.join("client.json"), manifest).unwrap();
    }
}
