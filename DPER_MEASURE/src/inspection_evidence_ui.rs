//! Read-only presentation of application-reassessed inspection evidence. Origin
//! strings never confer geometry validation, physical qualification or conformity.
use crate::{
    dper_inspection::{InspectionRecord, InspectionWorkspace},
    dper_metrology::{self, GeometryState, ValidationThresholds},
    dper_vision::InspectionGeometry,
    geometry::Measurement,
};
use eframe::egui;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EvidenceRole {
    Construction,
    Proposed,
    Measured,
    SoftwareValidated,
}

impl EvidenceRole {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Construction => "Construction · hors mesure",
            Self::Proposed => "Proposée · à observer ou réévaluer",
            Self::Measured => "Mesurée · à vérifier",
            Self::SoftwareValidated => "Vérifiée par logiciel",
        }
    }
}

fn latest_record<'a>(measurement: &Measurement, state: &'a InspectionWorkspace) -> Option<&'a InspectionRecord> {
    state.records.iter().rev().find(|record| record.measurement_id == Some(measurement.id)
        && record.capture_id == measurement.capture_id)
}

pub(crate) fn measurement_role(measurement: &Measurement, state: &InspectionWorkspace) -> EvidenceRole {
    if measurement.is_construction { return EvidenceRole::Construction; }
    if measurement.is_numeric_resize() { return EvidenceRole::Proposed; }
    if measurement.dependency_stale || state.dependencies.is_stale(measurement.id) {
        return EvidenceRole::Measured;
    }
    match latest_record(measurement, state).map(|record| record.outcome.geometry_state) {
        Some(GeometryState::Proposed) => EvidenceRole::Proposed,
        Some(GeometryState::Validated) => EvidenceRole::SoftwareValidated,
        // A native manual trace remains a measurement to verify. Its provenance
        // cannot replace current contour evidence from the inspection service.
        _ => EvidenceRole::Measured,
    }
}

fn suspended_value(record: &InspectionRecord) -> bool {
    record.imported || record.outcome.blocking_reasons.iter().any(|reason|
        (reason != "decision_evidence_changed" && reason.contains("changed")) || reason.contains("stale") || reason.contains("mismatch")
            || matches!(reason.as_str(), "source_image_missing" | "not_observable_in_2d"
                | "drawing_feature_missing_or_unobservable")
            || (reason == "measurement_missing" && record.measurement_id.is_some()))
}

pub(crate) fn measurement_display_value(measurement: &Measurement, state: &InspectionWorkspace) -> String {
    if measurement.is_numeric_resize() { return crate::geometry::result(measurement); }
    if latest_record(measurement, state).is_some_and(suspended_value) {
        "Valeur suspendue · nouvelle observation requise".into()
    } else { crate::geometry::result(measurement) }
}

fn record_role(record: &InspectionRecord) -> EvidenceRole {
    if record.measurement_id.is_none() { return EvidenceRole::Proposed; }
    match record.outcome.geometry_state {
        GeometryState::Proposed => EvidenceRole::Proposed,
        GeometryState::Measured => EvidenceRole::Measured,
        GeometryState::Validated => EvidenceRole::SoftwareValidated,
    }
}

fn dependency_label(measurement: &Measurement, state: &InspectionWorkspace) -> String {
    if measurement.source_measurement_ids.is_empty() { return "Dépendances : aucune source liée".into(); }
    let ids = measurement.source_measurement_ids.iter().map(|id| format!("M{id}")).collect::<Vec<_>>().join(", ");
    let freshness = if measurement.dependency_stale || state.dependencies.is_stale(measurement.id) {
        " · périmées, recalcul requis"
    } else { "" };
    format!("Dépendances : {ids}{freshness}")
}

fn visible_suspensions(ui: &mut egui::Ui, record: &InspectionRecord) {
    for reason in &record.outcome.blocking_reasons {
        if reason == "decision_evidence_changed" {
            ui.label("Conformité suspendue : base documentaire ou contexte de décision à revoir.");
        } else if reason.contains("changed") || reason.contains("stale")
            || matches!(reason.as_str(), "capture_mismatch" | "not_observable_in_2d"
                | "drawing_feature_missing_or_unobservable" | "image_quality_failed"
                | "imported_evidence_requires_reassessment") {
            ui.label(egui::RichText::new(format!("Suspendu : {}", dper_metrology::reason_label(reason)))
                .small().color(egui::Color32::from_rgb(153, 86, 25)));
        }
    }
}

