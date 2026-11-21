use crate::storage::Session;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, io::Write as _, path::{Path, PathBuf}};

#[derive(Default)]
pub(crate) struct PieceObservations {
    groups: BTreeMap<PiecePlan, BTreeMap<String, Vec<ViewObservation>>>,
    reviews: BTreeMap<ReviewScope, RetainedObservation>,
    draft: Option<RetainedObservation>,
    pub(crate) review_ready: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
struct PiecePlan {
    lot: String,
    part: String,
    requirements: String,
    drawing: Option<String>,
    fingerprint: String,
}

#[derive(Debug, PartialEq, Serialize)]
struct ViewObservation {
    capture: String,
    view: String,
    support: Option<String>,
    profile: Option<String>,
    optics: String,
    value: Option<f64>,
    uncertainty: Option<f64>,
    path: PathBuf,
    measurement_id: u64,
    revision: Option<u64>,
    image_fingerprint: u64,
    profile_fingerprint: Option<u64>,
    association_fingerprint: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
struct ReviewScope {
    plan: PiecePlan,
    occurrence: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct RetainedObservation {
    scope: ReviewScope,
    candidate: String,
    evidence: String,
    justification: String,
}

impl PieceObservations {
    pub(crate) fn collect(&mut self, session: &Session, path: &Path) {
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
            let support = assignment.filter(|assignment| {
                let mut scoped = assignment.primary_assignment();
                if assignment.view_id != proof.view_id {
                    scoped.representations = assignment.representations.iter()
                        .filter(|detail| detail.source.view_id == proof.view_id).cloned().collect();
                    if scoped.representations.iter()
                        .filter(|detail| detail.source.occurrence_number == proof.member.number)
                        .count() != 1
                    {
                        return false;
                    }
                }
                serde_json::to_vec(&scoped).is_ok_and(|bytes|
                    crate::dper_binding::fingerprint_bytes(&bytes) == proof.assignment_fingerprint)
            }).map(|_| proof.member.geometry_id.clone());
            let source = session.measurements.iter().find(|measurement| measurement.id == id
                && measurement.capture_id == session.capture_id && !measurement.is_construction);
            let value = record.value_mm.filter(|value| value.is_finite() && *value > 0.0);
            let profile_matches = session.calibration.as_ref().is_some_and(|profile|
                record.calibration_id.as_deref() == Some(profile.id.as_str())
                    && !session.device_id.trim().is_empty() && !session.optical_setup.trim().is_empty()
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
                measurement_id: id,
                revision: record.measurement_revision,
                image_fingerprint: record.source_fingerprint,
                profile_fingerprint: record.calibration_fingerprint,
                association_fingerprint: proof.assignment_fingerprint.clone(),
            };
            let views = observations.entry(proof.member.id.clone()).or_default();
            if !views.iter().any(|existing| existing.capture == candidate.capture
                && existing.view == candidate.view && existing.support == candidate.support
                && existing.profile == candidate.profile && existing.optics == candidate.optics
                && existing.measurement_id == candidate.measurement_id && existing.revision == candidate.revision
                && existing.image_fingerprint == candidate.image_fingerprint
                && existing.profile_fingerprint == candidate.profile_fingerprint
                && existing.association_fingerprint == candidate.association_fingerprint
                && existing.value == candidate.value && existing.uncertainty == candidate.uncertainty)
            {
                views.push(candidate);
            }
        }
    }

    pub(crate) fn load_reviews(&mut self, root: &Path) {
        for (plan, occurrences) in &self.groups {
            for (occurrence, candidates) in occurrences {
                let scope = ReviewScope { plan: plan.clone(), occurrence: occurrence.clone() };
                let Ok(path) = review_path(root, &scope) else { continue; };
                let Ok(bytes) = fs::read(path) else { continue; };
                let Ok(mut choice) = serde_json::from_slice::<RetainedObservation>(&bytes) else { continue; };
                if choice.scope == scope {
                    let current = candidates.iter().find(|candidate| candidate.value.is_some()
                        && observation_fingerprint(candidate).is_ok_and(|id| id == choice.candidate));
                    if !current.is_some_and(|candidate| verify_observed_sources(&scope, candidate).is_ok()) {
                        choice.evidence.clear();
                    }
                    self.reviews.insert(scope, choice);
                }
            }
        }
    }

