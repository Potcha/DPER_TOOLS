//! Exhaustive, read-only correspondence hypotheses for named points.
//!
//! This does not extract landmarks, verify sources, register an inspection, or
//! calibrate measurements. All permutations (at most 720) use the existing
//! orientation-preserving similarity fitter. Its RMS <= 0.8 * threshold policy
//! remains in force, and every supplied point must additionally be an inlier.

use crate::dper_registration::{register_similarity, Correspondence, RegistrationReport};
use crate::geometry::Point;
use rmcp::schemars;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NamedLandmark {
    pub id: String,
    pub position: Point,
}

#[derive(Clone, Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LandmarkMatchingRequest {
    pub plan_landmarks: Vec<NamedLandmark>,
    pub image_landmarks: Vec<NamedLandmark>,
    pub plan_source: String,
    pub image_source: String,
    pub threshold_px: f64,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LandmarkMatchingState {
    UniqueHypothesis,
    Ambiguous,
    NoMatch,
}

#[derive(Clone, Debug, Serialize)]
pub struct LandmarkMapping {
    pub plan_id: String,
    pub image_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub residual_px: Option<f64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct LandmarkHypothesis {
    pub candidate_id: String,
    pub mappings: Vec<LandmarkMapping>,
    pub registration: RegistrationReport,
}

#[derive(Clone, Debug, Serialize)]
pub struct RejectedLandmarkPermutation {
    pub candidate_id: String,
    pub mappings: Vec<LandmarkMapping>,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct LandmarkMatchingReport {
    pub plan_landmarks: Vec<NamedLandmark>,
    pub image_landmarks: Vec<NamedLandmark>,
    pub state: LandmarkMatchingState,
    pub hypotheses: Vec<LandmarkHypothesis>,
    pub rejected_permutations: Vec<RejectedLandmarkPermutation>,
    pub evaluated_permutations: usize,
    pub accepted_count: usize,
    pub rejected_count: usize,
    /// A candidate to review, never an applied or authoritative registration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unique_candidate_id: Option<String>,
    pub plan_source: String,
    pub image_source: String,
    pub threshold_px: f64,
    pub registered: bool,
    pub reviewed: bool,
    pub input_provenance_verified: bool,
    pub limitations: Vec<String>,
}

/// IDs identify evidence only; they do not constrain the exhaustive search.
/// No hypothesis is selected by residual ranking, cached, persisted, or applied.
pub fn match_landmarks(request: &LandmarkMatchingRequest) -> Result<LandmarkMatchingReport, String> {
    let count = request.plan_landmarks.len();
    if !(3..=6).contains(&count) || request.image_landmarks.len() != count {
        return Err("Le plan et l'image doivent contenir le même nombre de repères (3 à 6).".into());
    }
    match_landmark_pool(request)
}

pub(crate) fn validate_landmark_pool_size(plan: usize, image: usize) -> Result<usize, String> {
    if !(3..=6).contains(&plan) || !(plan..=8).contains(&image) {
        return Err("Il faut de 3 à 6 repères PDF et entre ce nombre et 8 repères photo.".into());
    }
    let count = (image - plan + 1..=image).product();
    if count > 720 {
        return Err(format!("Trop d'hypothèses ({count}, limite 720). Réduisez le nombre de repères photo."));
    }
    Ok(count)
}

pub(crate) fn match_landmark_pool(request: &LandmarkMatchingRequest) -> Result<LandmarkMatchingReport, String> {
    let count = request.plan_landmarks.len();
    let evaluated_permutations = validate_landmark_pool_size(count, request.image_landmarks.len())?;
    if !request.threshold_px.is_finite()
        || request.threshold_px <= 0.0
        || request.threshold_px > 1e6
    {
        return Err("Le seuil doit être fini, positif et inférieur ou égal à 1000000 pixels.".into());
    }
    for (name, source) in [("plan", &request.plan_source), ("image", &request.image_source)] {
        if source.trim().is_empty() || source.len() > 4096 {
            return Err(format!("La source {name} doit être renseignée et limitée à 4096 octets."));
        }
    }
    validate_landmarks("plan", &request.plan_landmarks)?;
    validate_landmarks("image", &request.image_landmarks)?;

    let mut permutations = Vec::with_capacity(evaluated_permutations);
    enumerate_permutations(&mut (0..request.image_landmarks.len()).collect::<Vec<_>>(), 0, count, &mut permutations);
    let mut hypotheses = Vec::new();
    let mut rejected_permutations = Vec::new();
    for (index, permutation) in permutations.iter().enumerate() {
        let candidate_id = format!("candidate-{:03}", index + 1);
        let pairs = permutation.iter().enumerate().map(|(plan_index, &image_index)| {
            Correspondence {
                plan: request.plan_landmarks[plan_index].position,
                image: request.image_landmarks[image_index].position,
            }
        }).collect::<Vec<_>>();
        let mut mappings = permutation.iter().enumerate().map(|(plan_index, &image_index)| {
            LandmarkMapping {
                plan_id: request.plan_landmarks[plan_index].id.clone(),
                image_id: request.image_landmarks[image_index].id.clone(),
                residual_px: None,
            }
        }).collect::<Vec<_>>();
        let reason = match register_similarity(&pairs, request.threshold_px) {
            Ok(registration) => {
                for (mapping, &residual) in mappings.iter_mut().zip(&registration.residuals_px) {
                    mapping.residual_px = Some(residual);
                }
                if registration.inlier_indices.len() == count
                    && registration.residuals_px.len() == count
                    && registration.residuals_px.iter().all(|&residual| {
                        residual.is_finite() && residual <= request.threshold_px
                    })
                {
                    hypotheses.push(LandmarkHypothesis { candidate_id, mappings, registration });
                    continue;
                }
                "Hypothèse refusée : tous les couples de l'hypothèse doivent respecter le seuil, sans exclusion de point.".to_string()
            }
            Err(reason) => reason,
        };
        rejected_permutations.push(RejectedLandmarkPermutation { candidate_id, mappings, reason });
    }
    let accepted_count = hypotheses.len();
    let state = match accepted_count {
        0 => LandmarkMatchingState::NoMatch,
        1 => LandmarkMatchingState::UniqueHypothesis,
        _ => LandmarkMatchingState::Ambiguous,
    };
    let unique_candidate_id = if accepted_count == 1 {
        Some(hypotheses[0].candidate_id.clone())
    } else {
        None
    };
    Ok(LandmarkMatchingReport {
        plan_landmarks: request.plan_landmarks.clone(),
        image_landmarks: request.image_landmarks.clone(),
        state,
        accepted_count,
        rejected_count: rejected_permutations.len(),
        evaluated_permutations,
        unique_candidate_id,
        hypotheses,
        rejected_permutations,
        plan_source: request.plan_source.clone(),
        image_source: request.image_source.clone(),
        threshold_px: request.threshold_px,
        registered: false,
        reviewed: false,
        input_provenance_verified: false,
        limitations: vec![
            "Coordonnées explicites fournies par l'opérateur, sans extraction automatique ni vérification de leur provenance.".into(),
            "Les sources sont des libellés non vérifiés ; leur contenu, leur identité et les unités des coordonnées ne sont pas vérifiés.".into(),
            "Similarité préservant l'orientation uniquement : aucune réflexion, perspective ou déformation anisotrope.".into(),
            "Toutes les affectations admissibles par le nombre de repères sont évaluées. Toutes les hypothèses acceptées sont conservées sans classement ni choix automatique.".into(),
            "Chaque point doit respecter le seuil ; la politique existante impose aussi une erreur RMS inférieure ou égale à 0,8 fois ce seuil.".into(),
            "Une hypothèse, même unique, n'est ni revue ni appliquée et ne remplace jamais une calibration physique indépendante.".into(),
        ],
    })
}

fn validate_landmarks(name: &str, landmarks: &[NamedLandmark]) -> Result<(), String> {
    let mut ids = HashSet::new();
    for (index, landmark) in landmarks.iter().enumerate() {
        let id = landmark.id.trim();
        if id.is_empty() || landmark.id.len() > 128 || !ids.insert(id) {
            return Err(format!("Les identifiants {name} doivent être non vides, uniques après suppression des espaces périphériques et limités à 128 octets."));
        }
        let point = landmark.position;
        if !point.x.is_finite() || !point.y.is_finite()
            || point.x.abs() > 1e12 || point.y.abs() > 1e12
        {
            return Err(format!("Le repère {name} {id} contient une coordonnée invalide ou excessive."));
        }
        if landmarks[..index].iter().any(|previous| previous.position.dist(point) <= 1e-9) {
            return Err(format!("Les repères {name} doivent être distincts (distance supérieure à 1e-9)."));
        }
    }
    Ok(())
}

fn enumerate_permutations(items: &mut [usize], start: usize, count: usize, result: &mut Vec<Vec<usize>>) {
    if start == count {
        result.push(items[..count].to_vec());
        return;
    }
    for index in start..items.len() {
        items.swap(start, index);
        enumerate_permutations(items, start + 1, count, result);
        items.swap(start, index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dper_registration::SimilarityTransform;

    const POINTS: [Point; 6] = [
        Point { x: 0.0, y: 0.0 }, Point { x: 40.0, y: 0.0 },
        Point { x: 3.0, y: 27.0 }, Point { x: -19.0, y: 11.0 },
        Point { x: 26.0, y: -23.0 }, Point { x: 10.0, y: 3.0 },
    ];

    fn request(points: &[Point]) -> LandmarkMatchingRequest {
        let landmarks = || points.iter().enumerate().map(|(index, &position)| NamedLandmark {
            id: format!("p{index}"), position,
        }).collect();
        LandmarkMatchingRequest {
            plan_landmarks: landmarks(), image_landmarks: landmarks(),
            plan_source: "operator-plan".into(), image_source: "operator-image".into(),
            threshold_px: 0.01,
        }
    }

    #[test]
    fn asymmetric_triangle_recovers_rotation_scale_and_permuted_ids() {
        let mut input = request(&POINTS[..3]);
        let transform = SimilarityTransform { scale: 2.7, rotation_rad: 0.63, translation: Point { x: 90.0, y: -31.0 } };
        for landmark in &mut input.image_landmarks { landmark.position = transform.apply(landmark.position); }
        input.image_landmarks.swap(0, 2);
        let report = match_landmarks(&input).unwrap();
        assert_eq!(report.state, LandmarkMatchingState::UniqueHypothesis);
        assert_eq!(report.accepted_count, 1);
        for mapping in &report.hypotheses[0].mappings {
            assert_eq!(mapping.plan_id, mapping.image_id);
            assert!(mapping.residual_px.unwrap() < 1e-9);
        }
        assert!(report.hypotheses[0].registration.transform.apply(POINTS[1]).dist(transform.apply(POINTS[1])) < 1e-9);
    }

    #[test]
    fn square_preserves_four_ambiguous_rotations_without_selection() {
        let input = request(&[Point { x: 0.0, y: 0.0 }, Point { x: 10.0, y: 0.0 }, Point { x: 10.0, y: 10.0 }, Point { x: 0.0, y: 10.0 }]);
        let report = match_landmarks(&input).unwrap();
        assert_eq!(report.state, LandmarkMatchingState::Ambiguous);
        assert_eq!(report.accepted_count, 4);
        assert!(report.unique_candidate_id.is_none());
        assert_eq!(report.rejected_count, 20);
    }

    #[test]
    fn equilateral_and_collinear_symmetries_remain_ambiguous() {
        let equilateral = [Point { x: 0.0, y: 0.0 }, Point { x: 10.0, y: 0.0 }, Point { x: 5.0, y: 5.0 * 3.0_f64.sqrt() }];
        let line = [Point { x: -10.0, y: 0.0 }, Point { x: 0.0, y: 0.0 }, Point { x: 10.0, y: 0.0 }];
        for (points, expected) in [(&equilateral, 3), (&line, 2)] {
            let report = match_landmarks(&request(points)).unwrap();
            assert_eq!(report.state, LandmarkMatchingState::Ambiguous);
            assert_eq!(report.accepted_count, expected);
        }
    }

    #[test]
    fn incompatible_and_reflected_asymmetric_sets_have_no_match() {
        for reflected in [false, true] {
            let mut input = request(&POINTS[..3]);
            if reflected {
                for landmark in &mut input.image_landmarks { landmark.position.x = -landmark.position.x; }
            } else { input.image_landmarks[2].position.y += 9.0; }
            let report = match_landmarks(&input).unwrap();
            assert_eq!(report.state, LandmarkMatchingState::NoMatch);
            assert_eq!(report.rejected_count, 6);
            assert!(report.unique_candidate_id.is_none());
            assert!(report.rejected_permutations.iter().all(|item| !item.reason.is_empty() && item.mappings.len() == 3));
        }
    }

    #[test]
    fn noisy_sets_preserve_full_residual_evidence_and_all_viable_alternatives() {
        let mut input = request(&POINTS);
        input.threshold_px = 0.2;
        for (index, landmark) in input.image_landmarks.iter_mut().enumerate() {
            landmark.position.x += (index as f64 * 1.7).sin() * 0.02;
            landmark.position.y += (index as f64 * 2.1).cos() * 0.02;
        }
        let report = match_landmarks(&input).unwrap();
        assert_eq!(report.state, LandmarkMatchingState::UniqueHypothesis);
        let fit = &report.hypotheses[0].registration;
        assert_eq!(fit.inlier_indices.len(), 6);
        assert_eq!(fit.residuals_px.len(), 6);
        assert!(fit.rms_error_px < 0.04);
        // A generous tolerance may admit alternatives with different errors.
        input.threshold_px = 1000.0;
        let report = match_landmarks(&input).unwrap();
        assert_eq!(report.accepted_count, 720);
        assert_eq!(report.state, LandmarkMatchingState::Ambiguous);
        assert!(report.unique_candidate_id.is_none());
        assert!(report.hypotheses.iter().any(|item| item.registration.rms_error_px > 1.0));
    }

    #[test]
    fn enumeration_covers_each_permutation_once_up_to_six_factorial() {
        for (count, factorial) in [(3, 6), (4, 24), (5, 120), (6, 720)] {
            let report = match_landmarks(&request(&POINTS[..count])).unwrap();
            assert_eq!(report.evaluated_permutations, factorial);
            assert_eq!(report.accepted_count + report.rejected_count, factorial);
            let ids = report.hypotheses.iter().map(|item| item.candidate_id.as_str())
                .chain(report.rejected_permutations.iter().map(|item| item.candidate_id.as_str())).collect::<HashSet<_>>();
            assert_eq!(ids.len(), factorial);
            let permutations = report.hypotheses.iter().map(|item| &item.mappings)
                .chain(report.rejected_permutations.iter().map(|item| &item.mappings))
                .map(|items| items.iter().map(|item| item.image_id.as_str()).collect::<Vec<_>>()).collect::<HashSet<_>>();
            assert_eq!(permutations.len(), factorial);
        }
    }

    #[test]
    fn photo_pool_checks_every_ordered_subset_and_reports_the_unused_point() {
        let mut input = request(&POINTS[..3]);
        input.image_landmarks.insert(0, NamedLandmark { id: "distractor".into(), position: POINTS[3] });
        assert!(match_landmarks(&input).is_err());
        let report = match_landmark_pool(&input).unwrap();
        assert_eq!(report.state, LandmarkMatchingState::UniqueHypothesis);
        assert_eq!((report.evaluated_permutations, report.accepted_count, report.rejected_count), (24, 1, 23));
        assert_eq!(report.image_landmarks.len(), 4);
        assert!(report.hypotheses[0].mappings.iter().all(|mapping| mapping.plan_id == mapping.image_id));

        let assignments = report.hypotheses.iter().map(|item| &item.mappings)
            .chain(report.rejected_permutations.iter().map(|item| &item.mappings))
            .map(|mappings| mappings.iter().map(|mapping| mapping.image_id.as_str()).collect::<Vec<_>>())
            .collect::<HashSet<_>>();
        assert_eq!(assignments.len(), 24);
        assert!(assignments.iter().all(|ids| ids.len() == 3 && ids.iter().collect::<HashSet<_>>().len() == 3));
    }

    #[test]
    fn photo_pool_refuses_incomplete_inputs_and_excessive_assignments() {
        assert_eq!(validate_landmark_pool_size(3, 8).unwrap(), 336);
        assert_eq!(validate_landmark_pool_size(5, 6).unwrap(), 720);
        for (plan, image) in [(2, 4), (4, 3), (3, 9), (4, 7), (6, 7)] {
            assert!(validate_landmark_pool_size(plan, image).is_err(), "{plan}/{image}");
        }
    }

    #[test]
    fn invalid_counts_ids_coordinates_and_thresholds_are_refused() {
        let valid = request(&POINTS[..3]);
        for count in [0, 2, 7] {
            let mut input = valid.clone();
            input.plan_landmarks.resize(count, input.plan_landmarks[0].clone());
            input.image_landmarks.resize(count, input.image_landmarks[0].clone());
            assert!(match_landmarks(&input).is_err());
        }
        let mut input = valid.clone(); input.image_landmarks.pop();
        assert!(match_landmarks(&input).is_err());
        for threshold in [0.0, -1.0, f64::NAN, f64::INFINITY, 1e6 + 1.0] {
            let mut input = valid.clone(); input.threshold_px = threshold;
            assert!(match_landmarks(&input).is_err());
        }
        for value in [f64::NAN, f64::INFINITY, -f64::INFINITY, 1e12 + 1.0] {
            for image in [false, true] {
                let mut input = valid.clone();
                let points = if image { &mut input.image_landmarks } else { &mut input.plan_landmarks };
                points[0].position.y = value;
                assert!(match_landmarks(&input).is_err());
            }
        }
        for image in [false, true] {
            for id in [" ".to_string(), "p0 ".to_string(), "x".repeat(129)] {
                let mut input = valid.clone();
                let points = if image { &mut input.image_landmarks } else { &mut input.plan_landmarks };
                points[1].id = id;
                assert!(match_landmarks(&input).is_err());
            }
            let mut input = valid.clone();
            let points = if image { &mut input.image_landmarks } else { &mut input.plan_landmarks };
            points[1].position = Point { x: 5e-10, y: 0.0 };
            assert!(match_landmarks(&input).is_err());
        }
        assert!(match_landmarks(&valid).is_ok()); // IDs may repeat across sets.
    }

    #[test]
    fn near_threshold_point_cannot_be_silently_discarded_by_ransac() {
        let mut input = request(&POINTS);
        input.threshold_px = 0.1;
        input.image_landmarks[5].position.y += 0.15;
        let pairs = input.plan_landmarks.iter().zip(&input.image_landmarks).map(|(plan, image)| Correspondence { plan: plan.position, image: image.position }).collect::<Vec<_>>();
        let partial = register_similarity(&pairs, input.threshold_px).unwrap();
        assert!(partial.inlier_indices.len() < 6);
        let report = match_landmarks(&input).unwrap();
        let identity = report.rejected_permutations.iter().find(|item| item.candidate_id == "candidate-001").unwrap();
        assert!(identity.reason.contains("sans exclusion"));
        assert!(identity.mappings.iter().any(|item| item.residual_px.unwrap() > input.threshold_px));
        assert!(report.hypotheses.iter().all(|item| item.registration.inlier_indices.len() == 6 && item.mappings.iter().all(|mapping| mapping.residual_px.unwrap() <= input.threshold_px)));
    }

    #[test]
    fn sources_false_authority_flags_and_strict_request_schema_are_preserved() {
        let valid = request(&POINTS[..3]);
        let report = match_landmarks(&valid).unwrap();
        assert_eq!(report.plan_source, valid.plan_source);
        assert_eq!(report.image_source, valid.image_source);
        assert!(!report.registered && !report.reviewed && !report.input_provenance_verified);
        assert_eq!(report.unique_candidate_id.as_ref(), Some(&report.hypotheses[0].candidate_id));
        assert!(report.limitations.iter().any(|text| text.contains("0,8")));
        for image in [false, true] {
            for source in ["  ".into(), "x".repeat(4097)] {
                let mut input = valid.clone();
                if image { input.image_source = source; } else { input.plan_source = source; }
                assert!(match_landmarks(&input).is_err());
            }
        }
        let base = serde_json::json!({"plan_landmarks":[{"id":"a","position":{"x":0,"y":0}}],"image_landmarks":[],"plan_source":"p","image_source":"i","threshold_px":1});
        assert!(serde_json::from_value::<LandmarkMatchingRequest>(base.clone()).is_ok());
        let mut extra = base.clone(); extra["apply"] = true.into();
        assert!(serde_json::from_value::<LandmarkMatchingRequest>(extra).is_err());
        let mut extra = base; extra["plan_landmarks"][0]["unverified_extra"] = true.into();
        assert!(serde_json::from_value::<LandmarkMatchingRequest>(extra).is_err());
        let schema = serde_json::to_value(schemars::schema_for!(LandmarkMatchingRequest)).unwrap();
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(schema["$defs"]["NamedLandmark"]["additionalProperties"], false);
        assert_eq!(serde_json::to_value(report).unwrap()["state"], "unique_hypothesis");
    }
}
