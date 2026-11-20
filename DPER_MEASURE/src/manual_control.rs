use super::*;
use crate::dper_drawing::{declared_repetition, DrawingCharacteristic, DrawingDimensionKind, DrawingMeasurability};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct ManualControlTarget {
    pub feature_id: String,
    pub occurrence: Option<u32>,
    signature: String,
}

impl ManualControlTarget {
    pub(crate) fn row_id(&self) -> String {
        self.occurrence.map_or_else(|| self.feature_id.clone(), |number| {
            format!("repetition:{}:{number}", self.feature_id)
        })
    }

    pub(crate) fn identify(
        plan: &crate::dper_drawing::DrawingControlPlan, feature: &DrawingCharacteristic,
        occurrence: Option<u32>,
    ) -> Option<Self> {
        let count = declared_repetition(feature).unwrap_or(1);
        let valid_occurrence = match occurrence {
            Some(number) => (2..=crate::dper_binding::MAX_OCCURRENCES).contains(&count)
                && (1..=count).contains(&number),
            None => count == 1,
        };
        if !valid_occurrence || !feature.confirmed || feature.has_unresolved_unit_context()
            || feature.measurability != DrawingMeasurability::PlanarCandidate
            || !matches!(feature.kind, DrawingDimensionKind::Linear | DrawingDimensionKind::Diameter
                | DrawingDimensionKind::Radius | DrawingDimensionKind::Angle)
        {
            return None;
        }
        let source = serde_json::to_vec(&(&plan.source_name, &plan.source_fingerprint, feature)).ok()?;
        Some(Self { feature_id: feature.id.clone(), occurrence, signature: crate::dper_source::fingerprint_bytes(&source) })
    }

    fn resolve<'a>(&self, plan: &'a crate::dper_drawing::DrawingControlPlan) -> Option<&'a DrawingCharacteristic> {
        let mut matching = plan.characteristics.iter().filter(|feature| feature.id == self.feature_id);
        let feature = matching.next()?;
        if matching.next().is_some() || Self::identify(plan, feature, self.occurrence).as_ref() != Some(self) {
            return None;
        }
        Some(feature)
    }
}

#[derive(Clone, Debug)]
pub(super) struct ActiveManualControl {
    target: ManualControlTarget,
    capture_id: String,
    revision: u64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct ManualControlLink {
    pub target: ManualControlTarget,
    pub capture_id: String,
    pub measurement_id: u64,
}

fn compatible(kind: DrawingDimensionKind, tool: Tool) -> bool {
    match kind {
        DrawingDimensionKind::Linear => matches!(tool, Tool::Distance | Tool::PointLineDistance | Tool::ParallelDistance),
        DrawingDimensionKind::Diameter => matches!(tool, Tool::Circle | Tool::CircleFit),
        DrawingDimensionKind::Radius => matches!(tool, Tool::Circle | Tool::CircleFit | Tool::Arc),
        DrawingDimensionKind::Angle => tool == Tool::Angle,
        _ => false,
    }
}

impl DperMeasureApp {
    pub(super) fn activate_manual_control(&mut self, target: ManualControlTarget) {
        let Some(feature) = self.inspection.plan.as_ref().and_then(|plan| target.resolve(plan)) else {
            self.active_manual_control = None;
            self.message = "La cote ou l’occurrence a changé. Sélectionnez-la à nouveau.".into();
            return;
        };
        if self.image.is_none() || self.calibration_mode || self.registration_photo_pick_owns_frame()
            || self.point_edit.is_some() || self.numeric_resize_dialog.is_some()
        {
            self.message = "Terminez l’action en cours et ouvrez une image avant de sélectionner la cote.".into();
            return;
        }
        let label = feature.label.clone();
        self.clear_pending();
        self.construction_mode = false;
        self.selection_mode = false;
        self.pan_mode = false;
        self.active_manual_control = Some(ActiveManualControl {
            target, capture_id: self.capture_id.clone(), revision: self.document_revision,
        });
        self.message = format!("Cote active : {label}. Tracez la mesure sur l’image.");
    }

    pub(super) fn active_manual_control_row(&self) -> Option<String> {
        let active = self.active_manual_control.as_ref()?;
        (active.capture_id == self.capture_id && active.revision == self.document_revision)
            .then(|| active.target.row_id())
    }

