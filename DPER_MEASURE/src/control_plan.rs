use crate::{DperMeasureApp, mcp_server::{DrawingRequest, InspectBoundFeatureArgs, PreviewControlPlanArgs}};
use serde_json::{json, Value};

impl DperMeasureApp {
    pub(crate) fn preview_control_plan(&mut self, args: PreviewControlPlanArgs) -> Result<Value, String> {
        self.inspection_guard(&args.capture_id, args.expected_revision)?;
        if self.inspection_observation_ui.has_draft() {
            return Err("Un brouillon d'observation est ouvert ; terminez-le avant l'aperçu groupé.".into());
        }
        if !(1..=8).contains(&args.limit) { return Err("La limite doit être comprise entre 1 et 8.".into()); }
        let state = self.current_inspection();
        let plan = state.plan.as_ref().ok_or("Aucun plan de contrôle actif.")?;
        let progress = crate::inspection_progress::summarize(&state);
        let occurrences = crate::drawing_structure::occurrence_observations(&state);
        let mut jobs: Vec<(String, Option<String>, String)> = Vec::new();
        let mut blocked = Vec::new();
        for item in &progress {
            let feature = plan.characteristics.iter().find(|f| f.id == item.feature_id).unwrap();
            if item.next_action.code != "observe_contours" {
                blocked.push(json!({"feature_id":item.feature_id,"status":"blocked_or_complete","reason":item.next_action.instruction}));
                continue;
            }
            if !matches!(feature.kind, crate::dper_drawing::DrawingDimensionKind::Diameter | crate::dper_drawing::DrawingDimensionKind::Radius) {
                blocked.push(json!({"feature_id":item.feature_id,"status":"unsupported","reason":"Recherche groupée disponible pour les diamètres et rayons uniquement."}));
                continue;
            }
            let group = state.records.iter().rev().find(|r| r.feature_id == item.feature_id && r.capture_id == args.capture_id && !r.imported && !r.group.trim().is_empty())
                .map(|r| r.group.clone()).unwrap_or_else(|| format!("Contrôle · {}", item.feature_id));
            if let Some(repeated) = occurrences.iter().find(|r| r.feature_id == item.feature_id) {
                if let Some(assignment) = state.occurrence_assignments.iter().find(|a| a.feature_id == item.feature_id && a.selected_in_session) {
                    for member in &assignment.members {
                        if !repeated.observed_occurrence_ids.contains(&member.id) {
                            jobs.push((item.feature_id.clone(), Some(member.id.clone()), group.clone()));
                        }
                    }
                } else {
                    blocked.push(json!({"feature_id":item.feature_id,"status":"blocked","reason":"Affectation courante indisponible."}));
                }
            } else {
                jobs.push((item.feature_id.clone(), None, group));
            }
        }
        let mut outcomes = Vec::new();
        for (index, (feature_id, occurrence_id, group)) in jobs.into_iter().enumerate() {
            if index >= args.limit {
                outcomes.push(json!({"feature_id":feature_id,"occurrence_id":occurrence_id,"status":"pending_limit"}));
                continue;
            }
            let result = self.handle_drawing_request(DrawingRequest::InspectBound(InspectBoundFeatureArgs {
                capture_id: args.capture_id.clone(), expected_revision: self.document_revision,
                feature_id: feature_id.clone(), occurrence_id: occurrence_id.clone(), group,
                source_measurement_ids: Vec::new(), options: None, commit: false,
            }));
            outcomes.push(match result {
                Ok(value) => json!({"feature_id":feature_id,"occurrence_id":occurrence_id,"status":"previewed","result":value}),
                Err(error) => json!({"feature_id":feature_id,"occurrence_id":occurrence_id,"status":"error","error":error}),
            });
        }
        let attempted = outcomes.iter().filter(|v| v["status"] != "pending_limit").count();
        Ok(json!({"capture_id":self.capture_id,"document_revision":self.document_revision,"limit":args.limit,
            "attempted":attempted,"jobs":outcomes,"blocked":blocked,
            "characteristic_progress":crate::inspection_progress::summarize(&self.current_inspection()),
            "limitations":["Les aperçus enregistrent des preuves dans la session et ne créent aucune mesure.", "Aucune validation humaine ni décision de conformité n'est effectuée."]}))
    }
}
