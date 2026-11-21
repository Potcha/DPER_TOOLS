//! Explicit, reviewable links from localized drawing text to vector proposals.
//!
//! Proximity and bounded endpoint attachments rank hypotheses only. Neither
//! identifies a physical view or a leader/arrow symbol, nor confirms a dimension.
//! Nominals never filter radii or establish an image scale.
use crate::{
    dper_drawing::{
        declared_repetition, DrawingCharacteristic, DrawingControlPlan, DrawingDimensionKind, DrawingMeasurability,
        DrawingUnit, DrawingUnitStatus,
    },
    dper_pdf_geometry::{SvgGeometryDocument, VectorPrimitive, VectorPrimitiveKind},
    dper_pdf_layout::PdfBox,
    dper_vision::InspectionGeometry,
    geometry::Point,
};
use rmcp::schemars;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    f64::consts::TAU,
};

const MAX_VIEW_ITEMS: usize = 1_000;
const MAX_CATALOG_ITEMS: usize = 100_000;
pub(crate) const SHARED_CALLOUT_SOURCE: &str = "endpoint_attachment_abstained_due_to_shared_text_source";

#[path = "drawing_leader_hypotheses.rs"]
mod drawing_leader_hypotheses;

#[path = "drawing_linear_supports.rs"]
pub(crate) mod drawing_linear_supports;

#[path = "drawing_angular_supports.rs"]
pub(crate) mod drawing_angular_supports;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AuxiliaryRole {
    Axis,
    SectionMarker,
    Construction,
}

