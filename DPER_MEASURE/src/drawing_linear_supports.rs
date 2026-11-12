//! Documentary inspection of an explicitly supplied pair; never a photo measurement.
use super::{feature_fingerprint, feature_index, primitive_fingerprint, primitive_index,
    validate_view, DrawingView, ViewSourceProof};
use crate::{
    dper_drawing::{DrawingControlPlan, DrawingDimensionKind, DrawingMeasurability,
        DrawingSource, DrawingUnit, DrawingUnitStatus},
    dper_pdf_geometry::{SvgGeometryDocument, VectorPrimitive, VectorPrimitiveKind},
    geometry::Point,
};
use serde::Serialize;

const EPS: f64 = 1e-6;

#[derive(Clone, Debug, Serialize)]
pub struct LinearSupportProposal {
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
    pub normal_separation_pt: f64,
    pub overlap_length_pt: f64,
    pub projected_points_pt: [Point; 2],
    pub suspension_reasons: Vec<String>,
}

pub(crate) fn line(primitive: &VectorPrimitive) -> Result<(Point, Point, Point), String> {
    if primitive.kind != VectorPrimitiveKind::Line || primitive.closed
        || primitive.points.len() != 2
        || primitive.points.iter().any(|p| !p.x.is_finite() || !p.y.is_finite())
    {
        return Err("Chaque support doit être une ligne ouverte à exactement deux points finis.".into());
    }
    let a = primitive.points[0];
    let b = primitive.points[1];
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let length = dx.hypot(dy);
    if !length.is_finite() || length <= EPS {
        return Err("Support rectiligne dégénéré ou hors limites numériques.".into());
    }
    let mut direction = Point { x: dx / length, y: dy / length };
    if direction.x < 0.0 || (direction.x == 0.0 && direction.y < 0.0) {
        direction.x = -direction.x;
        direction.y = -direction.y;
    }
    Ok((a, b, direction))
}

fn dot(a: Point, b: Point) -> f64 { a.x * b.x + a.y * b.y }
fn sub(a: Point, b: Point) -> Point { Point { x: a.x - b.x, y: a.y - b.y } }

