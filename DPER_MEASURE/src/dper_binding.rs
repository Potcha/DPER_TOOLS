//! Explicit, reviewable links from localized drawing text to vector proposals.
//!
//! Proximity ranks hypotheses only. It neither identifies a physical view nor
//! confirms a dimension. Nominals never filter radii or establish an image scale.
use crate::{
    dper_drawing::{
        DrawingCharacteristic, DrawingControlPlan, DrawingDimensionKind, DrawingMeasurability,
        DrawingUnit,
    },
    dper_pdf_geometry::{SvgGeometryDocument, VectorPrimitive, VectorPrimitiveKind},
    dper_pdf_layout::PdfBox,
    dper_vision::InspectionGeometry,
    geometry::Point,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    f64::consts::TAU,
};

const MAX_VIEW_ITEMS: usize = 1_000;
const MAX_CATALOG_ITEMS: usize = 100_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DrawingView {
    pub id: String,
    pub page: u32,
    pub label: String,
    pub feature_ids: Vec<String>,
    pub geometry_ids: Vec<String>,
    pub bounds: PdfBox,
    pub document_fingerprint: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BindingCandidate {
    pub id: String,
    pub feature_id: String,
    pub geometry_id: String,
    pub page: u32,
    pub view_id: String,
    pub document_fingerprint: String,
    pub feature_fingerprint: String,
    pub primitive_fingerprint: String,
    pub rank: u32,
    pub distance_pt: f64,
    pub evidence: Vec<String>,
    pub suspension_reasons: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BindingSelection {
    pub candidate: BindingCandidate,
    pub justification: String,
    /// Only a current-session UI review may set this. JSON cannot grant review.
    #[serde(skip_deserializing, default)]
    pub reviewed: bool,
    pub snapshot: VectorPrimitive,
}

/// Stable FNV-1a for detecting source changes, not a cryptographic signature.
pub fn fingerprint_bytes(bytes: &[u8]) -> String {
    crate::dper_source::fingerprint_bytes(bytes)
}

fn fingerprint<T: Serialize + ?Sized>(value: &T) -> Result<String, String> {
    serde_json::to_vec(value)
        .map(|bytes| fingerprint_bytes(&bytes))
        .map_err(|error| format!("Impossible d'identifier la source de l'association : {error}"))
}

pub(crate) fn feature_fingerprint(feature: &DrawingCharacteristic) -> Result<String, String> {
    // Review and a native measurement association are UI states, not changes in
    // drawing source. All source and interpretation fields remain fingerprinted.
    let mut value = serde_json::to_value(feature).map_err(|error| error.to_string())?;
    if let Some(object) = value.as_object_mut() {
        object.remove("confirmed");
        object.remove("measurement_id");
    }
    fingerprint(&value)
}

fn primitive_fingerprint(primitive: &VectorPrimitive) -> Result<String, String> {
    let mut value = serde_json::to_value(primitive).map_err(|error| error.to_string())?;
    if let Some(object) = value.as_object_mut() {
        object.remove("confirmed");
    }
    fingerprint(&value)
}

fn valid_box(bounds: PdfBox) -> bool {
    [bounds.x_min, bounds.y_min, bounds.x_max, bounds.y_max]
        .iter()
        .all(|value| value.is_finite() && value.abs() <= 1e9)
        && bounds.x_min <= bounds.x_max
        && bounds.y_min <= bounds.y_max
}

fn on_page(bounds: PdfBox, catalog: &SvgGeometryDocument) -> bool {
    let epsilon = 1e-6;
    valid_box(bounds)
        && bounds.x_min >= -epsilon
        && bounds.y_min >= -epsilon
        && bounds.x_max <= catalog.width_pt + epsilon
        && bounds.y_max <= catalog.height_pt + epsilon
}

fn union(left: PdfBox, right: PdfBox) -> PdfBox {
    PdfBox {
        x_min: left.x_min.min(right.x_min),
        y_min: left.y_min.min(right.y_min),
        x_max: left.x_max.max(right.x_max),
        y_max: left.y_max.max(right.y_max),
    }
}

fn unique_ids(ids: &[String], what: &str) -> Result<Vec<String>, String> {
    if ids.is_empty() || ids.len() > MAX_VIEW_ITEMS || ids.iter().any(|id| id.trim().is_empty()) {
        return Err(format!(
            "La vue demande de 1 à {MAX_VIEW_ITEMS} identifiants {what} non vides."
        ));
    }
    let unique = ids.iter().cloned().collect::<BTreeSet<_>>();
    if unique.len() != ids.len() {
        return Err(format!("Identifiants {what} dupliqués dans la vue."));
    }
    Ok(unique.into_iter().collect())
}

fn feature_index(
    plan: &DrawingControlPlan,
) -> Result<BTreeMap<&str, &DrawingCharacteristic>, String> {
    let mut result = BTreeMap::new();
    for feature in &plan.characteristics {
        if feature.id.trim().is_empty() || result.insert(feature.id.as_str(), feature).is_some() {
            return Err("Identifiant de caractéristique vide ou ambigu dans le plan.".into());
        }
    }
    Ok(result)
}

fn primitive_index(
    catalog: &SvgGeometryDocument,
) -> Result<BTreeMap<&str, &VectorPrimitive>, String> {
    if catalog.primitives.len() > MAX_CATALOG_ITEMS {
        return Err("Catalogue vectoriel supérieur au budget d'association.".into());
    }
    let mut result = BTreeMap::new();
    for primitive in &catalog.primitives {
        if primitive.id.trim().is_empty()
            || result.insert(primitive.id.as_str(), primitive).is_some()
        {
            return Err("Identifiant de primitive vide ou ambigu dans le catalogue.".into());
        }
    }
    Ok(result)
}

/// Define an explicit page-local scope. This does not identify a physical view.
/// Every selected text and vector box contributes to the returned scope bounds.
pub fn define_view(
    plan: &DrawingControlPlan,
    docfp: &str,
    catalog: &SvgGeometryDocument,
    label: &str,
    feature_ids: &[String],
    geometry_ids: &[String],
) -> Result<DrawingView, String> {
    if docfp.trim().is_empty()
        || docfp.len() > 256
        || label.trim().is_empty()
        || label.chars().count() > 200
    {
        return Err("Le document source et le libellé de vue explicite sont obligatoires.".into());
    }
    if catalog.page == 0
        || !catalog.width_pt.is_finite()
        || !catalog.height_pt.is_finite()
        || catalog.width_pt <= 0.0
        || catalog.height_pt <= 0.0
        || !valid_box(catalog.view_box)
        || !catalog.flattening_tolerance_pt.is_finite()
        || catalog.flattening_tolerance_pt <= 0.0
    {
        return Err("Repère de page vectorielle invalide.".into());
    }
    let pages = plan
        .page_geometry
        .iter()
        .filter(|page| page.page == catalog.page)
        .collect::<Vec<_>>();
    if pages.len() != 1
        || !pages[0].width_pt.is_finite()
        || !pages[0].height_pt.is_finite()
        || (pages[0].width_pt - catalog.width_pt).abs() > 1e-5
        || (pages[0].height_pt - catalog.height_pt).abs() > 1e-5
    {
        return Err(
            "Le texte et les primitives doivent partager un même repère de page PDF explicite."
                .into(),
        );
    }
    let feature_ids = unique_ids(feature_ids, "de caractéristiques")?;
    let geometry_ids = unique_ids(geometry_ids, "de géométries")?;
    let features = feature_index(plan)?;
    let primitives = primitive_index(catalog)?;
    let mut bounds = None;
    for id in &feature_ids {
        let feature = features
            .get(id.as_str())
            .ok_or("Caractéristique de la vue absente du plan.")?;
        let bbox = feature
            .source
            .pdf_bbox
            .ok_or("La caractéristique n'a pas de boîte source PDF localisée.")?;
        if feature.source.page != Some(catalog.page)
            || !on_page(bbox, catalog)
            || bbox.x_min == bbox.x_max
            || bbox.y_min == bbox.y_max
            || feature.source.exact_text.trim().is_empty()
            || feature.source.method.trim().is_empty()
        {
            return Err("La caractéristique doit posséder une source textuelle localisée sur la page de la vue.".into());
        }
        bounds = Some(bounds.map_or(bbox, |previous| union(previous, bbox)));
    }
    for id in &geometry_ids {
        let primitive = primitives
            .get(id.as_str())
            .ok_or("Géométrie de la vue absente du catalogue.")?;
        if primitive.page != catalog.page
            || !on_page(primitive.bbox, catalog)
            || primitive.source.path_data.trim().is_empty()
            || primitive.source.element_name.trim().is_empty()
            || primitive
                .source
                .transform
                .iter()
                .any(|value| !value.is_finite())
        {
            return Err(
                "La primitive doit posséder une source vectorielle valide sur la page de la vue."
                    .into(),
            );
        }
        bounds = Some(bounds.map_or(primitive.bbox, |previous| union(previous, primitive.bbox)));
    }
    let bounds = bounds.ok_or("Vue vide.")?;
    let label = label.trim().to_owned();
    let id = format!(
        "view-{}",
        fingerprint(&(
            docfp,
            catalog.page,
            &label,
            &feature_ids,
            &geometry_ids,
            bounds
        ))?
    );
    Ok(DrawingView {
        id,
        page: catalog.page,
        label,
        feature_ids,
        geometry_ids,
        bounds,
        document_fingerprint: docfp.into(),
    })
}

fn validate_view(
    plan: &DrawingControlPlan,
    docfp: &str,
    catalog: &SvgGeometryDocument,
    view: &DrawingView,
) -> Result<(), String> {
    if view.document_fingerprint != docfp || view.page != catalog.page {
        return Err("La vue appartient à une autre page ou version du document.".into());
    }
    let current = define_view(
        plan,
        docfp,
        catalog,
        &view.label,
        &view.feature_ids,
        &view.geometry_ids,
    )?;
    if current.id != view.id || current.bounds != view.bounds {
        return Err(
            "Le périmètre ou les sources de la vue ont changé ; redéfinissez la vue.".into(),
        );
    }
    Ok(())
}

fn circle_geometry(primitive: &VectorPrimitive) -> Result<InspectionGeometry, String> {
    let circle = primitive
        .circle
        .as_ref()
        .ok_or("La primitive ne porte pas d'ajustement circulaire.")?;
    if primitive.kind != VectorPrimitiveKind::CircleCandidate
        || !primitive.closed
        || primitive.points.len() < 24
        || circle.sample_count < 48
        || !circle.center.x.is_finite()
        || !circle.center.y.is_finite()
        || !circle.radius_pt.is_finite()
        || circle.radius_pt <= 0.0
        || !circle.angular_coverage_rad.is_finite()
        || circle.angular_coverage_rad < TAU * 0.90
        || circle.angular_coverage_rad > TAU + 1e-9
        || !circle.rms_residual_pt.is_finite()
        || circle.rms_residual_pt < 0.0
        || !circle.max_residual_pt.is_finite()
        || circle.max_residual_pt < circle.rms_residual_pt
        || circle.rms_residual_pt > circle.radius_pt * 0.0025
        || circle.max_residual_pt > circle.radius_pt * 0.005
        || primitive
            .points
            .iter()
            .any(|point| !point.x.is_finite() || !point.y.is_finite())
    {
        return Err("Un cercle complet fermé, bien réparti et à résidus bornés est requis ; arc et supports isolés non pris en charge.".into());
    }
    Ok(InspectionGeometry::Circle {
        center: circle.center,
        radius: circle.radius_pt,
    })
}

fn point_box_distance(point: Point, bounds: PdfBox) -> f64 {
    let dx = (bounds.x_min - point.x)
        .max(0.0)
        .max(point.x - bounds.x_max);
    let dy = (bounds.y_min - point.y)
        .max(0.0)
        .max(point.y - bounds.y_max);
    dx.hypot(dy)
}

/// List every compatible alternative in the explicitly selected scope.
/// Deterministic ranks are distances to text boxes, never confirmed relations.
pub fn binding_candidates(
    plan: &DrawingControlPlan,
    docfp: &str,
    catalog: &SvgGeometryDocument,
    view: &DrawingView,
    feature_id: &str,
) -> Result<Vec<BindingCandidate>, String> {
    validate_view(plan, docfp, catalog, view)?;
    if !view.feature_ids.iter().any(|id| id == feature_id) {
        return Err("La caractéristique n'appartient pas à cette vue explicite.".into());
    }
    let features = feature_index(plan)?;
    let feature = features.get(feature_id).ok_or("Caractéristique absente.")?;
    if feature.measurability == DrawingMeasurability::NotMeasurable2d
        || !matches!(
            feature.kind,
            DrawingDimensionKind::Radius | DrawingDimensionKind::Diameter
        )
        || feature.unit == Some(DrawingUnit::Degree)
    {
        return Err("Association non prise en charge : seules les cotes rayon/diamètre vers un cercle complet sont proposées ; une longueur exige ses deux supports et une méthode dédiée.".into());
    }
    let feature_fingerprint = feature_fingerprint(feature)?;
    let text_box = feature.source.pdf_bbox.ok_or("Boîte source PDF absente.")?;
    let primitives = primitive_index(catalog)?;
    let mut result = Vec::new();
    for id in &view.geometry_ids {
        let primitive = primitives[id.as_str()];
        if circle_geometry(primitive).is_err() {
            continue;
        }
        let primitive_fingerprint = primitive_fingerprint(primitive)?;
        let distance_pt = primitive
            .points
            .iter()
            .map(|point| point_box_distance(*point, text_box))
            .fold(f64::INFINITY, f64::min);
        if !distance_pt.is_finite() {
            continue;
        }
        let id = format!(
            "binding-{}",
            fingerprint(&(
                docfp,
                &view.id,
                feature_id,
                &primitive.id,
                &feature_fingerprint,
                &primitive_fingerprint
            ))?
        );
        let mut suspension_reasons = vec![
            "proximity_is_only_a_hypothesis".into(),
            "physical_view_and_planar_observability_require_review".into(),
            "binding_requires_explicit_review".into(),
        ];
        if catalog.unsupported_path_count > 0 || !catalog.warnings.is_empty() {
            suspension_reasons.push("vector_extraction_has_limitations".into());
        }
        if feature.repetition.is_some_and(|count| count > 1) {
            suspension_reasons.push("repeated_feature_instances_require_explicit_selection".into());
        }
        result.push(BindingCandidate {
            id,
            feature_id: feature_id.into(),
            geometry_id: primitive.id.clone(),
            page: catalog.page,
            view_id: view.id.clone(),
            document_fingerprint: docfp.into(),
            feature_fingerprint: feature_fingerprint.clone(),
            primitive_fingerprint,
            rank: 0,
            distance_pt,
            evidence: vec![
                "same_pdf_page".into(),
                "explicit_scope_membership".into(),
                "compatible_closed_circle_fit".into(),
                format!("source_word_ids:{}", feature.source.word_ids.join(",")),
                format!(
                    "vector_source_element:{};subpath:{}",
                    primitive.source.element_index, primitive.source.subpath_index
                ),
            ],
            suspension_reasons,
        });
    }
    result.sort_by(|left, right| {
        left.distance_pt
            .total_cmp(&right.distance_pt)
            .then_with(|| left.geometry_id.cmp(&right.geometry_id))
    });
    let multiple = result.len() > 1;
    for (index, candidate) in result.iter_mut().enumerate() {
        candidate.rank = index as u32 + 1;
        if multiple {
            candidate
                .suspension_reasons
                .push("alternative_geometries_are_retained".into());
        }
    }
    Ok(result)
}

/// Select a known candidate with an explicit source justification. No auto-review.
pub fn select_binding(
    candidates: &[BindingCandidate],
    candidate_id: &str,
    catalog: &SvgGeometryDocument,
    justification: &str,
) -> Result<BindingSelection, String> {
    if justification.trim().is_empty() || justification.chars().count() > 2_000 {
        return Err("Le choix doit citer une justification/source de 1 à 2 000 caractères.".into());
    }
    if candidates.len() > MAX_VIEW_ITEMS {
        return Err("Trop de candidats d'association.".into());
    }
    let mut matching = candidates
        .iter()
        .filter(|candidate| candidate.id == candidate_id);
    let candidate = matching.next().ok_or("Candidat d'association absent.")?;
    if candidate_id.trim().is_empty() || matching.next().is_some() {
        return Err("Identifiant de candidat vide ou ambigu.".into());
    }
    let primitives = primitive_index(catalog)?;
    let primitive = primitives
        .get(candidate.geometry_id.as_str())
        .ok_or("Primitive du candidat absente du catalogue.")?;
    if candidate.page != catalog.page
        || primitive.page != candidate.page
        || primitive_fingerprint(primitive)? != candidate.primitive_fingerprint
    {
        return Err("La primitive du candidat a changé depuis sa proposition.".into());
    }
    circle_geometry(primitive)?;
    Ok(BindingSelection {
        candidate: candidate.clone(),
        justification: justification.trim().into(),
        reviewed: false,
        snapshot: (*primitive).clone(),
    })
}

/// Resolve a proposed link against freshly extracted, current source data.
/// The caller must load `catalog` from the actual document, not a client snapshot.
/// Resolution never grants review; callers must suspend association validation
/// and conformity while `selection.reviewed` is false.
/// Serialized numeric snapshots may round on JSON reload. Their strict identity
/// is not relaxed: renew such a selection from freshly extracted candidates.
pub fn resolve_binding(
    plan: &DrawingControlPlan,
    docfp: &str,
    catalog: &SvgGeometryDocument,
    view: &DrawingView,
    selection: &BindingSelection,
) -> Result<InspectionGeometry, String> {
    if selection.justification.trim().is_empty() || selection.justification.chars().count() > 2_000
    {
        return Err("Justification de l'association absente ou excessive.".into());
    }
    let candidates =
        binding_candidates(plan, docfp, catalog, view, &selection.candidate.feature_id)?;
    let current = candidates
        .iter()
        .find(|candidate| candidate.id == selection.candidate.id)
        .ok_or("Les sources de l'association ont changé ; recommencez le choix et sa revue.")?;
    if fingerprint(current)? != fingerprint(&selection.candidate)?
        || primitive_fingerprint(&selection.snapshot)? != current.primitive_fingerprint
        || selection.snapshot.id != current.geometry_id
        || selection.snapshot.page != current.page
    {
        return Err(
            "L'association ou son instantané ne correspond plus aux sources actuelles.".into(),
        );
    }
    let primitives = primitive_index(catalog)?;
    let primitive = primitives
        .get(current.geometry_id.as_str())
        .ok_or("Primitive associée absente.")?;
    circle_geometry(primitive)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        dper_drawing::parse_text_plan, dper_pdf_geometry::parse_svg_geometry,
        dper_pdf_layout::PdfPageGeometry,
    };

    fn circle_path(center: Point, radius: f64) -> String {
        let mut path = String::new();
        for index in 0..128 {
            let angle = TAU * index as f64 / 128.0;
            path.push_str(&format!(
                "{} {} {} ",
                if index == 0 { "M" } else { "L" },
                center.x + radius * angle.cos(),
                center.y + radius * angle.sin()
            ));
        }
        path.push('Z');
        format!(r#"<path d="{path}"/>"#)
    }

    fn fixture() -> (DrawingControlPlan, SvgGeometryDocument) {
        let mut plan = parse_text_plan("Ø999 mm", "test.pdf");
        plan.page_geometry = vec![PdfPageGeometry {
            page: 1,
            width_pt: 240.0,
            height_pt: 180.0,
        }];
        let feature = &mut plan.characteristics[0];
        feature.source.page = Some(1);
        feature.source.pdf_bbox = Some(PdfBox {
            x_min: 114.0,
            y_min: 18.0,
            x_max: 126.0,
            y_max: 26.0,
        });
        feature.source.word_ids = vec!["word-1".into()];
        let body = format!(
            "{}{}{}<path d=\"M 10 140 L 220 140\"/>",
            circle_path(Point { x: 60.0, y: 90.0 }, 20.0),
            circle_path(Point { x: 180.0, y: 90.0 }, 20.0),
            circle_path(Point { x: 60.0, y: 90.0 }, 12.0)
        );
        let svg =
            format!(r#"<svg width="240pt" height="180pt" viewBox="0 0 240 180">{body}</svg>"#);
        (plan, parse_svg_geometry(&svg, 1).unwrap())
    }

    fn view(plan: &DrawingControlPlan, catalog: &SvgGeometryDocument) -> DrawingView {
        define_view(
            plan,
            "document-a",
            catalog,
            "Vue explicitement choisie pour revue",
            &[plan.characteristics[0].id.clone()],
            &catalog
                .primitives
                .iter()
                .map(|primitive| primitive.id.clone())
                .collect::<Vec<_>>(),
        )
        .unwrap()
    }

    #[test]
    fn symmetric_and_concentric_alternatives_are_retained_without_nominal_filtering() {
        let (plan, catalog) = fixture();
        let view = view(&plan, &catalog);
        let candidates = binding_candidates(
            &plan,
            "document-a",
            &catalog,
            &view,
            &plan.characteristics[0].id,
        )
        .unwrap();
        assert_eq!(candidates.len(), 3);
        assert_eq!(
            candidates
                .iter()
                .map(|candidate| candidate.rank)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert_eq!(plan.characteristics[0].nominal, Some(999.0));
        for candidate in &candidates {
            assert!(candidate
                .suspension_reasons
                .contains(&"proximity_is_only_a_hypothesis".into()));
            assert!(candidate
                .suspension_reasons
                .contains(&"alternative_geometries_are_retained".into()));
        }
        let again = binding_candidates(
            &plan,
            "document-a",
            &catalog,
            &view,
            &plan.characteristics[0].id,
        )
        .unwrap();
        assert_eq!(
            fingerprint(&candidates).unwrap(),
            fingerprint(&again).unwrap()
        );
        assert_eq!(view.bounds.y_max, 140.0);
        assert!(view.bounds.y_min <= plan.characteristics[0].source.pdf_bbox.unwrap().y_min);
    }

    #[test]
    fn selection_requires_justification_and_ui_review_and_json_cannot_forge_review() {
        let (plan, catalog) = fixture();
        let view = view(&plan, &catalog);
        let candidates = binding_candidates(
            &plan,
            "document-a",
            &catalog,
            &view,
            &plan.characteristics[0].id,
        )
        .unwrap();
        assert!(select_binding(&candidates, &candidates[0].id, &catalog, " ").is_err());
        let mut selection = select_binding(
            &candidates,
            &candidates[0].id,
            &catalog,
            "Lecture explicite du rappel de cote sur cette vue ; cercle à contrôler.",
        )
        .unwrap();
        assert!(!selection.reviewed);
        assert!(resolve_binding(&plan, "document-a", &catalog, &view, &selection).is_ok());
        assert!(!selection.reviewed);
        selection.reviewed = true;
        assert!(matches!(
            resolve_binding(&plan, "document-a", &catalog, &view, &selection).unwrap(),
            InspectionGeometry::Circle { .. }
        ));
        let json = serde_json::to_string(&selection).unwrap();
        assert!(json.contains("\"reviewed\":true"));
        let forged: BindingSelection = serde_json::from_str(&json).unwrap();
        assert!(!forged.reviewed);
        assert_eq!(forged.candidate.id, selection.candidate.id);
        assert_eq!(
            forged.candidate.feature_fingerprint,
            selection.candidate.feature_fingerprint
        );
        assert_eq!(
            forged.candidate.primitive_fingerprint,
            selection.candidate.primitive_fingerprint
        );
        // A historical JSON snapshot is not a new extraction: its numeric values
        // may have rounded on deserialization. Do not weaken source equality to
        // make that snapshot usable. Renew the proposal from current sources.
        let renewed = select_binding(
            &candidates,
            &forged.candidate.id,
            &catalog,
            &forged.justification,
        )
        .unwrap();
        assert!(resolve_binding(&plan, "document-a", &catalog, &view, &renewed).is_ok());
        assert!(!renewed.reviewed);
        assert!(!forged.reviewed);
    }

    #[test]
    fn resolution_rejects_changed_document_feature_primitive_snapshot_and_view() {
        let (mut plan, catalog) = fixture();
        let view = view(&plan, &catalog);
        let candidates = binding_candidates(
            &plan,
            "document-a",
            &catalog,
            &view,
            &plan.characteristics[0].id,
        )
        .unwrap();
        let mut selection = select_binding(
            &candidates,
            &candidates[0].id,
            &catalog,
            "Repère source vérifié.",
        )
        .unwrap();
        selection.reviewed = true;
        assert!(resolve_binding(&plan, "document-b", &catalog, &view, &selection).is_err());
        let original = plan.characteristics[0].source.exact_text.clone();
        plan.characteristics[0]
            .source
            .exact_text
            .push_str(" source modifiée");
        assert!(resolve_binding(&plan, "document-a", &catalog, &view, &selection).is_err());
        plan.characteristics[0].source.exact_text = original;
        let mut changed = catalog.clone();
        changed
            .primitives
            .iter_mut()
            .find(|primitive| primitive.id == selection.candidate.geometry_id)
            .unwrap()
            .source
            .path_data
            .push(' ');
        assert!(resolve_binding(&plan, "document-a", &changed, &view, &selection).is_err());
        let mut changed_selection = selection.clone();
        changed_selection
            .snapshot
            .circle
            .as_mut()
            .unwrap()
            .radius_pt += 1.0;
        assert!(resolve_binding(&plan, "document-a", &catalog, &view, &changed_selection).is_err());
        let mut shifted_snapshot = selection.clone();
        let original_x = shifted_snapshot.snapshot.points[0].x;
        shifted_snapshot.snapshot.points[0].x = f64::from_bits(original_x.to_bits() + 1);
        assert_ne!(
            primitive_fingerprint(&shifted_snapshot.snapshot).unwrap(),
            primitive_fingerprint(&selection.snapshot).unwrap()
        );
        assert!(resolve_binding(&plan, "document-a", &catalog, &view, &shifted_snapshot).is_err());
        let mut changed_view = view.clone();
        changed_view.label.push_str(" autre");
        assert!(resolve_binding(&plan, "document-a", &catalog, &changed_view, &selection).is_err());
        // Human source confirmation does not mutate the original drawing source.
        plan.characteristics[0].confirmed = true;
        assert!(resolve_binding(&plan, "document-a", &catalog, &view, &selection).is_ok());
    }

    #[test]
    fn unsupported_linear_depth_open_and_poor_circle_supports_do_not_resolve() {
        let (mut plan, catalog) = fixture();
        for kind in [
            DrawingDimensionKind::Linear,
            DrawingDimensionKind::Depth,
            DrawingDimensionKind::Angle,
        ] {
            plan.characteristics[0].kind = kind;
            let view = view(&plan, &catalog);
            assert!(binding_candidates(
                &plan,
                "document-a",
                &catalog,
                &view,
                &plan.characteristics[0].id
            )
            .is_err());
        }
        plan.characteristics[0].kind = DrawingDimensionKind::Radius;
        let mut bad = catalog.clone();
        for primitive in &mut bad.primitives {
            if let Some(circle) = primitive.circle.as_mut() {
                circle.angular_coverage_rad = TAU * 0.5;
            }
        }
        let view = view(&plan, &bad);
        assert!(binding_candidates(
            &plan,
            "document-a",
            &bad,
            &view,
            &plan.characteristics[0].id
        )
        .unwrap()
        .is_empty());
        for primitive in &mut bad.primitives {
            primitive.closed = false;
        }
        assert!(binding_candidates(
            &plan,
            "document-a",
            &bad,
            &view,
            &plan.characteristics[0].id
        )
        .unwrap()
        .is_empty());
    }

    #[test]
    fn duplicate_ids_missing_locations_and_cross_page_scopes_are_refused() {
        let (mut plan, mut catalog) = fixture();
        let feature_id = plan.characteristics[0].id.clone();
        let geometry_id = catalog.primitives[0].id.clone();
        assert!(define_view(
            &plan,
            "document-a",
            &catalog,
            "vue",
            &[feature_id.clone(), feature_id.clone()],
            &[geometry_id.clone()]
        )
        .is_err());
        catalog.primitives.push(catalog.primitives[0].clone());
        assert!(define_view(
            &plan,
            "document-a",
            &catalog,
            "vue",
            &[feature_id.clone()],
            &[geometry_id.clone()]
        )
        .is_err());
        catalog.primitives.pop();
        plan.characteristics.push(plan.characteristics[0].clone());
        assert!(define_view(
            &plan,
            "document-a",
            &catalog,
            "vue",
            &[feature_id.clone()],
            &[geometry_id.clone()]
        )
        .is_err());
        plan.characteristics.pop();
        plan.characteristics[0].source.page = Some(2);
        assert!(define_view(
            &plan,
            "document-a",
            &catalog,
            "vue",
            &[feature_id.clone()],
            &[geometry_id.clone()]
        )
        .is_err());
        plan.characteristics[0].source.page = Some(1);
        plan.characteristics[0].source.pdf_bbox = None;
        assert!(define_view(
            &plan,
            "document-a",
            &catalog,
            "vue",
            &[feature_id],
            &[geometry_id]
        )
        .is_err());
    }

    #[test]
    fn duplicate_candidate_ids_cannot_select_an_arbitrary_snapshot() {
        let (plan, catalog) = fixture();
        let view = view(&plan, &catalog);
        let mut candidates = binding_candidates(
            &plan,
            "document-a",
            &catalog,
            &view,
            &plan.characteristics[0].id,
        )
        .unwrap();
        candidates.push(candidates[0].clone());
        assert!(
            select_binding(&candidates, &candidates[0].id, &catalog, "Repère vérifié.").is_err()
        );
        assert_ne!(fingerprint_bytes(b"one"), fingerprint_bytes(b"two"));
        assert_eq!(fingerprint_bytes(b""), "fnv1a64:cbf29ce484222325");
    }
}