impl AuxiliaryRole {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Axis => "Axe",
            Self::SectionMarker => "Repère de coupe",
            Self::Construction => "Trait de construction",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DrawingView {
    pub id: String,
    pub page: u32,
    pub label: String,
    pub feature_ids: Vec<String>,
    pub geometry_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub auxiliary_roles: std::collections::BTreeMap<String, AuxiliaryRole>,
    pub bounds: PdfBox,
    pub document_fingerprint: String,
    /// Missing on historical projects: never infer current provenance on load.
    #[serde(default)]
    pub source_proof: Option<ViewSourceProof>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ViewSourceProof {
    pub schema_version: u32,
    pub fingerprint: String,
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

/// A documentary interpretation saved with the evidence returned by the PDF diagnostic.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SupportAssociation {
    pub feature_id: String,
    pub view_id: String,
    pub page: u32,
    pub support_ids: [String; 2],
    pub angular_sector: Option<drawing_angular_supports::AngularSector>,
    pub evidence: serde_json::Value,
    #[serde(skip)]
    pub(crate) selected_in_session: bool,
}

pub(crate) const MAX_OCCURRENCES: u32 = 64;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OccurrenceAssignment {
    pub feature_id: String,
    pub view_id: String,
    pub page: u32,
    pub document_fingerprint: String,
    pub justification: String,
    pub members: Vec<OccurrenceMember>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub representations: Vec<OccurrenceRepresentation>,
    #[serde(skip)]
    pub(crate) selected_in_session: bool,
}

impl OccurrenceAssignment {
    pub(crate) fn primary_assignment(&self) -> Self {
        Self { representations: Vec::new(), ..self.clone() }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, schemars::JsonSchema)]
pub struct OccurrenceRepresentationInput {
    pub occurrence_number: u32,
    pub view_id: String,
    pub feature_id: String,
    pub candidate_id: String,
    /// Cite the detail reference or drawing annotation linking the two representations.
    pub justification: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct OccurrenceRepresentation {
    pub source: OccurrenceRepresentationInput,
    pub page: u32,
    pub geometry_id: String,
    pub candidate_fingerprint: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OccurrenceMember {
    pub id: String,
    pub number: u32,
    pub geometry_id: String,
    pub candidate_id: String,
    pub candidate_fingerprint: String,
}

pub(crate) fn assign_occurrences(
    plan: &DrawingControlPlan,
    docfp: &str,
    catalog: &SvgGeometryDocument,
    view: &DrawingView,
    feature_id: &str,
    candidate_ids: &[String],
    justification: &str,
) -> Result<OccurrenceAssignment, String> {
    let features = feature_index(plan)?;
    let feature = features.get(feature_id).ok_or("Caractéristique absente.")?;
    let count = declared_repetition(feature)
        .filter(|count| (2..=MAX_OCCURRENCES).contains(count))
        .ok_or("Une cote répétée de 2 à 64 occurrences est requise.")?;
    if candidate_ids.len() != count as usize {
        return Err(format!("Sélectionnez exactement {count} occurrences dans l'ordre voulu."));
    }
    if justification.trim().is_empty() || justification.chars().count() > 2_000 {
        return Err("La justification doit contenir de 1 à 2 000 caractères.".into());
    }
    let candidates = binding_candidates(plan, docfp, catalog, view, feature_id)?;
    let primitives = primitive_index(catalog)?;
    let mut geometry_ids = BTreeSet::new();
    let mut centers: Vec<Point> = Vec::with_capacity(candidate_ids.len());
    let mut members = Vec::with_capacity(candidate_ids.len());
    for (index, candidate_id) in candidate_ids.iter().enumerate() {
        let candidate = candidates.iter().find(|candidate| &candidate.id == candidate_id)
            .ok_or("Candidat absent de la vue ou périmé.")?;
        if !geometry_ids.insert(candidate.geometry_id.as_str()) {
            return Err("Un même objet PDF ne peut occuper deux places du groupe.".into());
        }
        let InspectionGeometry::Circle { center, .. } =
            circle_geometry(primitives[candidate.geometry_id.as_str()])? else {
                return Err("Un cercle complet est requis pour chaque occurrence.".into());
            };
        // Multiple concentric contours can describe one physical location.
        if centers.iter().any(|other| (center.x - other.x).hypot(center.y - other.y) <= 0.01) {
            return Err("Deux cercles ont le même centre dans la vue PDF. Choisissez des emplacements distincts.".into());
        }
        centers.push(center);
        let number = index as u32 + 1;
        members.push(OccurrenceMember {
            id: format!("repetition:{feature_id}:{number}"),
            number,
            geometry_id: candidate.geometry_id.clone(),
            candidate_id: candidate.id.clone(),
            candidate_fingerprint: fingerprint(candidate)?,
        });
    }
    Ok(OccurrenceAssignment {
        feature_id: feature_id.into(),
        view_id: view.id.clone(),
        page: view.page,
        document_fingerprint: docfp.into(),
        justification: justification.trim().into(),
        members,
        representations: Vec::new(),
        selected_in_session: true,
    })
}

/// Record an explicitly evidenced second representation without adding a member.
pub(crate) fn add_occurrence_representation(
    assignment: &mut OccurrenceAssignment,
    plan: &DrawingControlPlan,
    catalog: &SvgGeometryDocument,
    view: &DrawingView,
    source: OccurrenceRepresentationInput,
) -> Result<(), String> {
    if source.view_id != view.id || view.id == assignment.view_id {
        return Err("La représentation alternative doit appartenir à une autre vue du même plan.".into());
    }
    if assignment.representations.len() >= MAX_OCCURRENCES as usize
        || !assignment.members.iter().any(|member| member.number == source.occurrence_number)
    {
        return Err("Occurrence absente du groupe ou limite de représentations atteinte.".into());
    }
    if source.justification.trim().is_empty() || source.justification.chars().count() > 2_000 {
        return Err("Citez le repère du plan qui relie les deux représentations.".into());
    }
    if assignment.representations.iter().any(|other|
        other.source.view_id == source.view_id && other.source.occurrence_number == source.occurrence_number)
    {
        return Err("Cette occurrence est déjà représentée dans cette vue.".into());
    }
    let candidates = binding_candidates(plan, &assignment.document_fingerprint, catalog, view, &source.feature_id)?;
    let candidate = candidates.iter().find(|candidate| candidate.id == source.candidate_id)
        .ok_or("Candidat du détail absent de la vue ou périmé.")?;
    let page = candidate.page;
    let geometry_id = candidate.geometry_id.clone();
    if (assignment.page == page && assignment.members.iter().any(|member| member.geometry_id == geometry_id))
        || assignment.representations.iter().any(|other| other.page == page && other.geometry_id == geometry_id)
    {
        return Err("Ce support du plan est déjà affecté à une occurrence.".into());
    }
    assignment.representations.push(OccurrenceRepresentation {
        source: OccurrenceRepresentationInput { justification: source.justification.trim().into(), ..source },
        page, geometry_id, candidate_fingerprint: fingerprint(candidate)?,
    });
    Ok(())
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
    let feature_sources = feature_ids.iter().map(|id|
        Ok((id, feature_fingerprint(features[id.as_str()])?))).collect::<Result<Vec<_>, String>>()?;
    let geometry_sources = geometry_ids.iter().map(|id|
        Ok((id, primitive_fingerprint(primitives[id.as_str()])?))).collect::<Result<Vec<_>, String>>()?;
    let mut warnings = catalog.warnings.clone();
    warnings.sort();
    let source_proof = Some(ViewSourceProof { schema_version: 1, fingerprint: fingerprint(&(
        "drawing_view_source_scope_v1", docfp, catalog.page,
        pages[0].width_pt, pages[0].height_pt, catalog.width_pt, catalog.height_pt,
        catalog.view_box, catalog.flattening_tolerance_pt, catalog.unsupported_path_count,
        warnings, feature_sources, geometry_sources,
    ))? });
    let id = format!(
        "view-{}",
        fingerprint(&(
            docfp,
            catalog.page,
            &label,
            &feature_ids,
            &geometry_ids,
            bounds,
            &source_proof
        ))?
    );
    Ok(DrawingView {
        id,
        page: catalog.page,
        label,
        feature_ids,
        geometry_ids,
        auxiliary_roles: Default::default(),
        bounds,
        document_fingerprint: docfp.into(),
        source_proof,
    })
}

pub(crate) fn define_view_with_roles(
    plan: &DrawingControlPlan,
    docfp: &str,
    catalog: &SvgGeometryDocument,
    label: &str,
    feature_ids: &[String],
    geometry_ids: &[String],
    roles: &std::collections::BTreeMap<String, AuxiliaryRole>,
) -> Result<DrawingView, String> {
    let mut view = define_view(plan, docfp, catalog, label, feature_ids, geometry_ids)?;
    if roles.is_empty() { return Ok(view); }
    if roles.keys().any(|id| !view.geometry_ids.contains(id)) {
        return Err("Chaque rôle auxiliaire doit désigner une géométrie de cette vue.".into());
    }
    let proof = view.source_proof.as_mut().unwrap();
    proof.fingerprint = fingerprint(&("drawing_view_auxiliary_roles_v1", &proof.fingerprint, roles))?;
    view.id = format!("view-{}", fingerprint(&(&view.id, &proof.fingerprint))?);
    view.auxiliary_roles = roles.clone();
    Ok(view)
}

pub(crate) fn validate_view(
    plan: &DrawingControlPlan,
    docfp: &str,
    catalog: &SvgGeometryDocument,
    view: &DrawingView,
) -> Result<(), String> {
    if view.source_proof.as_ref().is_none_or(|proof| proof.schema_version != 1 || proof.fingerprint.is_empty()) {
        return Err("Vue historique sans preuve de sources courante ; redéfinissez la vue et ses associations.".into());
    }
    if view.document_fingerprint != docfp || view.page != catalog.page {
        return Err("La vue appartient à une autre page ou version du document.".into());
    }
    let current = define_view_with_roles(
        plan,
        docfp,
        catalog,
        &view.label,
        &view.feature_ids,
        &view.geometry_ids,
        &view.auxiliary_roles,
    )?;
    if current.id != view.id || current.bounds != view.bounds || current.source_proof != view.source_proof {
        return Err(
            "Le périmètre ou les sources de la vue ont changé ; redéfinissez la vue.".into(),
        );
    }
    Ok(())
}

pub(crate) fn unique_view<'a>(views: &'a [DrawingView], view_id: &str) -> Result<&'a DrawingView, String> {
    let mut matches = views.iter().filter(|view| view.id == view_id);
    let view = matches.next().ok_or("Vue absente.")?;
    if matches.next().is_some() || views.iter().any(|other| other.id != view.id
        && other.feature_ids.iter().any(|id| view.feature_ids.contains(id))) {
        return Err("Vue ambiguë ou caractéristique revendiquée par plusieurs vues.".into());
    }
    Ok(view)
}

pub(crate) fn require_contour_supports(view: &DrawingView, supports: [&VectorPrimitive; 2]) -> Result<(), String> {
    if let Some(role) = supports.iter().find_map(|primitive| view.auxiliary_roles.get(&primitive.id)) {
        return Err(format!("{} déclaré dans cette vue : ce trait ne peut pas servir de contour physique.", role.label()));
    }
    if supports.into_iter().any(crate::drawing_scope_hypotheses::is_possible_auxiliary_line) {
        return Err("Un trait interrompu peut être un axe, un repère de coupe ou un autre trait auxiliaire. Choisissez des contours physiques continus dans la vue.".into());
    }
    Ok(())
}

pub(crate) fn circle_geometry(primitive: &VectorPrimitive) -> Result<InspectionGeometry, String> {
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
/// A unique bounded endpoint-attachment hypothesis may precede text proximity;
/// deterministic ranks never confirm a relation or recognize a leader symbol.
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
    let attachments = drawing_leader_hypotheses::attachment_hypotheses(
        catalog, view, &features, &primitives, feature_id,
    )?;
    let mut result = Vec::new();
    for id in &view.geometry_ids {
        let primitive = primitives[id.as_str()];
        if view.auxiliary_roles.contains_key(id) || circle_geometry(primitive).is_err() {
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
        let mut id = format!(
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
        let mut evidence = vec![
            "same_pdf_page".into(),
            "explicit_scope_membership".into(),
            "compatible_closed_circle_fit".into(),
            format!("source_word_ids:{}", feature.source.word_ids.join(",")),
            format!(
                "vector_source_element:{};subpath:{}",
                primitive.source.element_index, primitive.source.subpath_index
            ),
        ];
        let single_traces = attachments.by_geometry.get(&primitive.id);
        let chain_traces = attachments.chain_by_geometry.get(&primitive.id);
        let tip_traces = attachments.tip_by_geometry.get(&primitive.id);
        if single_traces.is_some() || chain_traces.is_some() || tip_traces.is_some() {
            if let Some(traces) = single_traces {
                for trace in traces {
                    evidence.push(format!("endpoint_attachment_hypothesis:{}",
                        serde_json::to_string(trace).map_err(|error| format!("Trace d'attachement impossible : {error}"))?));
                }
            }
            if let Some(traces) = chain_traces {
                for trace in traces {
                    evidence.push(format!("endpoint_attachment_chain_hypothesis:{}",
                        serde_json::to_string(trace).map_err(|error| format!("Trace de chaîne impossible : {error}"))?));
                }
            }
            if let Some(traces) = tip_traces {
                for trace in traces {
                    evidence.push(format!("geometric_triangular_tip_attachment_hypothesis:{}",
                        serde_json::to_string(trace).map_err(|error| format!("Trace de pointe impossible : {error}"))?));
                }
                suspension_reasons.push("triangular_tip_geometry_requires_explicit_review".into());
            }
            suspension_reasons.push("endpoint_attachment_hypothesis_requires_explicit_review".into());
            if attachments.preferred_geometry.as_deref() == Some(primitive.id.as_str()) {
                suspension_reasons.push("unique_endpoint_attachment_is_not_leader_or_arrow_recognition".into());
            } else {
                suspension_reasons.push("endpoint_attachment_ambiguity_is_retained".into());
            }
        }
        if let Some(reason) = attachments.abstention_reason {
            suspension_reasons.push(reason.into());
        }
        // A tip proposal records a complete scoped hypothesis. Bind its
        // evidence to the proposal identity so select_binding can reject an
        // edited alternative list or scope trace even before full resolution.
        if tip_traces.is_some() {
            id.push_str(&format!("-tip-{}", fingerprint(&evidence)?));
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
            evidence,
            suspension_reasons,
        });
    }
    result.sort_by(|left, right| {
        let preferred = attachments.preferred_geometry.as_deref();
        (preferred == Some(right.geometry_id.as_str()))
            .cmp(&(preferred == Some(left.geometry_id.as_str())))
            .then_with(|| left.distance_pt
            .total_cmp(&right.distance_pt)
            .then_with(|| left.geometry_id.cmp(&right.geometry_id)))
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
    let mut expected_id = format!("binding-{}", fingerprint(&(
        &candidate.document_fingerprint, &candidate.view_id, &candidate.feature_id,
        &candidate.geometry_id, &candidate.feature_fingerprint, &candidate.primitive_fingerprint,
    ))?);
    if candidate.evidence.iter().any(|line| line.starts_with("geometric_triangular_tip_attachment_hypothesis:")) {
        expected_id.push_str(&format!("-tip-{}", fingerprint(&candidate.evidence)?));
    }
    if candidate.id != expected_id {
        return Err("La proposition ou ses traces ont changé ; reproposez l'association et sa revue.".into());
    }
    drawing_leader_hypotheses::validate_support_evidence(candidate, &primitives)?;
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
    fn llt_pln_004_01_repeated_cote_keeps_two_identities_with_one_linked_detail() {
        let (plan, catalog, mut assignment, detail, source) = occurrence_detail_fixture();
        let primary = assignment.clone();
        let expected_nominal = plan.characteristics[0].nominal;
        assert_eq!(expected_nominal, Some(999.0));
        assert_eq!(declared_repetition(&plan.characteristics[0]), Some(2));
        assert_ne!(assignment.members[0].id, assignment.members[1].id);

        add_occurrence_representation(&mut assignment, &plan, &catalog, &detail, source).unwrap();
        assert_eq!(assignment.primary_assignment(), primary);
        let structure = crate::drawing_structure::structure_with_occurrences(&plan, &[&assignment]);
        let json = serde_json::to_value(structure).unwrap();
        assert_eq!(json["groups"][0]["expected_count"], 2);
        assert_eq!(json["groups"][0]["identified_occurrence_ids"].as_array().unwrap().len(), 2);
        assert_eq!(json["groups"][0]["representations"][0]["source"]["occurrence_number"], 1);
    }

    #[test]
    fn llt_pln_004_02_alternate_support_requires_evidence_and_cannot_claim_two_members() {
        let (plan, catalog, mut assignment, detail, source) = occurrence_detail_fixture();
        assert_eq!(plan.characteristics[0].nominal, plan.characteristics[1].nominal);
        assert_eq!(assignment.members.len(), 2);
        let mut unsubstantiated = source.clone();
        unsubstantiated.justification.clear();
        assert!(add_occurrence_representation(&mut assignment, &plan, &catalog, &detail, unsubstantiated).is_err());
        assert!(assignment.representations.is_empty());
        assert_eq!(assignment.members.len(), 2);

        add_occurrence_representation(&mut assignment, &plan, &catalog, &detail, source.clone()).unwrap();
        let proof = &assignment.representations[0];
        assert_eq!(proof.source.view_id, detail.id);
        assert_eq!(proof.geometry_id, catalog.primitives[2].id);
        assert_ne!(assignment.members[0].geometry_id, proof.geometry_id);
        assert!(!proof.candidate_fingerprint.is_empty());
        let mut conflicting = source;
        conflicting.occurrence_number = 2;
        assert!(add_occurrence_representation(&mut assignment, &plan, &catalog, &detail, conflicting).is_err());
        assert_eq!(assignment.representations.len(), 1);

        let saved = serde_json::to_value(&assignment).unwrap();
        let restored: OccurrenceAssignment = serde_json::from_value(saved).unwrap();
        assert_eq!(restored.representations, assignment.representations);
        assert!(!restored.selected_in_session);
    }

    #[test]
    fn llt_aut_007_01_detail_preserves_the_primary_occurrence_and_refuses_a_missing_support() {
        use crate::dper_inspection::{resolve_occurrence, DrawingRegistrationScope, InspectionWorkspace};
        let (mut plan, catalog, mut assignment, detail, representation) = occurrence_detail_fixture();
        let doc = "document-a";
        plan.source_fingerprint = Some(doc.into());
        let general = define_view(&plan, doc, &catalog, "Vue générale",
            std::slice::from_ref(&assignment.feature_id),
            &[catalog.primitives[0].id.clone(), catalog.primitives[1].id.clone()]).unwrap();
        assert_eq!(general.id, assignment.view_id);
        add_occurrence_representation(&mut assignment, &plan, &catalog, &detail, representation).unwrap();
        let first = assignment.members[0].clone();
        let second = assignment.members[1].clone();
        let feature = assignment.feature_id.clone();
        let mut workspace = InspectionWorkspace {
            plan: Some(plan), views: vec![general.clone(), detail.clone()],
            occurrence_assignments: vec![assignment], ..Default::default()
        };
        let catalogs = [crate::dper_pdf_geometry::DrawingGeometryCatalog {
            document_fingerprint: doc.into(), geometry: catalog,
        }];
        let scope = |view: &DrawingView| DrawingRegistrationScope {
            document_fingerprint: doc.into(), page: view.page, view_id: view.id.clone(),
        };
        workspace.registration_scope = Some(scope(&general));
        let (overview, _) = resolve_occurrence(&workspace, &feature, &first.id, &catalogs).unwrap();
        workspace.registration_scope = Some(scope(&detail));
        let (closeup, geometries) = resolve_occurrence(&workspace, &feature, &first.id, &catalogs).unwrap();
        assert_eq!(overview.member, closeup.member);
        assert_ne!(overview.view_id, closeup.view_id);
        assert!(matches!(geometries.as_slice(), [(1, InspectionGeometry::Circle { radius, .. })] if (*radius - 12.0).abs() < 0.05));
        assert!(resolve_occurrence(&workspace, &feature, &second.id, &catalogs).unwrap_err().contains("Aucun support"));
        workspace.registration_scope = None;
        assert!(resolve_occurrence(&workspace, &feature, &first.id, &catalogs).is_err());
    }

    fn occurrence_detail_fixture() -> (
        DrawingControlPlan, SvgGeometryDocument, OccurrenceAssignment, DrawingView, OccurrenceRepresentationInput,
    ) {
        let (mut plan, catalog) = fixture();
        let mut detail_feature = plan.characteristics[0].clone();
        detail_feature.id = "detail-cote".into();
        detail_feature.source.word_ids = vec!["detail-word".into()];
        plan.characteristics[0].repetition = Some(2);
        plan.characteristics.push(detail_feature);
        let doc = "document-a";
        let feature = &plan.characteristics[0].id;
        let detail_feature = &plan.characteristics[1].id;
        let general = define_view(&plan, doc, &catalog, "Vue générale",
            std::slice::from_ref(feature), &[catalog.primitives[0].id.clone(), catalog.primitives[1].id.clone()]).unwrap();
        let detail = define_view(&plan, doc, &catalog, "Détail B",
            std::slice::from_ref(detail_feature), std::slice::from_ref(&catalog.primitives[2].id)).unwrap();
        let candidates = binding_candidates(&plan, doc, &catalog, &general, feature).unwrap();
        let ids = candidates.into_iter().map(|candidate| candidate.id).collect::<Vec<_>>();
        let assignment = assign_occurrences(&plan, doc, &catalog, &general, feature, &ids, "Deux emplacements repérés.").unwrap();
        let candidates = binding_candidates(&plan, doc, &catalog, &detail, detail_feature).unwrap();
        let source = OccurrenceRepresentationInput {
            occurrence_number: 1, view_id: detail.id.clone(), feature_id: detail_feature.clone(),
            candidate_id: candidates[0].id.clone(), justification: "Renvoi B attaché au premier emplacement.".into(),
        };
        (plan, catalog, assignment, detail, source)
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
        assert!(binding_candidates(&plan, "document-a", &bad, &view, &plan.characteristics[0].id).is_err());
        let refreshed = define_view(&plan, "document-a", &bad, &view.label,
            &view.feature_ids, &view.geometry_ids).unwrap();
        assert!(binding_candidates(
            &plan,
            "document-a",
            &bad,
            &refreshed,
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
    fn scoped_source_changes_invalidate_the_view_even_without_attachment_traces() {
        let (mut plan, catalog) = fixture();
        let mut secondary = plan.characteristics[0].clone();
        secondary.id = "secondary-text".into();
        plan.characteristics.push(secondary);
        let scope = define_view(&plan, "document-a", &catalog, "Complete scope",
            &plan.characteristics.iter().map(|feature| feature.id.clone()).collect::<Vec<_>>(),
            &catalog.primitives.iter().map(|primitive| primitive.id.clone()).collect::<Vec<_>>()).unwrap();
        let proposals = binding_candidates(&plan, "document-a", &catalog, &scope, &plan.characteristics[0].id).unwrap();
        let selected = select_binding(&proposals, &proposals[0].id, &catalog, "Source explicite.").unwrap();
        for mutation in 0..7 {
            let mut changed_plan = plan.clone();
            let mut changed = catalog.clone();
            match mutation {
                0 => changed_plan.characteristics[1].source.exact_text.push_str(" modified"),
                1 => changed.primitives[1].source.path_data.push(' '),
                2 => changed.primitives[1].style.stroke = Some("red".into()),
                3 => changed.view_box.x_min += 1.0,
                4 => changed.flattening_tolerance_pt *= 2.0,
                5 => changed.warnings.push("Extraction partielle".into()),
                _ => changed.unsupported_path_count += 1,
            }
            assert!(validate_view(&changed_plan, "document-a", &changed, &scope).is_err(), "mutation {mutation}");
            assert!(resolve_binding(&changed_plan, "document-a", &changed, &scope, &selected).is_err());
        }
    }

    #[test]
    fn scope_proof_is_canonical_and_ignores_review_and_unselected_sources() {
        let (mut plan, mut catalog) = fixture();
        let scope = define_view(&plan, "document-a", &catalog, "Selected sources",
            &[plan.characteristics[0].id.clone()], &[catalog.primitives[0].id.clone()]).unwrap();
        plan.characteristics[0].confirmed = !plan.characteristics[0].confirmed;
        plan.characteristics[0].measurement_id = Some(999);
        catalog.primitives[0].confirmed = !catalog.primitives[0].confirmed;
        catalog.primitives[1].source.path_data.push(' ');
        let mut unselected = plan.characteristics[0].clone();
        unselected.id = "unselected-text".into();
        plan.characteristics.push(unselected);
        validate_view(&plan, "document-a", &catalog, &scope).unwrap();
        let whole = view(&plan, &catalog);
        let mut reversed = whole.geometry_ids.clone();
        reversed.reverse();
        let reordered = define_view(&plan, "document-a", &catalog, &whole.label, &whole.feature_ids, &reversed).unwrap();
        assert_eq!(whole.id, reordered.id);
        assert_eq!(whole.source_proof, reordered.source_proof);
    }

    #[test]
    fn historical_and_contradictory_views_require_explicit_redefinition() {
        let (plan, catalog) = fixture();
        let scope = view(&plan, &catalog);
        let mut json = serde_json::to_value(&scope).unwrap();
        json.as_object_mut().unwrap().remove("source_proof");
        let historical: DrawingView = serde_json::from_value(json).unwrap();
        assert!(validate_view(&plan, "document-a", &catalog, &historical).unwrap_err().contains("historique"));
        assert!(unique_view(&[scope.clone(), scope.clone()], &scope.id).is_err());
        let alternative = define_view(&plan, "document-a", &catalog, "Conflicting alternative",
            &scope.feature_ids, &scope.geometry_ids).unwrap();
        assert!(unique_view(&[scope.clone(), alternative], &scope.id).is_err());
        let mut changed_members = scope.geometry_ids.clone();
        changed_members.pop();
        let replacement = define_view(&plan, "document-a", &catalog, &scope.label,
            &scope.feature_ids, &changed_members).unwrap();
        assert_ne!(scope.id, replacement.id);
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