pub(crate) fn inspect(
    plan: &DrawingControlPlan,
    docfp: &str,
    catalog: &SvgGeometryDocument,
    view: &DrawingView,
    feature_id: &str,
    support_ids: &[String; 2],
    justification: &str,
) -> Result<LinearSupportProposal, String> {
    validate_view(plan, docfp, catalog, view)?;
    if view.feature_ids.iter().filter(|id| id.as_str() == feature_id).count() != 1 {
        return Err("La cote doit appartenir une seule fois à cette vue explicite.".into());
    }
    if justification.trim().is_empty() || justification.chars().count() > 2000 {
        return Err("La justification doit contenir de 1 à 2 000 caractères.".into());
    }
    let features = feature_index(plan)?;
    let feature = features.get(feature_id).ok_or("Caractéristique absente.")?;
    if feature.kind != DrawingDimensionKind::Linear
        || feature.measurability == DrawingMeasurability::NotMeasurable2d
        || feature.unit == Some(DrawingUnit::Degree)
    {
        return Err("Une cote linéaire potentiellement observable en 2D est requise.".into());
    }
    for context in plan.unit_context.iter().chain(feature.unit_context.iter().map(|e| &e.document)) {
        let units = context.declarations.iter().flat_map(|d| d.units.iter()).collect::<Vec<_>>();
        if context.status == DrawingUnitStatus::Conflict
            || units.first().is_some_and(|first| units.iter().any(|unit| unit != first))
            || context.unique_unit() == Some(DrawingUnit::Degree)
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
    let (a, b, direction) = line(first)?;
    let (c, d, other_direction) = line(second)?;
    let cross = direction.x * other_direction.y - direction.y * other_direction.x;
    if cross.abs() > EPS {
        return Err("Les supports doivent être parallèles dans la tolérance documentaire.".into());
    }
    let first_end = dot(sub(b, a), direction);
    let c_projection = dot(sub(c, a), direction);
    let d_projection = dot(sub(d, a), direction);
    let lo = 0.0_f64.min(first_end).max(c_projection.min(d_projection));
    let hi = 0.0_f64.max(first_end).min(c_projection.max(d_projection));
    let overlap = hi - lo;
    if !overlap.is_finite() || overlap <= EPS {
        return Err("Les supports n'ont pas de recouvrement commun sans extrapolation.".into());
    }
    // Angular tolerance must not permit a crossing or contact inside the overlap.
    let signed_gap = |along: f64| {
        let t = (along - c_projection) / dot(other_direction, direction);
        let delta = Point { x: c.x + other_direction.x * t - a.x - direction.x * along,
            y: c.y + other_direction.y * t - a.y - direction.y * along };
        direction.x * delta.y - direction.y * delta.x
    };
    let (gap_lo, gap_hi) = (signed_gap(lo), signed_gap(hi));
    if !gap_lo.is_finite() || !gap_hi.is_finite() || gap_lo.abs() <= EPS
        || gap_hi.abs() <= EPS || gap_lo.is_sign_positive() != gap_hi.is_sign_positive()
    {
        return Err("Les supports se croisent ou se touchent dans leur recouvrement.".into());
    }
    let middle = lo + overlap / 2.0;
    let p = Point { x: a.x + direction.x * middle, y: a.y + direction.y * middle };
    let along_other = (middle - c_projection) / dot(other_direction, direction);
    let q = Point { x: c.x + other_direction.x * along_other,
        y: c.y + other_direction.y * along_other };
    let delta = sub(q, p);
    let separation = (direction.x * delta.y - direction.y * delta.x).abs();
    if !separation.is_finite() || separation <= EPS
        || [p, q].iter().any(|point| !point.x.is_finite() || !point.y.is_finite())
    {
        return Err("Les supports sont confondus ou leur séparation est hors limites.".into());
    }
    let mut suspension_reasons = vec![
        "annotation_vs_part_contour_roles_are_not_recognized".into(),
        "physical_view_and_planar_observability_require_review".into(),
        "documentary_pair_requires_explicit_review".into(),
        "pdf_points_are_not_physical_measurement_units".into(),
        "no_photo_measurement_or_conformity_validation".into(),
    ];
    if feature.unit.is_none() { suspension_reasons.push("feature_unit_absent".into()); }
    if catalog.unsupported_path_count > 0 || !catalog.warnings.is_empty() {
        suspension_reasons.push("vector_extraction_has_limitations".into());
    }
    Ok(LinearSupportProposal {
        state: "documentary_proposal", reviewed: false, measured_on_photo: false,
        feature_id: feature_id.into(), support_ids: support_ids.clone(), page: catalog.page,
        view_id: view.id.clone(), document_fingerprint: docfp.into(),
        view_source_proof: view.source_proof.clone().ok_or("Preuve de vue absente.")?,
        feature_fingerprint: feature_fingerprint(feature)?,
        primitive_fingerprints: [primitive_fingerprint(first)?, primitive_fingerprint(second)?],
        feature_source: feature.source.clone(), supports: [(**first).clone(), (**second).clone()],
        justification: justification.trim().into(), normal_separation_pt: separation,
        overlap_length_pt: overlap, projected_points_pt: [p, q], suspension_reasons,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{dper_binding::define_view, dper_drawing::parse_text_plan,
        dper_pdf_geometry::parse_svg_geometry, dper_pdf_layout::{PdfBox, PdfPageGeometry}};

    fn fixture(points: [[f64; 4]; 2]) -> (DrawingControlPlan, SvgGeometryDocument, DrawingView, [String; 2]) {
        let mut plan = parse_text_plan("Longueur 999 mm", "test.pdf");
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
    fn run(plan: &DrawingControlPlan, catalog: &SvgGeometryDocument, view: &DrawingView, ids: &[String; 2]) -> Result<LinearSupportProposal, String> {
        inspect(plan, "doc", catalog, view, &plan.characteristics[0].id, ids, "Paire choisie explicitement")
    }
    fn base() -> [[f64; 4]; 2] { [[20.0, 50.0, 100.0, 50.0], [40.0, 60.0, 120.0, 60.0]] }

    #[test]
    fn angular_tolerance_never_accepts_a_crossing_inside_the_overlap() {
        let (plan, catalog, view, ids) = fixture([
            [20.0, 50.0, 100.0, 50.0], [40.0, 49.99999, 120.0, 50.00005],
        ]);
        assert!(run(&plan, &catalog, &view, &ids).is_err());
        assert!(run(&plan, &catalog, &view, &[ids[1].clone(), ids[0].clone()]).is_err());
    }

    #[test]
    fn translates_and_rotates_without_changing_distance_or_overlap() {
        for angle in [0.0_f64, 0.4, 1.57] {
            let mut points = base();
            for line in &mut points {
                for pair in line.chunks_exact_mut(2) {
                    let (x, y) = (pair[0], pair[1]);
                    pair[0] = 100.0 + x * angle.cos() - y * angle.sin();
                    pair[1] = 10.0 + x * angle.sin() + y * angle.cos();
                }
            }
            let (plan, catalog, view, ids) = fixture(points);
            let result = run(&plan, &catalog, &view, &ids).unwrap();
            assert!((result.normal_separation_pt - 10.0).abs() < 1e-10);
            assert!((result.overlap_length_pt - 60.0).abs() < 1e-10);
        }
    }

    #[test]
    fn order_and_endpoint_direction_do_not_change_geometry() {
        for points in [base(), [[100.0, 50.0, 20.0, 50.0], [120.0, 60.0, 40.0, 60.0]]] {
            let (plan, catalog, view, ids) = fixture(points);
            for ids in [ids.clone(), [ids[1].clone(), ids[0].clone()]] {
                let result = run(&plan, &catalog, &view, &ids).unwrap();
                assert_eq!(result.normal_separation_pt, 10.0);
                assert_eq!(result.overlap_length_pt, 60.0);
                assert_eq!(result.projected_points_pt[0].x, 70.0);
                assert_eq!(result.projected_points_pt[1].x, 70.0);
            }
        }
    }

    #[test]
    fn preserves_sources_and_never_grants_review_or_measurement() {
        let (mut plan, catalog, view, ids) = fixture(base());
        let first = run(&plan, &catalog, &view, &ids).unwrap();
        assert_eq!(first.state, "documentary_proposal");
        assert!(!first.reviewed && !first.measured_on_photo);
        assert_eq!(serde_json::to_value(&first.feature_source).unwrap(), serde_json::to_value(&plan.characteristics[0].source).unwrap());
        assert_eq!(serde_json::to_value(&first.supports[0]).unwrap(), serde_json::to_value(&catalog.primitives[0]).unwrap());
        assert!(first.suspension_reasons.iter().any(|s| s.contains("roles_are_not_recognized")));
        plan.characteristics[0].nominal = Some(0.001);
        let current = define_view(&plan, "doc", &catalog, "Périmètre", &view.feature_ids, &view.geometry_ids).unwrap();
        let changed = run(&plan, &catalog, &current, &ids).unwrap();
        assert_eq!(first.normal_separation_pt, changed.normal_separation_pt);
        assert_eq!(first.overlap_length_pt, changed.overlap_length_pt);
        assert_ne!(first.feature_fingerprint, changed.feature_fingerprint);
    }

    #[test]
    fn refuses_collinear_nonparallel_disjoint_and_degenerate_supports() {
        for second in [[40.0, 50.0, 120.0, 50.0], [40.0, 60.0, 120.0, 70.0],
            [101.0, 60.0, 120.0, 60.0], [100.0, 60.0, 120.0, 60.0]] {
            let (plan, catalog, view, ids) = fixture([base()[0], second]);
            assert!(run(&plan, &catalog, &view, &ids).is_err());
        }
        let (plan, mut catalog, view, ids) = fixture(base());
        catalog.primitives[1].points[1] = catalog.primitives[1].points[0];
        let current = define_view(&plan, "doc", &catalog, "Périmètre", &view.feature_ids, &view.geometry_ids).unwrap();
        assert!(run(&plan, &catalog, &current, &ids).is_err());
    }

    #[test]
    fn refuses_scope_mismatch_stale_sources_and_duplicate_supports() {
        let (plan, mut catalog, view, ids) = fixture(base());
        assert!(run(&plan, &catalog, &view, &[ids[0].clone(), ids[0].clone()]).is_err());
        let scoped = define_view(&plan, "doc", &catalog, "Autre", &view.feature_ids, &[ids[0].clone()]).unwrap();
        assert!(run(&plan, &catalog, &scoped, &ids).is_err());
        catalog.primitives[1].source.path_data.push(' ');
        assert!(run(&plan, &catalog, &view, &ids).is_err());
        assert!(inspect(&plan, "changed", &catalog, &view, &plan.characteristics[0].id, &ids, "Justification").is_err());
    }

    #[test]
    fn refuses_other_dimension_kinds_and_documentary_unit_conflicts() {
        let (mut plan, catalog, view, ids) = fixture(base());
        for kind in [DrawingDimensionKind::Angle, DrawingDimensionKind::Depth, DrawingDimensionKind::Diameter] {
            plan.characteristics[0].kind = kind;
            let current = define_view(&plan, "doc", &catalog, "Périmètre", &view.feature_ids, &view.geometry_ids).unwrap();
            assert!(run(&plan, &catalog, &current, &ids).is_err());
        }
        plan.characteristics[0].kind = DrawingDimensionKind::Linear;
        plan.unit_context = parse_text_plan("Units mm\nUnits in\nLongueur 999 mm", "test.pdf").unit_context;
        let current = define_view(&plan, "doc", &catalog, "Périmètre", &view.feature_ids, &view.geometry_ids).unwrap();
        assert!(run(&plan, &catalog, &current, &ids).is_err());
    }

    #[test]
    fn refuses_non_line_shapes_and_invalid_justification() {
        let (plan, catalog, view, ids) = fixture(base());
        for variant in 0..3 {
            let mut changed = catalog.clone();
            let primitive = &mut changed.primitives[0];
            match variant {
                0 => primitive.kind = VectorPrimitiveKind::Polyline,
                1 => primitive.closed = true,
                _ => primitive.points.push(primitive.points[0]),
            }
            let current = define_view(&plan, "doc", &changed, "Périmètre", &view.feature_ids, &view.geometry_ids).unwrap();
            assert!(run(&plan, &changed, &current, &ids).is_err());
        }
        for justification in [" ".to_string(), "a".repeat(2001)] {
            assert!(inspect(&plan, "doc", &catalog, &view, &plan.characteristics[0].id, &ids, &justification).is_err());
        }
    }

    #[test]
    fn suspends_absent_unit_but_refuses_degree_and_nonobservable_feature() {
        let (mut plan, catalog, view, ids) = fixture(base());
        plan.characteristics[0].unit = None;
        let current = define_view(&plan, "doc", &catalog, "Périmètre", &view.feature_ids, &view.geometry_ids).unwrap();
        let result = run(&plan, &catalog, &current, &ids).unwrap();
        assert!(result.suspension_reasons.iter().any(|reason| reason == "feature_unit_absent"));
        plan.characteristics[0].unit = Some(DrawingUnit::Degree);
        let current = define_view(&plan, "doc", &catalog, "Périmètre", &view.feature_ids, &view.geometry_ids).unwrap();
        assert!(run(&plan, &catalog, &current, &ids).is_err());
        plan.characteristics[0].unit = Some(DrawingUnit::Millimeter);
        plan.characteristics[0].measurability = DrawingMeasurability::NotMeasurable2d;
        let current = define_view(&plan, "doc", &catalog, "Périmètre", &view.feature_ids, &view.geometry_ids).unwrap();
        assert!(run(&plan, &catalog, &current, &ids).is_err());
    }
}