    pub(super) fn manual_target_for_measurement(&mut self, kind: Tool) -> Result<Option<ManualControlTarget>, String> {
        let Some(active) = self.active_manual_control.as_ref() else { return Ok(None) };
        let resolved = self.inspection.plan.as_ref()
            .filter(|plan| plan.source_file.is_none() || self.current_drawing_source().is_ok())
            .and_then(|plan| active.target.resolve(plan));
        let current = active.capture_id == self.capture_id && active.revision == self.document_revision;
        let Some(feature) = resolved.filter(|_| current) else {
            self.active_manual_control = None;
            return Err("La capture ou le plan a changé. Sélectionnez à nouveau la cote.".into());
        };
        if self.construction_mode || !compatible(feature.kind, kind) {
            return Err(format!("Le tracé « {} » est incompatible avec la cote active « {} ».", kind.label(), feature.label));
        }
        Ok(self.active_manual_control.take().map(|active| active.target))
    }

    pub(super) fn retain_current_manual_links(&self, state: &mut crate::dper_inspection::InspectionWorkspace) {
        if state.manual_control_links.is_empty() {
            return;
        }
        let plan = state.plan.as_ref();
        let source_current = plan.is_some_and(|plan| plan.source_file.is_none() || self.current_drawing_source().is_ok());
        state.manual_control_links.retain(|link| source_current && link.capture_id == self.capture_id
            && plan.and_then(|plan| link.target.resolve(plan)).is_some_and(|feature| {
                self.measurements.iter().any(|measurement| measurement.id == link.measurement_id
                    && measurement.capture_id == link.capture_id && !measurement.is_construction
                    && !measurement.dependency_stale && compatible(feature.kind, measurement.kind))
            }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llt_man_001_01_only_records_the_current_compatible_occurrence() {
        let ctx = egui::Context::default();
        let mut app = DperMeasureApp::new_with_services(ctx.clone(), false);
        let image = image::RgbImage::from_pixel(100, 100, image::Rgb([120, 120, 120]));
        let mut quality = crate::quality::assess_image_quality(
            image.as_raw(), 100, 100, &crate::quality::ImageQualityThresholds::default(),
        ).unwrap();
        quality.is_exploitable = true;
        app.quality_assessment = Some(quality);
        app.image = Some(ImageData {
            width: 100, height: 100, rgb: image.into_raw(), source: "reference-test.png".into(),
        });
        let mut plan = crate::dper_drawing::parse_text_plan("2xØ4 ±0,1 mm", "fiche.pdf");
        plan.characteristics[0].confirmed = true;
        let first = ManualControlTarget::identify(&plan, &plan.characteristics[0], Some(1)).unwrap();
        let second = ManualControlTarget::identify(&plan, &plan.characteristics[0], Some(2)).unwrap();
        app.inspection.plan = Some(plan);

        app.activate_manual_control(first.clone());
        app.activate_manual_control(second.clone());
        assert_eq!(app.active_manual_control_row(), Some(second.row_id()));
        assert!(app.manual_target_for_measurement(Tool::Distance).is_err());
        assert!(app.inspection.manual_control_links.is_empty());
        app.choose_measurement_tool(Tool::Circle);
        app.pending = vec![Point { x: 30.0, y: 30.0 }, Point { x: 44.0, y: 30.0 }];
        app.finish_measurement();
        assert_eq!(app.measurements.len(), 1, "{}", app.message);
        assert_eq!(app.inspection.manual_control_links.len(), 1);
        assert_eq!(app.inspection.manual_control_links[0].target, second);
        assert_eq!(app.inspection.manual_control_links[0].measurement_id, app.measurements[0].id);
        assert!(app.active_manual_control_row().is_none());
        let state = app.current_inspection();
        let rows = crate::drawing_control_table::rows(&state);
        assert_ne!(rows[0].status, "Mesurée");
        assert_eq!(rows[1].status, "Mesurée");

        app.activate_manual_control(first.clone());
        app.escape_active_mode();
        assert!(app.active_manual_control_row().is_none());
        app.activate_manual_control(first);
        app.capture_id.push_str("-autre");
        assert!(app.manual_target_for_measurement(Tool::Circle).is_err());
        assert_eq!(app.measurements.len(), 1);
        assert!(app.current_inspection().manual_control_links.is_empty());
    }
}
