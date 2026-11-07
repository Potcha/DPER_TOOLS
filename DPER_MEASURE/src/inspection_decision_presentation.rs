//! Shared, read-only decision trace. Formatting never grants qualification.
use crate::{dper_drawing::DrawingControlPlan, dper_inspection::InspectionRecord};

pub(crate) fn details(record: &InspectionRecord, plan: Option<&DrawingControlPlan>) -> Vec<String> {
    let mut lines = vec!["Décision distincte de la validation du contour : intervalle valeur ± U entièrement inclus, entièrement disjoint, ou indéterminé.".into()];
    let features = plan.into_iter().flat_map(|plan| &plan.characteristics)
        .filter(|feature| feature.id == record.feature_id).collect::<Vec<_>>();
    if let [feature] = features.as_slice() {
        if let Some(reason) = crate::drawing_structure::suspension_reason(feature) {
            lines.push(format!("Comparaison suspendue. {}.", crate::dper_metrology::reason_label(reason)));
        } else if let Some(tolerance) = crate::dper_inspection::tolerance(feature) {
            let nominal = tolerance.nominal.map(|value| format!("{value:.6} mm")).unwrap_or_else(|| "non indiqué".into());
            lines.push(format!("Exigence confirmée : nominal {nominal} ; limites [{:.6}, {:.6}] mm.",
                tolerance.lower, tolerance.upper));
        } else { lines.push("Tolérance absente, non confirmée ou non résolue : aucun verdict de conformité.".into()); }
    } else { lines.push("Exigence absente ou ambiguë : aucun verdict de conformité.".into()); }
    match &record.decision_evidence {
        Some(evidence) => {
            let basis = &evidence.basis;
            lines.push(format!("Qualification indépendante déclarée par {} : {} ; domaine : {}.",
                basis.reviewer, basis.qualification_reference, basis.qualification_scope));
            lines.push(format!("U totale élargie fournie : {:.6} mm ; source : {}. Aucun calcul automatique de U.",
                basis.expanded_uncertainty_mm, basis.uncertainty_source));
            if let Some(value) = record.value_mm.filter(|value| value.is_finite()) {
                let lower = value - basis.expanded_uncertainty_mm;
                let upper = value + basis.expanded_uncertainty_mm;
                if basis.expanded_uncertainty_mm.is_finite() && basis.expanded_uncertainty_mm > 0.0
                    && lower.is_finite() && upper.is_finite() {
                    lines.push(format!("Intervalle déclaré à réévaluer : [{lower:.6}, {upper:.6}] mm ; valeur observée {value:.6} mm."));
                }
            }
            lines.push("La référence documentaire et son domaine sont attestés par l'opérateur ; le logiciel ne certifie pas ce document. Seul l'état courant réévalué autorise une décision.".into());
        }
        None => lines.push("Qualification indépendante et U totale de cette mesure non revues : conformité physique non établie. L'incertitude du profil d'étalonnage ne les remplace pas.".into()),
    }
    lines
}
