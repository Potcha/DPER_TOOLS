//! Explicit documentary unit declarations, without inferred page-local scope.
use super::*;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DrawingUnitStatus { Absent, Unique, Conflict }

impl DrawingUnitStatus {
    pub fn label(self) -> &'static str {
        match self { Self::Absent => "Déclaration absente", Self::Unique => "Unité déclarée unique", Self::Conflict => "Déclarations contradictoires" }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DrawingUnitDeclaration {
    pub exact_text: String,
    pub page: Option<u32>,
    pub pdf_bbox: Option<PdfBox>,
    pub word_ids: Vec<String>,
    pub units: Vec<DrawingUnit>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DrawingUnitContext {
    pub status: DrawingUnitStatus,
    pub declarations: Vec<DrawingUnitDeclaration>,
}

impl DrawingUnitContext {
    pub fn unique_unit(&self) -> Option<DrawingUnit> {
        let mut units = self.declarations.iter().flat_map(|declaration| declaration.units.iter()).copied();
        let first = units.next()?;
        (self.status == DrawingUnitStatus::Unique && units.all(|unit| unit == first)).then_some(first)
    }

    fn from_declarations(declarations: Vec<DrawingUnitDeclaration>) -> Self {
        let mut units = declarations.iter().flat_map(|declaration| declaration.units.iter());
        let status = match units.next() {
            None => DrawingUnitStatus::Absent,
            Some(first) if units.all(|unit| unit == first) => DrawingUnitStatus::Unique,
            Some(_) => DrawingUnitStatus::Conflict,
        };
        Self { status, declarations }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DrawingUnitOrigin { Unspecified, Inline, DocumentDeclaration, InterpretationProposal }

impl DrawingUnitOrigin {
    pub fn label(self) -> &'static str {
        match self {
            Self::Unspecified => "Unité non spécifiée",
            Self::Inline => "Unité explicite de la cote",
            Self::DocumentDeclaration => "Unité issue du document",
            Self::InterpretationProposal => "Unité proposée avec justification",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DrawingUnitEvidence {
    pub document: DrawingUnitContext,
    pub origin: DrawingUnitOrigin,
}

impl DrawingUnitEvidence {
    pub(super) fn permits_unit(&self, unit: Option<DrawingUnit>, reviewed: bool) -> bool {
        let Some(unit) = unit else { return false; };
        match self.origin {
            DrawingUnitOrigin::Inline => true,
            DrawingUnitOrigin::DocumentDeclaration => self.document.unique_unit() == Some(unit),
            DrawingUnitOrigin::Unspecified => false,
            DrawingUnitOrigin::InterpretationProposal => self.document.status != DrawingUnitStatus::Conflict || reviewed,
        }
    }
}

fn declaration(line: &str, page: Option<u32>, pdf_bbox: Option<PdfBox>, word_ids: Vec<String>) -> Option<DrawingUnitDeclaration> {
    let lower = line.trim_start().to_lowercase();
    let prefix = ["unités", "unites", "units", "toutes les cotes en", "all dimensions in"]
        .into_iter().find(|prefix| lower.starts_with(prefix)
            && lower[prefix.len()..].chars().next().is_none_or(|c| !c.is_alphabetic()))?;
    // Scan the declaration's contents only: the preposition in "dimensions in"
    // is not an inch declaration. Every distinct unit in the suffix participates.
    let mut units = Vec::new();
    for unit in lower[prefix.len()..].split(|c: char| !c.is_alphabetic()).filter_map(unit_word) {
        if !units.contains(&unit) { units.push(unit); }
    }
    Some(DrawingUnitDeclaration { exact_text: line.into(), page, pdf_bbox, word_ids, units })
}

pub(super) fn text_context(text: &str) -> DrawingUnitContext {
    let mut declarations = Vec::new();
    let mut page = 1u32;
    for page_text in text.split('\u{000c}') {
        for line in page_text.lines() {
            if let Some(marker) = ocr_page_marker(line) { page = marker; continue; }
            if let Some(source) = declaration(line, Some(page), None, Vec::new()) { declarations.push(source); }
        }
        page = page.saturating_add(1);
    }
    DrawingUnitContext::from_declarations(declarations)
}

pub(super) fn layout_context(groups: &[PdfTextGroup]) -> DrawingUnitContext {
    DrawingUnitContext::from_declarations(groups.iter().filter_map(|group|
        declaration(&group.source_text, Some(group.page), Some(group.bbox), group.word_ids.clone())).collect())
}

pub(super) fn apply_context(feature: &mut DrawingCharacteristic, context: &DrawingUnitContext) {
    let origin = if feature.unit.is_some() {
        DrawingUnitOrigin::Inline
    } else if feature.kind != DrawingDimensionKind::Angle && feature.kind != DrawingDimensionKind::Thread {
        if let Some(unit) = context.unique_unit().filter(|unit| unit.millimeters_per_unit().is_some()) {
            let prior = interpretation_assessment(feature.kind, None, feature.tolerance.is_some());
            let extra_reasons: Vec<_> = feature.reasons.iter().filter(|reason| !prior.1.contains(reason)).cloned().collect();
            let keep_suspended = feature.measurability == DrawingMeasurability::NeedsConfirmation && !extra_reasons.is_empty();
            feature.unit = Some(unit);
            let assessment = interpretation_assessment(feature.kind, feature.unit, feature.tolerance.is_some());
            feature.measurability = if keep_suspended { DrawingMeasurability::NeedsConfirmation } else { assessment.0 };
            feature.reasons = assessment.1;
            feature.reasons.extend(extra_reasons);
            DrawingUnitOrigin::DocumentDeclaration
        } else { DrawingUnitOrigin::Unspecified }
    } else { DrawingUnitOrigin::Unspecified };
    feature.unit_context = Some(DrawingUnitEvidence { document: context.clone(), origin });
    append_context_reason(feature);
}

pub(super) fn append_context_reason(feature: &mut DrawingCharacteristic) {
    let Some(evidence) = &feature.unit_context else { return; };
    match evidence.document.status {
        DrawingUnitStatus::Conflict => {
            feature.reasons.push("unit_declarations_conflict: Déclarations d'unités contradictoires conservées ; aucune portée locale ni unité héritée n'est choisie. Une unité inline explicite reste documentée séparément.".into());
            if evidence.origin == DrawingUnitOrigin::InterpretationProposal {
                if feature.measurability == DrawingMeasurability::PlanarCandidate {
                    feature.measurability = DrawingMeasurability::NeedsConfirmation;
                }
                feature.reasons.push("unit_conflict_interpretation_requires_review: La proposition justifiée ne résout pas automatiquement le conflit documentaire.".into());
            }
        },
        DrawingUnitStatus::Absent if feature.unit.is_none() => feature.reasons.push("unit_declaration_absent: Aucune déclaration d'unité reconnue ; aucun héritage implicite.".into()),
        _ => {},
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_declared_units_participate_independent_of_order_and_english_preposition() {
        for text in ["Units: mm / inches\nØ4 ±0.1", "Units: inches / mm\nØ4 ±0.1",
            "Units: mm\nUnits: inches\nØ4 ±0.1", "Units: inches\nUnits: mm\nØ4 ±0.1"] {
            let plan = parse_text_plan(text, "conflict");
            let context = plan.unit_context.as_ref().unwrap();
            assert_eq!(context.status, DrawingUnitStatus::Conflict);
            assert_eq!(context.unique_unit(), None);
            assert!(context.declarations.iter().all(|source| source.page == Some(1)));
            let feature = &plan.characteristics[0];
            assert_eq!(feature.unit, None);
            assert_eq!(feature.nominal_mm(), None);
            assert_eq!(feature.limits_mm(), None);
            assert!(feature.has_unresolved_unit_context());
            assert!(feature.reasons.iter().any(|reason| reason.starts_with("unit_declarations_conflict:")));
        }
        for text in ["All dimensions in mm\nØ4 ±0.1", "Toutes les cotes en mm\nØ4 ±0.1",
            "Units: mm / millimeters\nØ4 ±0.1"] {
            let plan = parse_text_plan(text, "unique");
            assert_eq!(plan.unit_context.as_ref().unwrap().status, DrawingUnitStatus::Unique);
            assert_eq!(plan.unit_context.as_ref().unwrap().unique_unit(), Some(DrawingUnit::Millimeter));
            assert_eq!(plan.characteristics[0].nominal_mm(), Some(4.0));
            assert_eq!(plan.characteristics[0].limits_mm(), Some((3.9, 4.1)));
            assert_eq!(plan.characteristics[0].unit_context.as_ref().unwrap().origin, DrawingUnitOrigin::DocumentDeclaration);
        }
    }

    #[test]
    fn absence_is_explicit_and_inline_units_survive_document_conflicts() {
        let absent = parse_text_plan("Ø4 ±0.1", "absent");
        assert_eq!(absent.unit_context.as_ref().unwrap().status, DrawingUnitStatus::Absent);
        assert!(absent.unit_context.as_ref().unwrap().declarations.is_empty());
        assert_eq!(absent.characteristics[0].nominal_mm(), None);
        let plan = parse_text_plan("Units: mm/inches\nØ4 mm ±0.1 mm\nR2 in\n45°", "inline");
        assert_eq!(plan.characteristics.len(), 3);
        assert_eq!(plan.characteristics[0].nominal_mm(), Some(4.0));
        assert_eq!(plan.characteristics[0].limits_mm(), Some((3.9, 4.1)));
        assert_eq!(plan.characteristics[1].nominal_mm(), Some(50.8));
        assert_eq!(plan.characteristics[2].unit, Some(DrawingUnit::Degree));
        for feature in &plan.characteristics {
            assert_eq!(feature.unit_context.as_ref().unwrap().origin, DrawingUnitOrigin::Inline);
            assert_eq!(feature.unit_context.as_ref().unwrap().document.status, DrawingUnitStatus::Conflict);
            assert!(!feature.has_unresolved_unit_context());
            assert!(!feature.confirmed);
        }
        let mixed_inline = parse_text_plan("Units: mm\nR2 in", "inline override");
        assert_eq!(mixed_inline.characteristics[0].nominal_mm(), Some(50.8));
        assert_eq!(mixed_inline.characteristics[0].unit_context.as_ref().unwrap().origin, DrawingUnitOrigin::Inline);
    }

    #[test]
    fn declarations_across_pages_never_invent_a_page_local_scope() {
        for text in ["Units: mm\nØ4 ±0.1\u{000c}Units: inches\nR2 ±0.1",
            "[Page 1]\nUnits: mm\nØ4 ±0.1\n[Page 2]\nUnits: inches\nR2 ±0.1",
            "[Page 1 OCR]\nUnits: mm\nØ4 ±0.1\n[Page 2 OCR]\nUnits: inches\nR2 ±0.1"] {
            let plan = parse_text_plan(text, "pages");
            let context = plan.unit_context.as_ref().unwrap();
            assert_eq!(context.status, DrawingUnitStatus::Conflict);
            assert_eq!(context.declarations.len(), 2);
            assert_eq!(context.declarations[0].page, Some(1));
            assert_eq!(context.declarations[1].page, Some(2));
            assert_eq!(context.declarations[0].exact_text, "Units: mm");
            assert_eq!(context.declarations[1].exact_text, "Units: inches");
            assert!(plan.characteristics.iter().all(|feature| feature.unit.is_none()
                && feature.nominal_mm().is_none() && feature.limits_mm().is_none()));
        }
    }

    fn group(page: u32, text: &str, id: &str) -> PdfTextGroup {
        PdfTextGroup {
            source_line_id: id.into(), page,
            bbox: PdfBox { x_min: 10.0, y_min: 10.0, x_max: 150.0, y_max: 20.0 },
            orientation: dper_pdf_layout::TextOrientation::Horizontal,
            source_text: text.into(), proposed_text: Some(text.into()),
            word_ids: vec![format!("{id}-word")], words: Vec::new(), suspension_reasons: Vec::new(),
        }
    }

    #[test]
    fn layout_conflict_preserves_declaration_pages_boxes_and_word_ids() {
        let groups = vec![group(1, "Units: mm", "declare-1"), group(1, "Ø4 ±0.1", "feature-1"),
            group(2, "Units: inches", "declare-2"), group(2, "R2 mm ±0.1 mm", "feature-2")];
        let plan = from_pdf_layout(PdfLayout { pages: vec![
            PdfPageGeometry { page: 1, width_pt: 200.0, height_pt: 200.0 },
            PdfPageGeometry { page: 2, width_pt: 200.0, height_pt: 200.0 }],
            lines: Vec::new(), groups: groups.clone(), warnings: Vec::new() }, "layout");
        assert_eq!(plan.characteristics.len(), 2);
        let context = plan.unit_context.as_ref().unwrap();
        assert_eq!(context.status, DrawingUnitStatus::Conflict);
        for (source, group) in context.declarations.iter().zip([&groups[0], &groups[2]]) {
            assert_eq!(source.exact_text, group.source_text);
            assert_eq!(source.page, Some(group.page));
            assert_eq!(source.pdf_bbox, Some(group.bbox));
            assert_eq!(source.word_ids, group.word_ids);
        }
        assert_eq!(plan.characteristics[0].nominal_mm(), None);
        assert_eq!(plan.characteristics[0].source.page, Some(1));
        assert_eq!(plan.characteristics[1].nominal_mm(), Some(2.0));
        assert_eq!(plan.characteristics[1].source.page, Some(2));
        let feature_context = &plan.characteristics[0].unit_context.as_ref().unwrap().document;
        assert_eq!(serde_json::to_value(feature_context).unwrap(), serde_json::to_value(context).unwrap());
    }

    #[test]
    fn manual_proposal_keeps_conflict_provenance_and_requires_review_before_conversion() {
        let mut plan = parse_text_plan("Units: mm/inches\nØ4 ±0.1", "review");
        let original = serde_json::to_value(&plan.unit_context).unwrap();
        let id = plan.characteristics[0].id.clone();
        propose_interpretation(&mut plan, &id, DrawingDimensionKind::Diameter,
            Some(DrawingUnit::Millimeter), "Cartouche contradictoire, unité mm proposée selon la cote source à revoir.").unwrap();
        let feature = &mut plan.characteristics[0];
        assert_eq!(feature.unit, Some(DrawingUnit::Millimeter));
        assert_eq!(feature.unit_context.as_ref().unwrap().origin, DrawingUnitOrigin::InterpretationProposal);
        assert_eq!(serde_json::to_value(&feature.unit_context.as_ref().unwrap().document).unwrap(), original);
        assert_eq!(feature.measurability, DrawingMeasurability::NeedsConfirmation);
        assert!(!feature.confirmed);
        assert_eq!(feature.nominal_mm(), None);
        assert_eq!(feature.limits_mm(), None);
        assert!(feature.reasons.iter().any(|reason| reason.starts_with("unit_declarations_conflict:")));
        assert!(feature.interpretation_notes[0].contains("Cartouche contradictoire"));
        feature.confirmed = true; // A separate, explicit human review remains necessary.
        assert_eq!(feature.nominal_mm(), Some(4.0));
        assert_eq!(feature.unit_context.as_ref().unwrap().document.status, DrawingUnitStatus::Conflict);
        propose_interpretation(&mut plan, &id, DrawingDimensionKind::Depth,
            Some(DrawingUnit::Millimeter), "Profondeur hors du plan, unité proposée à revoir.").unwrap();
        assert_eq!(plan.characteristics[0].measurability, DrawingMeasurability::NotMeasurable2d);
        assert!(!plan.characteristics[0].confirmed);
    }

    #[test]
    fn old_serialized_projects_remain_readable_without_new_context_fields() {
        let plan = parse_text_plan("Ø4 mm ±0.1 mm", "old");
        let mut value = serde_json::to_value(plan).unwrap();
        value.as_object_mut().unwrap().remove("unit_context");
        value["characteristics"][0].as_object_mut().unwrap().remove("unit_context");
        let old: DrawingControlPlan = serde_json::from_value(value).unwrap();
        assert!(old.unit_context.is_none());
        assert!(old.characteristics[0].unit_context.is_none());
        assert_eq!(old.characteristics[0].nominal_mm(), Some(4.0));
        assert_eq!(old.characteristics[0].limits_mm(), Some((3.9, 4.1)));
    }

    #[test]
    fn unmatched_legacy_dimensions_retain_global_conflict_even_if_legacy_confirmed() {
        let document = ReferenceDocument {
            source_file: "legacy.pdf".into(), source_name: "legacy".into(),
            extracted_text: "Units: mm\u{000c}Units: inches".into(),
            dimensions: vec![reference::ReferenceDimension {
                id: "unmatched-legacy".into(), label: "Ø4".into(), nominal_mm: 4.0,
                tolerance_minus_mm: 0.1, tolerance_plus_mm: 0.1,
                source_text: "Ø4 ±0.1".into(), confirmed: true, measurement_id: Some(7),
                ocr_page: Some(2), ocr_bbox: None, ocr_confidence: Some(90.0),
            }],
        };
        let plan = from_reference_document(&document);
        assert_eq!(plan.characteristics.len(), 1);
        let feature = &plan.characteristics[0];
        assert!(feature.confirmed);
        assert_eq!(feature.unit_context.as_ref().unwrap().document.status, DrawingUnitStatus::Conflict);
        assert_eq!(feature.unit_context.as_ref().unwrap().origin, DrawingUnitOrigin::Unspecified);
        assert_eq!(feature.source.page, Some(2));
        assert_eq!(feature.nominal_mm(), None);
        assert_eq!(feature.limits_mm(), None);
    }

    #[test]
    fn explicit_mixed_tolerance_units_never_compute_false_limits() {
        for text in ["Ø10 mm ±0.1 in", "Ø10 mm +0.2 / -0.1 in", "45° ±0.5 mm", "MIN 9.9 mm MAX 10.1 in"] {
            let plan = parse_text_plan(text, "mixed");
            let feature = &plan.characteristics[0];
            assert!(feature.tolerance.is_none(), "{text}");
            assert!(feature.limits_mm().is_none(), "{text}");
            assert_eq!(feature.measurability, DrawingMeasurability::NeedsConfirmation);
            assert!(feature.reasons.iter().any(|reason| reason.starts_with("explicit_tolerance_unit_conflict:")));
        }
        for text in ["Ø10 mm ±0.1 mm", "Ø10 ±0.1 mm", "Units: inches\nMIN 9.9 mm MAX 10.1"] {
            let plan = parse_text_plan(text, "compatible");
            assert_eq!(plan.characteristics[0].limits_mm(), Some((9.9, 10.1)));
        }
        let angle = parse_text_plan("45 ±0.5°", "angle");
        assert_eq!(angle.characteristics[0].unit, Some(DrawingUnit::Degree));
        assert!(angle.characteristics[0].tolerance.is_some());
    }
}
