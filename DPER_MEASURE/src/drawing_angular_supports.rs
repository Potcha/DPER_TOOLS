//! Documentary angles of an explicit pair of intersecting segments; never a photo measurement.
use super::{feature_fingerprint, feature_index, primitive_fingerprint, primitive_index,
    validate_view, DrawingView, ViewSourceProof};
use crate::{
    dper_drawing::{DrawingControlPlan, DrawingDimensionKind, DrawingMeasurability,
        DrawingSource, DrawingUnit, DrawingUnitStatus},
    dper_pdf_geometry::{SvgGeometryDocument, VectorPrimitive, VectorPrimitiveKind},
    geometry::Point,
};
use serde::Serialize;

const EPS_PT: f64 = 1e-6;
const MIN_ACUTE_ANGLE_DEG: f64 = 15.0;

#[derive(Clone, Debug, Serialize)]
pub struct AngularSupportProposal {
    pub state: &'static str,
    pub reviewed: bool,
    pub measured_on_photo: bool,
    pub feature_id: String,
    pub support_ids: [String; 2],
    pub page: u32,
    pub view_id: String,
    pub document_fingerprint: String,
    pub view_source_proof: ViewSourceProof,
    pub feature_fingerprint: String,
    pub primitive_fingerprints: [String; 2],
    pub feature_source: DrawingSource,
    pub supports: [VectorPrimitive; 2],
    pub justification: String,
    pub intersection_pt: Point,
    /// Unoriented sectors, acute first. Both remain present even for 90/90 degrees.
    pub candidate_angles_deg: [f64; 2],
    /// No sector is selected by nominal, style, screen orientation, or support order.
    pub selected_angle_deg: Option<f64>,
    pub minimum_acute_angle_deg: f64,
    pub segment_endpoint_tolerance_pt: f64,
    pub suspension_reasons: Vec<String>,
}

fn line(primitive: &VectorPrimitive) -> Result<(Point, Point, f64), String> {
    if primitive.kind != VectorPrimitiveKind::Line || primitive.closed
        || primitive.points.len() != 2
        || primitive.points.iter().any(|p| !p.x.is_finite() || !p.y.is_finite())
    {
        return Err("Chaque support doit être une ligne ouverte à exactement deux points finis.".into());
    }
    let a = primitive.points[0];
    let b = primitive.points[1];
    let length = (b.x - a.x).hypot(b.y - a.y);
    if !length.is_finite() || length <= EPS_PT {
        return Err("Support rectiligne dégénéré ou hors limites numériques.".into());
    }
    Ok((a, Point { x: (b.x - a.x) / length, y: (b.y - a.y) / length }, length))
}

fn cross(a: Point, b: Point) -> f64 { a.x * b.y - a.y * b.x }