    pub(crate) fn retain(&mut self, root: &Path, choice: RetainedObservation) -> Result<(), String> {
        if !self.review_ready {
            return Err("Relancez une recherche complète avec les identifiants du lot et de la pièce.".into());
        }
        if choice.justification.trim().is_empty() {
            return Err("Indiquez pourquoi cette observation est retenue.".into());
        }
        let observations = self.groups.get(&choice.scope.plan)
            .and_then(|occurrences| occurrences.get(&choice.scope.occurrence))
            .ok_or("L’occurrence a changé depuis sa revue.")?;
        if evidence_fingerprint(observations)? != choice.evidence {
            return Err("Les observations ont changé. Reprenez leur revue.".into());
        }
        let observed = observations.iter()
            .find(|candidate| observation_fingerprint(candidate).is_ok_and(|id| id == choice.candidate))
            .filter(|candidate| candidate.value.is_some())
            .ok_or("L’observation choisie ne dispose plus d’une mesure exploitable.")?;
        verify_observed_sources(&choice.scope, observed)?;
        let path = review_path(root, &choice.scope)?;
        let parent = path.parent().unwrap();
        fs::create_dir_all(parent).map_err(|error| format!("Dossier de revue indisponible : {error}"))?;
        let mut staged = tempfile::NamedTempFile::new_in(parent)
            .map_err(|error| format!("Préparation de la revue : {error}"))?;
        let bytes = serde_json::to_vec_pretty(&choice).map_err(|error| error.to_string())?;
        staged.write_all(&bytes).and_then(|_| staged.as_file().sync_all())
            .map_err(|error| format!("Écriture de la revue : {error}"))?;
        staged.persist(path).map_err(|error| format!("Publication de la revue : {}", error.error))?;
        self.reviews.insert(choice.scope.clone(), choice);
        Ok(())
    }

