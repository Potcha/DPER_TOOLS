//! Correspondence hypotheses from PDF circle candidates and fitted image contours.
//! The caller supplies the current source scope and its provenance envelope.
use crate::{
    dper_binding::{circle_geometry, DrawingView},
    dper_pdf_geometry::{SvgGeometryDocument, VectorCircleCandidate},
    dper_vision::{inspect_geometry, InspectionGeometry, InspectionResult, VisionOptions},
    geometry::Point,
    landmark_matching::{
        match_landmark_pool, validate_landmark_pool_size, LandmarkMatchingReport,
        LandmarkMatchingRequest, NamedLandmark,
    },
};
use image::RgbImage;
use rmcp::schemars;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CircleLandmarkSearch {
    pub id: String,
    #[serde(deserialize_with = "deserialize_center")]
    #[schemars(with = "StrictCenter")]
    pub center_px: Point,
    pub radius_px: f64,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct StrictCenter {
    x: f64,
    y: f64,
}

fn deserialize_center<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Point, D::Error> {
    let StrictCenter { x, y } = StrictCenter::deserialize(deserializer)?;
    Ok(Point { x, y })
}

#[derive(Debug, Serialize)]
pub(crate) struct PlanCircleLandmarkEvidence {
    pub id: String,
    pub circle: VectorCircleCandidate,
}

#[derive(Debug, Serialize)]
pub(crate) struct ImageCircleLandmarkEvidence {
    pub search: CircleLandmarkSearch,
    pub search_disk_radius_px: f64,
    pub fit: InspectionResult,
}

#[derive(Debug, Serialize)]
pub(crate) struct CircleLandmarkMatchReport {
    pub plan_circles: Vec<PlanCircleLandmarkEvidence>,
    pub image_circles: Vec<ImageCircleLandmarkEvidence>,
    pub vision_options: VisionOptions,
    pub matching: LandmarkMatchingReport,
    pub limitations: Vec<String>,
}

pub(crate) fn propose_circle_landmark_matches(
    catalog: &SvgGeometryDocument,
    view: &DrawingView,
    image: &RgbImage,
    plan_circle_ids: &[String],
    image_circle_searches: &[CircleLandmarkSearch],
    threshold_px: f64,
) -> Result<CircleLandmarkMatchReport, String> {
    let count = image_circle_searches.len();
    validate_landmark_pool_size(plan_circle_ids.len(), count)?;
    if catalog.page != view.page {
        return Err("Le catalogue et la vue doivent appartenir à la même page PDF.".into());
    }
    validate_ids("PDF", plan_circle_ids.iter().map(String::as_str))?;
    validate_ids(
        "image",
        image_circle_searches
            .iter()
            .map(|search| search.id.as_str()),
    )?;

    let plan_circles = plan_circle_ids
        .iter()
        .map(|id| {
            if !view.geometry_ids.contains(id) {
                return Err(format!("Le repère PDF {id} n'appartient pas à la vue."));
            }
            let mut matches = catalog
                .primitives
                .iter()
                .filter(|primitive| &primitive.id == id);
            let primitive = matches
                .next()
                .ok_or_else(|| format!("Repère PDF {id} absent du catalogue."))?;
            if primitive.page != view.page || matches.next().is_some() {
                return Err(format!(
                    "Repère PDF {id} ambigu ou situé hors de la page de la vue."
                ));
            }
            circle_geometry(primitive).map_err(|error| format!("Repère PDF {id} : {error}"))?;
            let circle = primitive
                .circle
                .clone()
                .ok_or_else(|| format!("Ajustement PDF {id} absent."))?;
            Ok(PlanCircleLandmarkEvidence {
                id: id.clone(),
                circle,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    let vision_options = VisionOptions::default();
    let mut disks = Vec::with_capacity(count);
    for search in image_circle_searches {
        // Enclose all radial profiles and their gradient probes, including the
        // neighboring offsets used to test for a peak at the search boundary.
        let disk_radius = search.radius_px + vision_options.search_radius_px + 2.0;
        let center = search.center_px;
        if !search.radius_px.is_finite()
            || search.radius_px < 3.0
            || !center.x.is_finite()
            || !center.y.is_finite()
            || center.x - disk_radius < 0.0
            || center.y - disk_radius < 0.0
            || center.x + disk_radius > f64::from(image.width().saturating_sub(1))
            || center.y + disk_radius > f64::from(image.height().saturating_sub(1))
        {
            return Err(format!(
                "Recherche image {} invalide ou disque de recherche hors image.",
                search.id
            ));
        }
        if disks.iter().any(|(previous_center, previous_radius)| {
            center.dist(*previous_center) <= disk_radius + previous_radius
        }) {
            return Err(format!(
                "Le disque de recherche {} touche ou chevauche un autre disque.",
                search.id
            ));
        }
        disks.push((center, disk_radius));
    }

    let mut image_circles = Vec::with_capacity(count);
    let mut image_landmarks: Vec<NamedLandmark> = Vec::with_capacity(count);
    for (search, (_, disk_radius)) in image_circle_searches.iter().zip(disks) {
        let expected = InspectionGeometry::Circle {
            center: search.center_px,
            radius: search.radius_px,
        };
        let fit = inspect_geometry(image, &expected, &vision_options)
            .map_err(|error| format!("Repère image {} : {error}", search.id))?;
        let InspectionGeometry::Circle { center, radius } = &fit.geometry else {
            return Err(format!(
                "Le repère image {} n'a pas produit un ajustement circulaire.",
                search.id
            ));
        };
        if search.center_px.dist(*center) + radius > disk_radius {
            return Err(format!(
                "Le cercle ajusté du repère image {} sort de son disque de recherche.",
                search.id
            ));
        }
        if image_landmarks
            .iter()
            .any(|landmark| center.dist(landmark.position) <= 1e-9)
        {
            return Err(format!(
                "Le centre ajusté du repère image {} n'est pas distinct.",
                search.id
            ));
        }
        image_landmarks.push(NamedLandmark {
            id: search.id.clone(),
            position: *center,
        });
        image_circles.push(ImageCircleLandmarkEvidence {
            search: search.clone(),
            search_disk_radius_px: disk_radius,
            fit,
        });
    }

    let mut matching = match_landmark_pool(&LandmarkMatchingRequest {
        plan_landmarks: plan_circles
            .iter()
            .map(|item| NamedLandmark {
                id: item.id.clone(),
                position: item.circle.center,
            })
            .collect(),
        image_landmarks,
        plan_source: "Centres des candidats circulaires du catalogue PDF de la vue".into(),
        image_source: "Centres ajustés sur les bords observés dans l'image".into(),
        threshold_px,
    })?;
    // Replace the source assumptions of the generic point-input tool. Its
    // permutation and similarity limits still apply to these derived centers.
    drop(matching.limitations.splice(..2, [
        "Centres PDF issus de candidats vectoriels de la vue. Les centres image proviennent des contours ajustés dans les zones indiquées.".into(),
        "Le moteur d'appariement ne vérifie pas lui-même la provenance des sources. Les preuves de périmètre sont fournies séparément.".into(),
    ]));
    if count > plan_circles.len() {
        matching.limitations.push(
            "Chaque hypothèse utilise tous les repères PDF et un sous-ensemble ordonné distinct de repères photo. Les repères photo non associés restent consultables dans les preuves.".into(),
        );
    }
    Ok(CircleLandmarkMatchReport {
        plan_circles,
        image_circles,
        vision_options,
        matching,
        limitations: vec![
            "Appariement des centres uniquement. Les rayons ne servent ni à classer les hypothèses ni à déduire une échelle physique.".into(),
            "La recherche est dirigée par les amorces fournies. Les ajustements et leurs résidus décrivent les pixels observés sans qualifier le repère physique.".into(),
        ],
    })
}

fn validate_ids<'a>(side: &str, ids: impl Iterator<Item = &'a str>) -> Result<(), String> {
    let mut seen = HashSet::new();
    for id in ids {
        let trimmed = id.trim();
        if trimmed.is_empty() || id.len() > 128 || !seen.insert(trimmed) {
            return Err(format!("Les identifiants {side} doivent être distincts, renseignés et limités à 128 octets."));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{dper_pdf_geometry::parse_svg_geometry, landmark_matching::LandmarkMatchingState};
    use image::Rgb;
    use std::f64::consts::TAU;

    const TRIANGLE: [Point; 3] = [
        Point { x: 50.0, y: 50.0 },
        Point { x: 156.0, y: 61.0 },
        Point { x: 77.0, y: 167.0 },
    ];
    const SQUARE: [Point; 4] = [
        Point { x: 55.0, y: 55.0 },
        Point { x: 170.0, y: 55.0 },
        Point { x: 170.0, y: 170.0 },
        Point { x: 55.0, y: 170.0 },
    ];

    fn plan(points: &[Point]) -> (SvgGeometryDocument, DrawingView, Vec<String>) {
        let mut svg = String::from(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="240pt" height="240pt" viewBox="0 0 240 240">"#,
        );
        for center in points {
            svg.push_str("<path d=\"");
            for index in 0..64 {
                let angle = index as f64 * TAU / 64.0;
                let command = if index == 0 { 'M' } else { 'L' };
                svg.push_str(&format!(
                    "{command} {} {} ",
                    center.x + 10.0 * angle.cos(),
                    center.y + 10.0 * angle.sin()
                ));
            }
            svg.push_str("Z\"/>");
        }
        svg.push_str("</svg>");
        let catalog = parse_svg_geometry(&svg, 1).unwrap();
        let ids: Vec<_> = catalog
            .primitives
            .iter()
            .map(|primitive| primitive.id.clone())
            .collect();
        let view = DrawingView {
            id: "view".into(),
            page: 1,
            label: "Vue test".into(),
            feature_ids: Vec::new(),
            geometry_ids: ids.clone(),
            bounds: catalog.view_box,
            document_fingerprint: "test".into(),
            source_proof: None,
        };
        (catalog, view, ids)
    }

    fn image_points(points: &[Point]) -> Vec<Point> {
        points
            .iter()
            .map(|p| Point {
                x: 220.0 - p.y,
                y: p.x + 12.0,
            })
            .collect()
    }

    fn raster(points: &[Point]) -> RgbImage {
        RgbImage::from_fn(256, 256, |x, y| {
            let pixel = Point {
                x: f64::from(x),
                y: f64::from(y),
            };
            let nearest_edge = points
                .iter()
                .map(|p| pixel.dist(*p) - 16.0)
                .fold(f64::INFINITY, f64::min);
            let intensity =
                (25.0 + 205.0 * (1.0 + (nearest_edge / 0.65).tanh()) * 0.5).round() as u8;
            Rgb([intensity; 3])
        })
    }

    fn searches(points: &[Point]) -> Vec<CircleLandmarkSearch> {
        points
            .iter()
            .enumerate()
            .map(|(i, p)| CircleLandmarkSearch {
                id: format!("image-{i}"),
                center_px: Point {
                    x: p.x + 1.3,
                    y: p.y - 0.8,
                },
                radius_px: 18.1,
            })
            .rev()
            .collect()
    }

    #[test]
    fn asymmetric_triangle_keeps_one_hypothesis_and_fitted_pixel_evidence() {
        let (catalog, view, ids) = plan(&TRIANGLE);
        let centers = image_points(&TRIANGLE);
        let report = propose_circle_landmark_matches(
            &catalog,
            &view,
            &raster(&centers),
            &ids,
            &searches(&centers),
            0.3,
        )
        .unwrap();
        assert_eq!(
            report.matching.state,
            LandmarkMatchingState::UniqueHypothesis
        );
        assert_eq!(report.matching.rejected_count, 5);
        let hypothesis = &report.matching.hypotheses[0];
        for (index, mapping) in hypothesis.mappings.iter().enumerate() {
            assert_eq!(mapping.plan_id, ids[index]);
            assert_eq!(mapping.image_id, format!("image-{index}"));
        }
        for (item, center) in report.image_circles.iter().zip(centers.iter().rev()) {
            let InspectionGeometry::Circle {
                center: fitted,
                radius,
            } = &item.fit.geometry
            else {
                panic!("circle expected")
            };
            assert!(fitted.dist(*center) < 0.2);
            assert!(fitted.dist(item.search.center_px) > 1.0);
            assert!((radius - 16.0).abs() < 0.2);
            assert!(item.fit.evidence.coverage_fraction > 0.9);
        }
        assert_eq!(report.plan_circles[0].id, ids[0]);
        assert!(report.plan_circles[0].circle.center.dist(TRIANGLE[0]) < 1e-8);
        assert_eq!(
            report.vision_options.sample_count,
            VisionOptions::default().sample_count
        );
        assert!(!report.matching.registered && !report.matching.reviewed);
    }

    #[test]
    fn square_keeps_all_four_orientation_preserving_hypotheses() {
        let (catalog, view, ids) = plan(&SQUARE);
        let centers = image_points(&SQUARE);
        let report = propose_circle_landmark_matches(
            &catalog,
            &view,
            &raster(&centers),
            &ids,
            &searches(&centers),
            0.3,
        )
        .unwrap();
        assert_eq!(report.matching.state, LandmarkMatchingState::Ambiguous);
        assert_eq!(report.matching.accepted_count, 4);
        assert_eq!(report.matching.rejected_permutations.len(), 20);
        assert!(report.matching.unique_candidate_id.is_none());
    }

    #[test]
    fn missing_contour_does_not_fall_back_to_amorce() {
        let (catalog, view, ids) = plan(&TRIANGLE);
        let centers = image_points(&TRIANGLE);
        let error = propose_circle_landmark_matches(
            &catalog,
            &view,
            &raster(&centers[..2]),
            &ids,
            &searches(&centers),
            0.3,
        )
        .unwrap_err();
        assert!(
            error.contains("image-2") && error.contains("Contour"),
            "{error}"
        );
    }

    #[test]
    fn rejects_an_acceptable_local_fit_that_extends_past_its_search_disk() {
        let (catalog, view, ids) = plan(&TRIANGLE);
        let seed = Point { x: 90.0, y: 85.0 };
        let center = Point { x: 99.0, y: 85.0 };
        let image = RgbImage::from_fn(400, 330, |x, y| {
            let pixel = Point {
                x: f64::from(x),
                y: f64::from(y),
            };
            let edge = pixel.dist(center) - 62.0;
            let intensity = (25.0 + 205.0 * (1.0 + (edge / 0.65).tanh()) * 0.5).round() as u8;
            Rgb([intensity; 3])
        });
        let expected = InspectionGeometry::Circle {
            center: seed,
            radius: 60.0,
        };
        assert!(inspect_geometry(&image, &expected, &VisionOptions::default()).is_ok());
        let searches = [
            CircleLandmarkSearch {
                id: "large".into(),
                center_px: seed,
                radius_px: 60.0,
            },
            CircleLandmarkSearch {
                id: "right".into(),
                center_px: Point { x: 270.0, y: 85.0 },
                radius_px: 16.0,
            },
            CircleLandmarkSearch {
                id: "bottom".into(),
                center_px: Point { x: 270.0, y: 245.0 },
                radius_px: 16.0,
            },
        ];
        let error = propose_circle_landmark_matches(&catalog, &view, &image, &ids, &searches, 0.3)
            .unwrap_err();
        assert!(error.contains("sort de son disque"), "{error}");
    }

    #[test]
    fn rejects_out_of_view_invalid_circles_and_overlapping_or_cropped_searches() {
        let (mut catalog, mut view, ids) = plan(&TRIANGLE);
        let centers = image_points(&TRIANGLE);
        let image = raster(&centers);
        let mut searches = searches(&centers);
        view.geometry_ids.remove(0);
        assert!(
            propose_circle_landmark_matches(&catalog, &view, &image, &ids, &searches, 0.3)
                .unwrap_err()
                .contains("vue")
        );
        view.geometry_ids.push(ids[0].clone());
        let original = catalog.primitives[0].circle.take();
        assert!(
            propose_circle_landmark_matches(&catalog, &view, &image, &ids, &searches, 0.3)
                .unwrap_err()
                .contains("circulaire")
        );
        catalog.primitives[0].circle = original;
        searches[0].center_px = searches[1].center_px;
        assert!(
            propose_circle_landmark_matches(&catalog, &view, &image, &ids, &searches, 0.3)
                .unwrap_err()
                .contains("chevauche")
        );
        searches[0].center_px.x = 20.0;
        assert!(
            propose_circle_landmark_matches(&catalog, &view, &image, &ids, &searches, 0.3)
                .unwrap_err()
                .contains("hors image")
        );
    }
}
