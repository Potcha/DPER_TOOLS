use crate::{DperMeasureApp, mcp_server::{DrawingRequest, InspectBoundFeatureArgs, PreviewControlPlanArgs}};
use serde_json::{json, Value};
use crate::{dper_inspection::InspectionRecord, inspection_observation_ui::ObservationContext};
use std::sync::Arc;

pub(crate) struct BatchPreviews {
    context: Arc<ObservationContext>,
    records: Vec<InspectionRecord>,
    pub last_preview_revision: Option<u64>,
}

fn continuation_token() -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    format!("{}-{}", crate::new_id("preview"), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
}

pub(crate) struct Continuation {
    pub token: String,
    capture_id: String,
    revision: u64,
    context: Arc<ObservationContext>,
    jobs: Vec<(String, Option<String>, String)>,
    blocked: Vec<Value>,
    next: usize,
}

impl DperMeasureApp {
    pub(crate) fn current_control_plan_previews(&self, context: &ObservationContext) -> &[InspectionRecord] {
        self.control_plan_previews.as_ref()
            .filter(|batch| batch.context.as_ref() == context)
            .map_or(&[], |batch| batch.records.as_slice())
    }

    pub(crate) fn control_plan_review_targets(&self) -> Vec<(String, Option<String>, String)> {
        self.control_plan_previews.as_ref()
            .filter(|batch| batch.last_preview_revision == Some(self.document_revision))
            .into_iter()
            .flat_map(|batch| &batch.records)
            .map(|record| (record.feature_id.clone(), record.occurrence.as_ref().map(|proof| proof.member.id.clone()), record.target_label()))
            .collect()
    }

    pub(crate) fn preview_control_plan(&mut self, args: PreviewControlPlanArgs) -> Result<Value, String> {
        self.inspection_guard(&args.capture_id, args.expected_revision)?;
        if self.inspection_observation_ui.has_draft() {
            return Err("Un brouillon d'observation est ouvert ; terminez-le avant l'aperçu groupé.".into());
        }
        if !(1..=8).contains(&args.limit) { return Err("La limite doit être comprise entre 1 et 8.".into()); }
        if let Some(cursor) = &args.cursor {
            let continuation = self.control_plan_continuation.as_ref().ok_or("Reprise expirée ou inconnue ; recommencez.")?;
            if continuation.token != *cursor || continuation.capture_id != args.capture_id || continuation.revision != args.expected_revision {
                return Err("Reprise périmée ou invalide ; recommencez.".into());
            }
            if self.observation_context()? != *continuation.context {
                return Err("Le contexte de reprise a changé ; recommencez.".into());
            }
            return self.run_control_plan_page(args.limit);
        }
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
        let context = Arc::new(self.observation_context()?);
        self.control_plan_continuation = Some(Continuation { token: continuation_token(), capture_id: args.capture_id, revision: self.document_revision, context, jobs, blocked, next: 0 });
        self.run_control_plan_page(args.limit)
    }

    fn run_control_plan_page(&mut self, limit: usize) -> Result<Value, String> {
        let mut continuation = self.control_plan_continuation.take().unwrap();
        let last_preview_revision = self.control_plan_previews.take()
            .and_then(|batch| batch.last_preview_revision);
        let start = continuation.next;
        let end = (start + limit).min(continuation.jobs.len());
        let mut outcomes = Vec::new();
        let mut records = Vec::new();
        for (index, (feature_id, occurrence_id, group)) in continuation.jobs.iter().cloned().enumerate().skip(start) {
            if index >= end {
                outcomes.push(json!({"feature_id":feature_id,"occurrence_id":occurrence_id,"status":"pending_limit"}));
                continue;
            }
            let result = self.handle_drawing_request(DrawingRequest::InspectBound(InspectBoundFeatureArgs {
                capture_id: continuation.capture_id.clone(), expected_revision: self.document_revision,
                feature_id: feature_id.clone(), occurrence_id: occurrence_id.clone(), group,
                source_measurement_ids: Vec::new(), options: None, commit: false,
            }));
            outcomes.push(match result {
                Ok(value) => {
                    if let Some(record) = self.inspection.records.last() {
                        records.push(record.clone());
                    }
                    json!({"feature_id":feature_id,"occurrence_id":occurrence_id,"status":"previewed","result":value})
                }
                Err(error) => json!({"feature_id":feature_id,"occurrence_id":occurrence_id,"status":"error","error":error}),
            });
        }
        let attempted = outcomes.iter().filter(|v| v["status"] != "pending_limit").count();
        continuation.next = end;
        continuation.revision = self.document_revision;
        let blocked = continuation.blocked.clone();
        // These searches only append evidence. Retain the original pixels and
        // sources so a concurrent change cannot make old evidence current.
        Arc::make_mut(&mut continuation.context).set_revision(self.document_revision);
        self.control_plan_previews = Some(BatchPreviews {
            last_preview_revision: if records.is_empty() { last_preview_revision } else { Some(self.document_revision) },
            context: Arc::clone(&continuation.context), records,
        });
        let next_cursor = if end < continuation.jobs.len() {
            continuation.token = continuation_token();
            let token = Some(continuation.token.clone());
            self.control_plan_continuation = Some(continuation);
            token
        } else { None };
        Ok(json!({"capture_id":self.capture_id,"document_revision":self.document_revision,"limit":limit,
            "attempted":attempted,"jobs":outcomes,"blocked":blocked,"next_cursor":next_cursor,
            "characteristic_progress":crate::inspection_progress::summarize(&self.current_inspection()),
            "limitations":["Les aperçus enregistrent des preuves dans la session et ne créent aucune mesure.", "Aucune validation humaine ni décision de conformité n'est effectuée."]}))
    }
}