    pub(crate) fn show(&mut self, ui: &mut egui::Ui, can_retain: bool) -> Option<RetainedObservation> {
        let mut submit = None;
        for (plan, occurrences) in &self.groups {
            if occurrences.is_empty() { continue; }
            let heading = format!("Lot {} · pièce {} · exigences {} · plan {} · {} occurrence(s)",
                plan.lot, plan.part, plan.requirements, plan.drawing.as_deref().unwrap_or(&plan.fingerprint),
                occurrences.len());
            egui::CollapsingHeader::new(heading).id_salt(&plan.fingerprint).show(ui, |ui| {
                for (id, views) in occurrences {
                    let scope = ReviewScope { plan: plan.clone(), occurrence: id.clone() };
                    let evidence = evidence_fingerprint(views).ok();
                    let review = self.reviews.get(&scope);
                    let current = review.filter(|choice| evidence.as_ref() == Some(&choice.evidence));
                    ui.collapsing(format!("{id} · {}", observation_status(views)), |ui| {
                        if review.is_some() && current.is_none() {
                            ui.label("Les observations ou leurs sources ont changé. Le choix précédent doit être revu.");
                        }
                        for candidate in views {
                            let fingerprint = observation_fingerprint(candidate).ok();
                            let value = candidate.value.map(|value| format!("{value:.4} mm"))
                                .unwrap_or_else(|| "mesure inexploitable ou association à revoir".into());
                            ui.label(format!("Capture {} · vue {} · {value}", candidate.capture, candidate.view));
                            if let Some(profile) = &candidate.profile {
                                ui.weak(format!("Étalonnage {profile} · {}", candidate.optics));
                            }
                            ui.weak(candidate.path.display().to_string());
                            if let Some(choice) = current
                                .filter(|choice| fingerprint.as_ref() == Some(&choice.candidate))
                            {
                                ui.label(format!("Observation retenue · {}", choice.justification));
                            } else if self.draft.as_ref().is_some_and(|draft|
                                draft.scope == scope && fingerprint.as_ref() == Some(&draft.candidate))
                            {
                                ui.label("Observation en cours de revue");
                            } else if can_retain && candidate.value.is_some() {
                                if let Some((evidence, fingerprint)) = evidence.as_ref().zip(fingerprint) {
                                    if ui.button("Retenir cette observation…").clicked() {
                                        self.draft = Some(RetainedObservation {
                                            scope: scope.clone(), candidate: fingerprint, evidence: evidence.clone(),
                                            justification: String::new(),
                                        });
                                    }
                                }
                            }
                        }
                        if let Some(draft) = self.draft.as_mut().filter(|draft| draft.scope == scope) {
                            ui.label("Justification du choix");
                            ui.add(egui::TextEdit::multiline(&mut draft.justification).char_limit(1000));
                            if ui.add_enabled(can_retain && !draft.justification.trim().is_empty(),
                                egui::Button::new("Enregistrer le choix")).clicked()
                            {
                                submit = Some(draft.clone());
                            }
                        }
                    });
                }
            });
        }
        submit
    }
}

fn observation_fingerprint(candidate: &ViewObservation) -> Result<String, String> {
    let bytes = serde_json::to_vec(candidate).map_err(|error| error.to_string())?;
    Ok(crate::dper_source::fingerprint_bytes(&bytes))
}

fn evidence_fingerprint(observations: &[ViewObservation]) -> Result<String, String> {
    let mut ids = observations.iter().map(observation_fingerprint).collect::<Result<Vec<_>, _>>()?;
    ids.sort();
    Ok(crate::dper_source::fingerprint_bytes(ids.join("\n").as_bytes()))
}

fn review_path(root: &Path, scope: &ReviewScope) -> Result<PathBuf, String> {
    let key = serde_json::to_vec(scope).map_err(|error| error.to_string())?;
    let hash = crate::dper_source::fingerprint_bytes(&key);
    Ok(root.join(".dper-measure-observation-reviews").join(format!("{}.json", &hash[8..])))
}

fn verify_observed_sources(scope: &ReviewScope, observed: &ViewObservation) -> Result<(), String> {
    let bytes = fs::read(&observed.path).map_err(|error| format!("Projet source inaccessible : {error}"))?;
    let session: Session = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Projet source illisible : {error}"))?;
    let mut current = PieceObservations::default();
    current.collect(&session, &observed.path);
    if !current.groups.get(&scope.plan).and_then(|groups| groups.get(&scope.occurrence))
        .is_some_and(|items| items.contains(observed))
    {
        return Err("Les preuves de l’observation ont changé dans le projet.".into());
    }
    let folder = observed.path.parent().ok_or("Dossier du projet source indisponible.")?;
    let image_path = crate::reference::resolve_source(&session.image_file, folder);
    let image = image::open(image_path).map_err(|error| format!("Capture source inaccessible : {error}"))?.into_rgb8();
    session.verify_source_pixels(image.width(), image.height(), image.as_raw())?;
    if crate::dper_inspection::fingerprint(&image) != observed.image_fingerprint {
        return Err("La capture ne correspond plus à l’observation.".into());
    }
    if !session.calibration.as_ref().is_some_and(|profile| profile.resolution == [image.width(), image.height()]) {
        return Err("La résolution de la capture ne correspond plus au profil.".into());
    }
    let plan_path = session.automated_inspection.as_ref().and_then(|ws| ws.plan.as_ref())
        .and_then(|plan| plan.source_file.as_deref()).ok_or("Plan source absent du projet.")?;
    let path = crate::reference::resolve_source(plan_path, folder);
    if crate::dper_source::fingerprint_file(&path)? != scope.plan.fingerprint {
        return Err("Le plan source a changé depuis l’observation.".into());
    }
    Ok(())
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
    fn hlt_aut_007_consolidates_an_overview_and_detail_and_invalidates_changed_sources() {
        use crate::dper_binding::{OccurrenceAssignment, OccurrenceMember, OccurrenceRepresentation, OccurrenceRepresentationInput};
        use crate::dper_inspection::{calibration_fingerprint, fingerprint_pixels, OccurrenceProof};

        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let mut photo = crate::report::test_support::historical_vision_photo();
        let session = &mut photo.session;
        let image_hash = fingerprint_pixels(photo.width, photo.height, &photo.image_rgb);
        session.part_identity = Some(crate::part_identity::PartIdentity::new("L1", "P1", "R1", "D1").unwrap());
        session.image_file = "overall.png".into();
        session.source_image_fingerprint = Some(format!("{image_hash:016x}"));
        session.device_id = "camera-fixture".into();
        session.optical_setup = "objectif A".into();
        session.calibration = Some(serde_json::from_value(serde_json::json!({
            "id": "independent-calibration", "device": "fixture", "device_id": session.device_id,
            "optical_setup": session.optical_setup, "resolution": [photo.width, photo.height],
            "mm_per_pixel": 0.1, "reference_mm": 10.0, "reference_px": 100.0,
        })).unwrap());
        let profile = session.calibration.as_ref().unwrap();
        image::save_buffer(root.join(&session.image_file), &photo.image_rgb, photo.width, photo.height, image::ColorType::Rgb8).unwrap();
        fs::write(root.join("plan.pdf"), include_bytes!("../tests/fixtures/drawing_geometry/01_circle_leader.pdf")).unwrap();
        let plan_hash = crate::dper_source::fingerprint_file(&root.join("plan.pdf")).unwrap();
        let member = OccurrenceMember {
            id: "occurrence-1".into(), number: 1, geometry_id: "circle-1".into(),
            candidate_id: "candidate-1".into(), candidate_fingerprint: "circle-proof".into(),
            arc_source_ids: Vec::new(),
        };
        let workspace = session.automated_inspection.as_mut().unwrap();
        let mut plan = crate::dper_drawing::parse_text_plan("Ø4 mm", "plan.pdf");
        plan.source_fingerprint = Some(plan_hash.clone());
        plan.source_file = Some("plan.pdf".into());
        workspace.plan = Some(plan);
        let record = &mut workspace.records[0];
        record.source_fingerprint = image_hash;
        record.calibration_id = Some(profile.id.clone());
        record.calibration_fingerprint = calibration_fingerprint(Some(profile));
        record.measurement_revision = Some(1);
        let assignment = OccurrenceAssignment {
            feature_id: record.feature_id.clone(), view_id: "ensemble".into(), page: 1,
            document_fingerprint: plan_hash, justification: "Support confirmé".into(),
            members: vec![member.clone()], representations: Vec::new(), selected_in_session: false,
        };
        record.occurrence = Some(OccurrenceProof {
            member: member.clone(), view_id: assignment.view_id.clone(), preview_revision: 1,
            assignment_fingerprint: crate::dper_binding::fingerprint_bytes(&serde_json::to_vec(&assignment).unwrap()),
            source_measurement_ids: Vec::new(),
        });
        workspace.occurrence_assignments.push(assignment);
        let project = root.join("overall.dperproj");
        fs::write(&project, serde_json::to_vec(session).unwrap()).unwrap();
        let mut detail = session.clone();
        detail.capture_id = "macro-P1".into();
        detail.image_file = "macro.png".into();
        detail.optical_setup = "objectif B".into();
        let detail_pixels = vec![145; photo.image_rgb.len()];
        let detail_hash = fingerprint_pixels(photo.width, photo.height, &detail_pixels);
        detail.source_image_fingerprint = Some(format!("{detail_hash:016x}"));
        image::save_buffer(root.join(&detail.image_file), &detail_pixels, photo.width, photo.height, image::ColorType::Rgb8).unwrap();
        let detail_profile = detail.calibration.as_mut().unwrap();
        detail_profile.id = "calibration-macro".into();
        detail_profile.optical_setup.clone_from(&detail.optical_setup);
        detail_profile.mm_per_pixel = 0.05;
        detail_profile.reference_px = 200.0;
        detail.measurements[0].capture_id.clone_from(&detail.capture_id);
        let detail_ws = detail.automated_inspection.as_mut().unwrap();
        let assignment = &mut detail_ws.occurrence_assignments[0];
        assignment.representations.push(OccurrenceRepresentation {
            source: OccurrenceRepresentationInput {
                occurrence_number: 1, view_id: "détail".into(), feature_id: "détail-cote".into(),
                candidate_id: "détail-candidat".into(), justification: "Même lamage dans le détail".into(),
            }, page: 1, geometry_id: "détail-lamage".into(), candidate_fingerprint: "détail-preuve".into(),
        });
        let detail_record = &mut detail_ws.records[0];
        detail_record.capture_id.clone_from(&detail.capture_id);
        detail_record.source_fingerprint = detail_hash;
        detail_record.calibration_id = Some(detail_profile.id.clone());
        detail_record.calibration_fingerprint = calibration_fingerprint(Some(detail_profile));
        detail_record.value_mm = Some(1.03);
        let detail_proof = detail_record.occurrence.as_mut().unwrap();
        detail_proof.view_id = "détail".into();
        detail_proof.assignment_fingerprint = crate::dper_binding::fingerprint_bytes(&serde_json::to_vec(&assignment).unwrap());
        fs::write(root.join("macro.dperproj"), serde_json::to_vec(&detail).unwrap()).unwrap();
        let mut incompatible = detail.clone();
        incompatible.capture_id = "macro-unverified".into();
        incompatible.optical_setup = "objectif C".into();
        incompatible.measurements[0].capture_id.clone_from(&incompatible.capture_id);
        incompatible.automated_inspection.as_mut().unwrap().records[0].capture_id.clone_from(&incompatible.capture_id);
        fs::write(root.join("macro-unverified.dperproj"), serde_json::to_vec(&incompatible).unwrap()).unwrap();
        let mut other = detail.clone();
        other.part_identity.as_mut().unwrap().part_id = "P2".into();
        fs::write(root.join("other-part.dperproj"), serde_json::to_vec(&other).unwrap()).unwrap();

        let (_, _, mut observations, _) = crate::scan_project_search(root, "", "L1", "P1").unwrap();
        let (plan, occurrences) = observations.groups.first_key_value().unwrap();
        let candidates = &occurrences["occurrence-1"];
        assert_eq!(occurrences.len(), 1);
        assert_eq!(candidates.len(), 3);
        assert_eq!(candidates.iter().filter(|candidate| candidate.value.is_some()).count(), 2);
        let macro_candidate = candidates.iter().find(|candidate| candidate.capture == detail.capture_id).unwrap();
        assert_eq!(macro_candidate.profile.as_deref(), Some("calibration-macro"));
        assert_eq!(macro_candidate.value, Some(1.03));
        let scope = ReviewScope { plan: plan.clone(), occurrence: "occurrence-1".into() };
        let choice = RetainedObservation {
            scope: scope.clone(), candidate: observation_fingerprint(macro_candidate).unwrap(),
            evidence: evidence_fingerprint(candidates).unwrap(), justification: "Contour complet et profil applicable".into(),
        };
        observations.retain(root, choice.clone()).unwrap();
        let (_, _, reloaded, _) = crate::scan_project_search(root, "", "L1", "P1").unwrap();
        assert_eq!(reloaded.reviews[&scope].evidence, choice.evidence);
        fs::write(root.join("macro.png"), b"replacement").unwrap();
        let (_, _, mut changed, _) = crate::scan_project_search(root, "", "L1", "P1").unwrap();
        assert!(changed.reviews[&scope].evidence.is_empty());
        assert!(changed.retain(root, choice).is_err());
    }

    #[test]
    fn llt_aut_007_02_counts_an_occurrence_once_and_keeps_view_conflicts() {
        let observation = |capture: &str, view: &str, value| ViewObservation {
            capture: capture.into(), view: view.into(), support: Some("circle-1".into()),
            profile: Some("profil-1".into()), optics: "objectif 1".into(),
            value: Some(value), uncertainty: Some(0.05), path: format!("{capture}.dperproj").into(),
            measurement_id: 1, revision: Some(1), image_fingerprint: 1,
            profile_fingerprint: Some(1), association_fingerprint: "source".into(),
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
            arc_source_ids: Vec::new(),
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