fn evidence_details(ui: &mut egui::Ui, record: &InspectionRecord, state: &InspectionWorkspace) {
    for line in crate::inspection_decision_presentation::details(record, state.plan.as_ref()) { ui.label(line); }
    let evidence = &record.result.evidence;
    ui.label(format!("Capture : {} · étalonnage : {}", record.capture_id,
        record.calibration_id.as_deref().unwrap_or("absent")));
    ui.label(format!("Empreinte des pixels observés : {:016x}", record.source_fingerprint));
    if record.imported { ui.label("Preuves historiques importées : nouvelle observation requise."); }
    if let Some(binding) = &record.drawing_binding_id { ui.label(format!("Association au plan : {binding}")); }
    if let Some(proof) = &record.occurrence {
        ui.label(format!("Occurrence {} · objet PDF {} · vue {}", proof.member.number, proof.member.geometry_id, proof.view_id));
    }
    ui.label(format!("Points acceptés : {} · rejetés/absents : {} · profils : {}",
        evidence.accepted_points.len(), evidence.rejected_points.len(), evidence.sample_count));
    ui.label(format!("Couverture des profils : {:.1} % · RMS : {:.3} px · écart maximum : {:.3} px",
        evidence.coverage_fraction * 100.0, evidence.rms_residual_px, evidence.max_residual_px));
    if let Some(clearance) = &record.clearance {
        ui.label("Passage à centre imposé : cercle dérivé des butées, sans observation d'un contour circulaire continu.");
        ui.label(format!("Centre ({:.3}, {:.3}) px · diamètre {:.3} px · butée limitante {}",
            clearance.result.center.x, clearance.result.center.y, clearance.result.diameter_px, clearance.result.limiting_stop_id));
        for stop in &clearance.result.stops {
            ui.label(format!("{} : contact ({:.3}, {:.3}) px · distance {:.3} px · jeu radial {:.3} px · RMS {:.3} px · profils {:.1} %",
                stop.id, stop.closest_point.x, stop.closest_point.y, stop.distance_px, stop.gap_px,
                stop.evidence.rms_residual_px, stop.evidence.coverage_fraction * 100.0));
        }
        for limitation in &clearance.result.limitations { ui.label(limitation); }
    } else {
        ui.label(format!("Contour observé en pixels natifs : {:?}", record.result.geometry));
    }
    if matches!(record.result.geometry, InspectionGeometry::Line { .. } | InspectionGeometry::Arc { .. }) {
        ui.label("Les extrémités réelles ne sont pas mesurées ; le support de droite ou le rayon d'arc ne détermine pas leur étendue.");
    }
    ui.separator();
    ui.label("Réglages de recherche conservés avec cette observation");
    if let Some(options) = &record.vision_options {
        ui.label(format!("Profils : {} · rayon de recherche : {:.3} px · gradient minimum : {:.3}",
            options.sample_count, options.search_radius_px, options.minimum_gradient));
        ui.label(format!("Polarité : {:?} · couverture minimum : {:.1} %", options.polarity, options.minimum_coverage * 100.0));
        ui.label(format!("Rejet des écarts : {:.3} px · RMS maximum de recherche : {:.3} px · arc minimum : {:.1}°",
            options.outlier_threshold_px, options.maximum_rms_px, options.minimum_arc_sweep_rad.to_degrees()));
    } else {
        ui.label("Réglages de recherche inconnus dans cet historique ; aucune valeur par défaut n'est attribuée.");
    }
    let thresholds = ValidationThresholds::default();
    ui.label("Seuils actuels de réévaluation logicielle des contours");
    ui.label(format!("Points minimum : {} · RMS maximum : {:.3} px · écart maximum : {:.3} px · couverture minimum : {:.1} %",
        thresholds.min_samples, thresholds.max_fit_rms, thresholds.max_fit_max, thresholds.min_coverage * 100.0));
    ui.label("Ces seuils logiciels ne qualifient ni le montage physique ni l'incertitude totale de mesure.");
    ui.label("Motifs de l'évaluation courante");
    if record.outcome.blocking_reasons.is_empty() { ui.label("Aucun motif bloquant signalé par l'évaluation courante."); }
    for reason in &record.outcome.blocking_reasons { ui.label(dper_metrology::reason_label(reason)); }
}

