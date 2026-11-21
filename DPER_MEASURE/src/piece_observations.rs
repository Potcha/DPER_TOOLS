use crate::storage::Session;
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Default)]
pub(crate) struct PieceObservations {
    groups: BTreeMap<PiecePlan, BTreeMap<String, Vec<ViewObservation>>>,
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct PiecePlan {
    lot: String,
    part: String,
    requirements: String,
    drawing: Option<String>,
    fingerprint: String,
}

#[derive(Debug, PartialEq)]
struct ViewObservation {
    capture: String,
    view: String,
    support: Option<String>,
    profile: Option<String>,
    optics: String,
    value: Option<f64>,
    uncertainty: Option<f64>,
    path: PathBuf,
}

impl PieceObservations {
    pub(crate) fn collect(&mut self, session: &Session, path: &std::path::Path) {
        let Some((identity, workspace)) = session.part_identity.as_ref().zip(session.automated_inspection.as_ref()) else { return; };
        let Some(plan) = workspace.plan.as_ref() else { return; };
        let Some(fingerprint) = plan.source_fingerprint.as_ref() else { return; };
        let observations = self.groups.entry(PiecePlan {
            lot: identity.lot_id.clone(), part: identity.part_id.clone(),
            requirements: identity.requirements_revision.clone(),
            drawing: identity.drawing_revision.clone(), fingerprint: fingerprint.clone(),
        }).or_default();

        for record in &workspace.records {
            let Some((id, proof)) = record.measurement_id.zip(record.occurrence.as_ref()) else { continue; };
            if record.capture_id != session.capture_id || record.withdrawal.is_some() { continue; }
            let assignment = workspace.occurrence_assignments.iter().find(|assignment|
                assignment.document_fingerprint == *fingerprint && assignment.feature_id == record.feature_id
                    && assignment.members.contains(&proof.member));
            let support = assignment.filter(|assignment| assignment.view_id == proof.view_id
                || assignment.representations.iter().any(|detail| detail.source.view_id == proof.view_id
                    && detail.source.occurrence_number == proof.member.number))
                .map(|_| proof.member.geometry_id.clone());
            let source = session.measurements.iter().find(|measurement| measurement.id == id
                && measurement.capture_id == session.capture_id && !measurement.is_construction);
            let value = record.value_mm.filter(|value| value.is_finite() && *value > 0.0);
            let profile_matches = session.calibration.as_ref().is_some_and(|profile|
                record.calibration_id.as_deref() == Some(profile.id.as_str())
                    && profile.device_id == session.device_id && profile.optical_setup == session.optical_setup
                    && record.calibration_fingerprint
                        == crate::dper_inspection::calibration_fingerprint(Some(profile)));
            let established = support.is_some() && profile_matches && source.is_some_and(|measurement|
                measurement.points == record.measurement_points && !measurement.is_numeric_resize());
            let candidate = ViewObservation {
                capture: session.capture_id.clone(), view: proof.view_id.clone(), support,
                profile: record.calibration_id.clone(), optics: session.optical_setup.clone(),
                value: value.filter(|_| established),
                uncertainty: record.decision_evidence.as_ref()
                    .filter(|evidence| evidence.measurement_id == id
                        && evidence.capture_id == record.capture_id
                        && Some(evidence.measurement_revision) == record.measurement_revision
                        && evidence.source_fingerprint == record.source_fingerprint
                        && Some(evidence.calibration_fingerprint) == record.calibration_fingerprint)
                    .map(|evidence| evidence.basis.expanded_uncertainty_mm)
                    .filter(|value| value.is_finite() && *value > 0.0),
                path: path.to_owned(),
            };
            let views = observations.entry(proof.member.id.clone()).or_default();
            if !views.iter().any(|existing| existing.capture == candidate.capture
                && existing.view == candidate.view && existing.support == candidate.support
                && existing.profile == candidate.profile && existing.optics == candidate.optics
                && existing.value == candidate.value && existing.uncertainty == candidate.uncertainty)
            {
                views.push(candidate);
            }
        }
    }