pub(crate) fn inspect(
    plan: &DrawingControlPlan,
    docfp: &str,
    catalog: &SvgGeometryDocument,
    view: &DrawingView,
    feature_id: &str,
    support_ids: &[String; 2],
    justification: &str,
) -> Result<AngularSupportProposal, String> {
    validate_view(plan, docfp, catalog, view)?;
    if view.feature_ids.iter().filter(|id| id.as_str() == feature_id).count() != 1 {
        return Err("La cote doit appartenir une seule fois à cette vue explicite.".into());
    }
    if justification.trim().is_empty() || justification.chars().count() > 2000 {
        return Err("La justification doit contenir de 1 à 2 000 caractères.".into());
    }
    let features = feature_index(plan)?;
    let feature = features.get(feature_id).ok_or("Caractéristique absente.")?;
    if feature.kind != DrawingDimensionKind::Angle
        || feature.unit != Some(DrawingUnit::Degree)
        || feature.measurability == DrawingMeasurability::NotMeasurable2d
    {
        return Err("Une cote angulaire explicite en degrés, potentiellement observable en 2D, est requise.".into());
    }
    // A coherent linear document unit does not override the explicit angular unit.
    for context in plan.unit_context.iter().chain(feature.unit_context.iter().map(|e| &e.document)) {
        let units = context.declarations.iter().flat_map(|d| d.units.iter()).collect::<Vec<_>>();
        if context.status == DrawingUnitStatus::Conflict
            || units.first().is_some_and(|first| units.iter().any(|unit| unit != first))
        {
            return Err("Les déclarations documentaires d'unités sont contradictoires.".into());
        }
    }
    if support_ids[0] == support_ids[1] || support_ids.iter().any(|id|
        view.geometry_ids.iter().filter(|member| *member == id).count() != 1)
    {
        return Err("Deux supports distincts appartenant à la vue explicite sont requis.".into());
    }
    let primitives = primitive_index(catalog)?;
    let first = primitives.get(support_ids[0].as_str()).ok_or("Premier support absent.")?;
    let second = primitives.get(support_ids[1].as_str()).ok_or("Second support absent.")?;
    let (a, direction, length) = line(first)?;
    let (b, other_direction, other_length) = line(second)?;
    let determinant = cross(direction, other_direction);
    let dot = direction.x * other_direction.x + direction.y * other_direction.y;
    let acute = determinant.abs().atan2(dot.abs()).to_degrees();
    if !acute.is_finite() || acute < MIN_ACUTE_ANGLE_DEG {
        return Err("Supports parallèles ou presque parallèles : angle aigu inférieur à 15 degrés.".into());
    }
    let delta = Point { x: b.x - a.x, y: b.y - a.y };
    let along = cross(delta, other_direction) / determinant;
    let other_along = cross(delta, direction) / determinant;
    let intersection = Point { x: a.x + along * direction.x, y: a.y + along * direction.y };
    if !intersection.x.is_finite() || !intersection.y.is_finite()
        || intersection.x < 0.0 || intersection.x > catalog.width_pt
        || intersection.y < 0.0 || intersection.y > catalog.height_pt
    {
        return Err("L'intersection des supports doit rester sur la page PDF.".into());
    }
    if !along.is_finite() || !other_along.is_finite()
        || along < -EPS_PT || along > length + EPS_PT
        || other_along < -EPS_PT || other_along > other_length + EPS_PT
    {
        return Err("L'intersection exige une extrapolation : elle doit appartenir aux deux segments (tolérance 1e-6 point PDF).".into());
    }
    let mut suspension_reasons = vec![
        "annotation_vs_part_contour_roles_are_not_recognized".into(),
        "angular_sector_selection_required_even_when_candidates_are_equal".into(),
        "physical_view_and_planar_observability_require_review".into(),
        "documentary_pair_requires_explicit_review".into(),
        "pdf_geometry_is_not_physical_measurement".into(),
        "no_photo_measurement_or_conformity_validation".into(),
    ];
    if catalog.unsupported_path_count > 0 || !catalog.warnings.is_empty() {
        suspension_reasons.push("vector_extraction_has_limitations".into());
    }
    Ok(AngularSupportProposal {
        state: "documentary_proposal", reviewed: false, measured_on_photo: false,
        feature_id: feature_id.into(), support_ids: support_ids.clone(), page: catalog.page,
        view_id: view.id.clone(), document_fingerprint: docfp.into(),
        view_source_proof: view.source_proof.clone().ok_or("Preuve de vue absente.")?,
        feature_fingerprint: feature_fingerprint(feature)?,
        primitive_fingerprints: [primitive_fingerprint(first)?, primitive_fingerprint(second)?],
        feature_source: feature.source.clone(), supports: [(**first).clone(), (**second).clone()],
        justification: justification.trim().into(), intersection_pt: intersection,
        candidate_angles_deg: [acute, 180.0 - acute], selected_angle_deg: None,
        minimum_acute_angle_deg: MIN_ACUTE_ANGLE_DEG, segment_endpoint_tolerance_pt: EPS_PT,
        suspension_reasons,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{dper_binding::define_view, dper_drawing::parse_text_plan,
        dper_pdf_geometry::parse_svg_geometry, dper_pdf_layout::{PdfBox, PdfPageGeometry}};

    fn fixture(points: [[f64; 4]; 2]) -> (DrawingControlPlan, SvgGeometryDocument, DrawingView, [String; 2]) {
        let mut plan = parse_text_plan("Units mm\nAngle 123°", "test.pdf");
        plan.page_geometry = vec![PdfPageGeometry { page: 1, width_pt: 240.0, height_pt: 180.0 }];
        let feature = &mut plan.characteristics[0];
        feature.source.page = Some(1);
        feature.source.pdf_bbox = Some(PdfBox { x_min: 10.0, y_min: 10.0, x_max: 50.0, y_max: 20.0 });
        feature.source.word_ids = vec!["word-1".into()];
        let feature_id = feature.id.clone();
        let body = points.iter().map(|p| format!("<path d=\"M {} {} L {} {}\"/>", p[0], p[1], p[2], p[3])).collect::<String>();
        let svg = format!(r#"<svg width="240pt" height="180pt" viewBox="0 0 240 180">{body}</svg>"#);
        let catalog = parse_svg_geometry(&svg, 1).unwrap();
        let ids = [catalog.primitives[0].id.clone(), catalog.primitives[1].id.clone()];
        let view = define_view(&plan, "doc", &catalog, "Périmètre", &[feature_id], &ids).unwrap();
        (plan, catalog, view, ids)
    }
    fn run(plan: &DrawingControlPlan, catalog: &SvgGeometryDocument, view: &DrawingView, ids: &[String; 2]) -> Result<AngularSupportProposal, String> {
        inspect(plan, "doc", catalog, view, &plan.characteristics[0].id, ids, "Paire choisie explicitement")
    }
    fn refresh(plan: &DrawingControlPlan, catalog: &SvgGeometryDocument, view: &DrawingView) -> DrawingView {
        define_view(plan, "doc", catalog, "Périmètre", &view.feature_ids, &view.geometry_ids).unwrap()
    }
    fn base() -> [[f64; 4]; 2] { [[40.0, 80.0, 120.0, 80.0], [40.0, 40.0, 120.0, 120.0]] }

    #[test]
    fn proposes_45_and_135_without_using_nominal_or_linear_document_units() {
        let (mut plan, catalog, view, ids) = fixture(base());
        assert_eq!(plan.characteristics[0].nominal, Some(123.0));
        let result = run(&plan, &catalog, &view, &ids).unwrap();
        assert!((result.candidate_angles_deg[0] - 45.0).abs() < 1e-10);
        assert!((result.candidate_angles_deg[1] - 135.0).abs() < 1e-10);
        assert_eq!(result.intersection_pt, Point { x: 80.0, y: 80.0 });
        assert_eq!(result.selected_angle_deg, None);
        plan.characteristics[0].nominal = Some(135.0);
        let changed = run(&plan, &catalog, &refresh(&plan, &catalog, &view), &ids).unwrap();
        assert_eq!(result.candidate_angles_deg, changed.candidate_angles_deg);
        assert_eq!(changed.selected_angle_deg, None);
        assert_ne!(result.feature_fingerprint, changed.feature_fingerprint);
    }

    #[test]
    fn order_endpoints_rotation_and_translation_preserve_candidates() {
        for angle in [0.0_f64, 0.4, 1.57] {
            for reverse in [false, true] {
                let mut points = base();
                for line in &mut points {
                    for pair in line.chunks_exact_mut(2) {
                        let (x, y) = (pair[0] - 80.0, pair[1] - 80.0);
                        pair[0] = 110.0 + x * angle.cos() - y * angle.sin();
                        pair[1] = 85.0 + x * angle.sin() + y * angle.cos();
                    }
                    if reverse { line.swap(0, 2); line.swap(1, 3); }
                }
                let (plan, catalog, view, ids) = fixture(points);
                for pair in [ids.clone(), [ids[1].clone(), ids[0].clone()]] {
                    let result = run(&plan, &catalog, &view, &pair).unwrap();
                    assert!((result.candidate_angles_deg[0] - 45.0).abs() < 1e-10);
                    assert!((result.candidate_angles_deg[1] - 135.0).abs() < 1e-10);
                    assert!((result.intersection_pt.x - 110.0).abs() < 1e-10);
                    assert!((result.intersection_pt.y - 85.0).abs() < 1e-10);
                }
            }
        }
    }

    #[test]
    fn right_angle_still_requires_explicit_sector_selection() {
        let (plan, catalog, view, ids) = fixture([base()[0], [80.0, 40.0, 80.0, 120.0]]);
        let result = run(&plan, &catalog, &view, &ids).unwrap();
        assert_eq!(result.candidate_angles_deg, [90.0, 90.0]);
        assert_eq!(result.selected_angle_deg, None);
        assert!(result.suspension_reasons.iter().any(|reason| reason.contains("sector_selection_required")));
    }

    #[test]
    fn refuses_parallel_nearly_parallel_extrapolation_and_outside_page() {
        for points in [
            [base()[0], [40.0, 90.0, 120.0, 90.0]],
            [base()[0], [40.0, 78.0, 120.0, 82.0]],
            [base()[0], [130.0, 130.0, 150.0, 150.0]],
            [[10.0, 20.0, 30.0, 40.0], [10.0, 40.0, 30.0, 80.0]],
        ] {
            let (plan, catalog, view, ids) = fixture(points);
            assert!(run(&plan, &catalog, &view, &ids).is_err());
        }
    }

    #[test]
    fn accepts_endpoints_only_with_documentary_point_tolerance() {
        for gap in [0.0, EPS_PT * 0.5, EPS_PT * 2.0] {
            let (plan, catalog, view, ids) = fixture([
                [40.0, 80.0, 80.0 - gap, 80.0], [80.0, 80.0, 100.0, 100.0],
            ]);
            assert_eq!(run(&plan, &catalog, &view, &ids).is_ok(), gap <= EPS_PT);
        }
    }

    #[test]
    fn preserves_full_sources_flags_proofs_and_explicit_limits() {
        let (plan, catalog, view, ids) = fixture(base());
        let result = run(&plan, &catalog, &view, &ids).unwrap();
        assert_eq!(result.state, "documentary_proposal");
        assert!(!result.reviewed && !result.measured_on_photo);
        assert_eq!(result.document_fingerprint, "doc");
        assert_eq!(result.view_source_proof, view.source_proof.unwrap());
        assert_eq!(result.minimum_acute_angle_deg, 15.0);
        assert_eq!(result.segment_endpoint_tolerance_pt, EPS_PT);
        assert_eq!(serde_json::to_value(&result.feature_source).unwrap(), serde_json::to_value(&plan.characteristics[0].source).unwrap());
        assert_eq!(serde_json::to_value(&result.supports).unwrap(), serde_json::to_value(&catalog.primitives).unwrap());
        assert!(result.suspension_reasons.iter().any(|reason| reason.contains("roles_are_not_recognized")));
        assert!(result.suspension_reasons.iter().any(|reason| reason.contains("not_physical_measurement")));
        assert!(result.primitive_fingerprints.iter().all(|fp| !fp.is_empty()));
    }

    #[test]
    fn refuses_stale_source_style_missing_proof_scope_and_duplicates() {
        let (plan, catalog, view, ids) = fixture(base());
        assert!(run(&plan, &catalog, &view, &[ids[0].clone(), ids[0].clone()]).is_err());
        let scoped = define_view(&plan, "doc", &catalog, "Autre", &view.feature_ids, &[ids[0].clone()]).unwrap();
        assert!(run(&plan, &catalog, &scoped, &ids).is_err());
        let mut historical = view.clone();
        historical.source_proof = None;
        assert!(run(&plan, &catalog, &historical, &ids).is_err());
        for style in [false, true] {
            let mut changed = catalog.clone();
            if style { changed.primitives[0].style.stroke = Some("red".into()); }
            else { changed.primitives[0].source.path_data.push(' '); }
            assert!(run(&plan, &changed, &view, &ids).is_err());
        }
        assert!(inspect(&plan, "changed", &catalog, &view, &plan.characteristics[0].id, &ids, "Paire explicite").is_err());
    }

    #[test]
    fn refuses_nonangular_unit_conflicts_and_nonobservable_features() {
        let (plan, catalog, view, ids) = fixture(base());
        for variant in 0..6 {
            let mut changed = plan.clone();
            match variant {
                0 => changed.characteristics[0].kind = DrawingDimensionKind::Linear,
                1 => changed.characteristics[0].unit = None,
                2 => changed.characteristics[0].unit = Some(DrawingUnit::Millimeter),
                3 => changed.characteristics[0].measurability = DrawingMeasurability::NotMeasurable2d,
                4 => changed.unit_context = parse_text_plan("Units mm\nUnits in\nAngle 123°", "test.pdf").unit_context,
                _ => changed.characteristics[0].unit_context.as_mut().unwrap().document.status = DrawingUnitStatus::Conflict,
            }
            let current = refresh(&changed, &catalog, &view);
            assert!(run(&changed, &catalog, &current, &ids).is_err());
        }
    }

    #[test]
    fn refuses_invalid_shape_degenerate_nonfinite_and_justification() {
        let (plan, catalog, view, ids) = fixture(base());
        for variant in 0..5 {
            let mut changed = catalog.clone();
            let primitive = &mut changed.primitives[0];
            match variant {
                0 => primitive.kind = VectorPrimitiveKind::Polyline,
                1 => primitive.closed = true,
                2 => primitive.points.push(primitive.points[0]),
                3 => primitive.points[1] = primitive.points[0],
                _ => primitive.points[1].x = f64::NAN,
            }
            match define_view(&plan, "doc", &changed, "Périmètre", &view.feature_ids, &view.geometry_ids) {
                Ok(current) => assert!(run(&plan, &changed, &current, &ids).is_err()),
                Err(_) => assert_eq!(variant, 4),
            }
        }
        for justification in [" ".to_string(), "a".repeat(2001)] {
            assert!(inspect(&plan, "doc", &catalog, &view, &plan.characteristics[0].id, &ids, &justification).is_err());
        }
    }
}