/// `state` must be one application-reassessed snapshot, shared across all cards.
/// This helper never clones/reassesses the complete workspace per measurement.
pub(crate) fn measurement_evidence_card(ui: &mut egui::Ui, measurement: &Measurement, state: &InspectionWorkspace) {
    let role = measurement_role(measurement, state);
    ui.label(egui::RichText::new(role.label()).small().strong());
    let record = latest_record(measurement, state);
    ui.label(egui::RichText::new(match record {
        Some(record) => format!("Groupe : {} · caractéristique {}", record.group, record.feature_id),
        None => "Groupe : sans groupe d'inspection".into(),
    }).small());
    ui.label(egui::RichText::new(dependency_label(measurement, state)).small());
    if let Some(record) = record.filter(|_| !measurement.is_construction) {
        ui.label(egui::RichText::new(format!("Conformité physique : {}", record.outcome.conformity_status.label())).small());
        visible_suspensions(ui, record);
    } else if !measurement.is_construction {
        ui.label(egui::RichText::new("Aucune preuve courante de contour ; origine et méthode ne valent pas validation.").small());
    }
    egui::CollapsingHeader::new(format!("Preuves et limites · M{}", measurement.id))
        .id_salt(("measurement_evidence", measurement.id)).show(ui, |ui| {
            if measurement.is_construction {
                ui.label("Géométrie auxiliaire de construction : exclue des dimensions et de la conformité.");
            } else if let Some(record) = record { evidence_details(ui, record, state); }
            else { ui.label("Tracé natif sans observation de contour associée. Validation logicielle et conformité physique non établies."); }
        });
}

fn grouped_records(state: &InspectionWorkspace) -> BTreeMap<&str, Vec<(usize, &InspectionRecord)>> {
    let mut groups = BTreeMap::new();
    for (index, record) in state.records.iter().enumerate() {
        groups.entry(record.group.as_str()).or_insert_with(Vec::new).push((index, record));
    }
    groups
}

pub(crate) fn grouped_inspection_results(
    ui: &mut egui::Ui,
    state: &InspectionWorkspace,
    measurements: &[Measurement],
) -> Option<u64> {
    let mut selected = None;
    if state.records.is_empty() { return selected; }
    ui.separator();
    ui.label("Observations par groupe · contrôles logiciels distincts de la conformité physique");
    for (group, records) in grouped_records(state) {
        let previews = records.iter().filter(|(_, record)| record.measurement_id.is_none()).count();
        let observations_label = if records.len() == 1 { "observation" } else { "observations" };
        let previews_label = if previews == 1 { "aperçu" } else { "aperçus" };
        egui::CollapsingHeader::new(format!("Groupe : {group} · {} {observations_label} · {previews} {previews_label}", records.len()))
            .id_salt(("inspection_evidence_group", group)).default_open(true).show(ui, |ui| {
                for (index, record) in records {
                    ui.push_id(("inspection_evidence_record", index), |ui| {
                        let role = record.measurement_id.and_then(|id| measurements.iter().find(|measurement|
                            measurement.id == id && measurement.capture_id == record.capture_id))
                            .filter(|measurement| measurement.is_construction)
                            .map(|_| EvidenceRole::Construction).unwrap_or_else(|| record_role(record));
                        ui.label(format!("{} · {}", record.feature_id, role.label()));
                        ui.label(if role == EvidenceRole::Construction {
                            "Construction : aucune valeur de cote".into()
                        } else if suspended_value(record) {
                            "Valeur suspendue · nouvelle observation requise".into()
                        } else { match record.value_mm {
                            Some(value) => format!("Valeur observée : {value:.4} mm"),
                            None => "Valeur dimensionnelle non établie".into(),
                        }});
                        if record.measurement_id.is_none() { ui.label("Aperçu : aucune mesure native enregistrée."); }
                        ui.label(format!("Conformité physique : {}", record.outcome.conformity_status.label()));
                        visible_suspensions(ui, record);
                        if let Some(id) = record.measurement_id {
                            if let Some(measurement) = measurements.iter().find(|measurement|
                                measurement.id == id && measurement.capture_id == record.capture_id) {
                                ui.label(egui::RichText::new(dependency_label(measurement, state)).small());
                                if ui.button(format!("Sélectionner M{id} pour correction")).clicked() { selected = Some(id); }
                            } else { ui.label(format!("Mesure M{id} absente : observation historique à réévaluer.")); }
                        }
                        egui::CollapsingHeader::new("Preuves, seuils et limites")
                            .id_salt(("inspection_record_details", index)).show(ui, |ui| evidence_details(ui, record, state));
                        ui.separator();
                    });
                }
            });
    }
    selected
}

#[cfg(test)]
#[path = "inspection_evidence_ui_tests.rs"]
mod tests;

#[cfg(test)]
mod numeric_resize_tests {
    use super::*;

    #[test]
    fn imposed_geometry_has_proposed_role_and_explicit_display_after_reload() {
        let measurement: Measurement = serde_json::from_value(serde_json::json!({
            "id": 1, "kind": "Distance", "label": "Cote imposée",
            "points": [{"x": 0.0, "y": 0.0}, {"x": 10.0, "y": 0.0}],
            "origin": "pattern", "method": "Copie. [dper:numeric_resize:v1]",
        })).unwrap();
        let state = InspectionWorkspace::default();
        assert_eq!(measurement_role(&measurement, &state), EvidenceRole::Proposed);
        assert!(measurement_display_value(&measurement, &state).starts_with("Géométrie imposée · "));
    }
}