    pub(crate) fn show(&self, ui: &mut egui::Ui) {
        for (plan, occurrences) in &self.groups {
            if occurrences.is_empty() { continue; }
            let heading = format!("Lot {} · pièce {} · exigences {} · plan {} · {} occurrence(s)",
                plan.lot, plan.part, plan.requirements, plan.drawing.as_deref().unwrap_or(&plan.fingerprint),
                occurrences.len());
            ui.collapsing(heading, |ui| {
                for (id, views) in occurrences {
                    ui.collapsing(format!("{id} · {}", observation_status(views)), |ui| {
                        for candidate in views {
                            let value = candidate.value.map(|value| format!("{value:.4} mm"))
                                .unwrap_or_else(|| "mesure inexploitable ou association à revoir".into());
                            ui.label(format!("Capture {} · vue {} · {value}", candidate.capture, candidate.view));
                            if let Some(profile) = &candidate.profile {
                                ui.weak(format!("Étalonnage {profile} · {}", candidate.optics));
                            }
                            ui.weak(candidate.path.display().to_string());
                        }
                    });
                }
            });
        }
    }
}

fn observation_status(views: &[ViewObservation]) -> &'static str {
    let conflict = views.iter().enumerate().any(|(index, left)| views[index + 1..].iter().any(|right| {
        if left.support.is_some() && right.support.is_some() && left.support != right.support { return true; }
        let interval = |candidate: &ViewObservation| {
            let (value, uncertainty) = candidate.value.zip(candidate.uncertainty)?;
            Some((value - uncertainty, value + uncertainty))
        };
        interval(left).zip(interval(right)).is_some_and(|((a, b), (c, d))| b < c || d < a)
    }));
    if conflict { "Conflit à revoir" }
    else if views.iter().any(|candidate| candidate.value.is_none()) { "Association ou étalonnage à revoir" }
    else { "Observations enregistrées à revoir" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llt_aut_007_02_counts_an_occurrence_once_and_keeps_view_conflicts() {
        let observation = |capture: &str, view: &str, value| ViewObservation {
            capture: capture.into(), view: view.into(), support: Some("circle-1".into()),
            profile: Some("profil-1".into()), optics: "objectif 1".into(),
            value: Some(value), uncertainty: Some(0.05), path: format!("{capture}.dperproj").into(),
        };
        let overview = observation("CAP-A", "ensemble", 4.01);
        let detail = observation("CAP-B", "détail", 4.04);
        assert_eq!(observation_status(&[overview, detail]), "Observations enregistrées à revoir");
        let overview = observation("CAP-A", "ensemble", 4.01);
        let conflicting = observation("CAP-B", "détail", 4.40);
        assert_eq!(observation_status(&[overview, conflicting]), "Conflit à revoir");

        let mut source = crate::report::test_support::historical_vision_photo().session;
        let ws = source.automated_inspection.as_mut().unwrap();
        let record = &mut ws.records[0];
        let member = crate::dper_binding::OccurrenceMember {
            id: "repetition:diameter:1".into(), number: 1, geometry_id: "circle-1".into(),
            candidate_id: "candidate-1".into(), candidate_fingerprint: "candidate-proof".into(),
        };
        record.capture_id.clone_from(&source.capture_id);
        record.occurrence = Some(crate::dper_inspection::OccurrenceProof {
            member: member.clone(), assignment_fingerprint: "evidence".into(),
            view_id: "ensemble".into(), preview_revision: 1, source_measurement_ids: Vec::new(),
        });
        let mut plan = crate::dper_drawing::parse_text_plan("Ø4 ±0.1 mm", "plan.pdf");
        plan.source_fingerprint = Some("plan-fingerprint".into());
        ws.plan = Some(plan);
        source.part_identity = Some(crate::part_identity::PartIdentity::new("LOT-A", "P1", "R1", "D1").unwrap());
        let mut detail = source.clone();
        detail.capture_id = "detail-capture".into();
        let record = &mut detail.automated_inspection.as_mut().unwrap().records[0];
        record.capture_id.clone_from(&detail.capture_id);
        record.occurrence.as_mut().unwrap().view_id = "détail".into();
        let mut other = source.clone();
        other.part_identity = Some(crate::part_identity::PartIdentity::new("LOT-B", "P1", "R1", "D1").unwrap());
        let mut consolidated = PieceObservations::default();
        for (session, file) in [(&source, "overview"), (&detail, "detail"), (&source, "copy"), (&other, "other")] {
            consolidated.collect(session, std::path::Path::new(file));
        }
        assert_eq!(consolidated.groups.len(), 2);
        let views = &consolidated.groups[&PiecePlan {
            lot: "LOT-A".into(), part: "P1".into(), requirements: "R1".into(),
            drawing: Some("D1".into()), fingerprint: "plan-fingerprint".into(),
        }];
        assert_eq!(views.len(), 1);
        assert_eq!(views[&member.id].len(), 2);
        assert_eq!(observation_status(&views[&member.id]), "Association ou étalonnage à revoir");
    }
}
