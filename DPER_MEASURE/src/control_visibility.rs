use crate::{dper_inspection::{self, InspectionWorkspace}, dper_vision::InspectionGeometry, DperMeasureApp};
use std::collections::BTreeMap;

impl DperMeasureApp {
    pub(super) fn control_row_visibility(&self, state: &InspectionWorkspace) -> BTreeMap<String, String> {
        let mut restrictions = BTreeMap::new();
        let Some(scope) = state.registration_scope.as_ref() else { return restrictions };
        if state.plan.is_none() || self.current_drawing_source().is_err() {
            return restrictions;
        }
        let Ok(image) = self.inspection_image() else { return restrictions };
        let catalogs = &self.drawing_geometry_cache;
        let Ok(registration) = dper_inspection::registration_with_fingerprint(
            state, &self.inspection_frame(), dper_inspection::fingerprint(&image), catalogs,
        ) else { return restrictions };
        let Some(catalog) = catalogs.iter().find(|catalog| {
            catalog.document_fingerprint == scope.document_fingerprint && catalog.geometry.page == scope.page
        }) else { return restrictions };

        let mut classify = |row_id: &str, view_id: &str, primitive_id: &str| {
            if view_id != scope.view_id {
                restrictions.insert(row_id.to_owned(), "Cote ou occurrence rattachée à une autre vue.".into());
                return;
            }
            let Some(primitive) = catalog.geometry.primitives.iter().find(|item| item.id == primitive_id) else {
                return;
            };
            let Ok(geometry) = crate::dper_binding::circle_geometry(primitive) else { return };
            let InspectionGeometry::Circle { center, radius } = dper_inspection::project(registration, &geometry) else { return };
            if let Some(reason) = circle_visibility(center, radius, image.width(), image.height()) {
                restrictions.insert(row_id.to_owned(), reason.into());
            }
        };
        for binding in state.bindings.iter().filter(|binding| {
            binding.reviewed && self.validate_drawing_binding(&binding.candidate.id).is_ok()
        }) {
            let binding = &binding.candidate;
            classify(&binding.feature_id, &binding.view_id, &binding.geometry_id);
        }
        for assignment in state.occurrence_assignments.iter().filter(|assignment| assignment.selected_in_session) {
            for member in &assignment.members {
                if assignment.view_id == scope.view_id {
                    classify(&member.id, &assignment.view_id, &member.geometry_id);
                    continue;
                }
                let mut details = assignment.representations.iter().filter(|representation|
                    representation.source.occurrence_number == member.number
                        && representation.source.view_id == scope.view_id);
                if let Some(detail) = details.next().filter(|_| details.next().is_none()) {
                    classify(&member.id, &scope.view_id, &detail.geometry_id);
                } else {
                    classify(&member.id, &assignment.view_id, &member.geometry_id);
                }
            }
        }
        for (feature, occurrence, error) in self.control_plan_preview_errors() {
            if error.contains("Contour") || error.contains("contour") || error.contains("profils") {
                restrictions.entry(occurrence.unwrap_or(feature).into())
                    .or_insert_with(|| format!("Contour inexploitable · {error}"));
            }
        }
        restrictions
    }
}

fn circle_visibility(center: crate::geometry::Point, radius: f64, width: u32, height: u32) -> Option<&'static str> {
    let (width, height) = (f64::from(width), f64::from(height));
    if center.x + radius < 0.0 || center.y + radius < 0.0
        || center.x - radius >= width || center.y - radius >= height
    {
        Some("Support projeté hors champ.")
    } else if center.x - radius < 0.0 || center.y - radius < 0.0
        || center.x + radius >= width || center.y + radius >= height
    {
        Some("Support projeté coupé par le cadrage.")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point;

    #[test]
    fn llt_aut_004_01_classifies_projected_supports_at_the_image_bounds() {
        assert_eq!(circle_visibility(Point { x: 35., y: 40. }, 12., 100, 100), None);
        assert_eq!(circle_visibility(Point { x: 75., y: 40. }, 12., 100, 100), None);
        assert_eq!(circle_visibility(Point { x: 75., y: 40. }, 12., 60, 100),
            Some("Support projeté hors champ."));
        assert_eq!(circle_visibility(Point { x: 35., y: 40. }, 12., 45, 100),
            Some("Support projeté coupé par le cadrage."));
        assert_eq!(circle_visibility(Point { x: 35., y: 40. }, 12., 60, 100), None);
    }
}
