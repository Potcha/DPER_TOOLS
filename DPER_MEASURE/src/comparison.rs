//! Comparaison A/B de deux sessions sans masquer les différences de protocole.
use crate::{geometry::{Measurement, Tool}, reference, storage::Session};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub struct AbComparison {
    pub a_name: String,
    pub b_name: String,
    pub comparable: bool,
    pub reasons: Vec<String>,
    pub rows: Vec<AbRow>,
}

#[derive(Clone, Debug)]
pub struct AbRow {
    pub label: String,
    pub kind: String,
    pub measurement_a: Option<u64>,
    pub measurement_b: Option<u64>,
    pub value_a_mm: Option<f64>,
    pub value_b_mm: Option<f64>,
    pub uncertainty_a_mm: Option<f64>,
    pub uncertainty_b_mm: Option<f64>,
    pub delta_b_minus_a_mm: Option<f64>,
    pub count_a: Option<usize>,
    pub count_b: Option<usize>,
    pub delta_count_b_minus_a: Option<i64>,
    pub status: String,
}

pub fn compare_sessions(
    a: &Session,
    b: &Session,
    a_name: String,
    b_name: String,
) -> AbComparison {
    let reasons = comparability_reasons(a, b);
    let comparable = reasons.is_empty();
    let a_measurements = grouped_measurements(&a.measurements);
    let b_measurements = grouped_measurements(&b.measurements);
    let keys = a_measurements.keys().chain(b_measurements.keys()).cloned()
        .collect::<BTreeSet<_>>();
    let mut rows = Vec::new();
    for (kind, label) in keys {
        let left = a_measurements.get(&(kind.clone(), label.clone())).cloned().unwrap_or_default();
        let right = b_measurements.get(&(kind.clone(), label.clone())).cloned().unwrap_or_default();
        for index in 0..left.len().max(right.len()) {
            let measurement_a = left.get(index).copied();
            let measurement_b = right.get(index).copied();
            let value_a_mm = measurement_a.and_then(reference::measurement_mm);
            let value_b_mm = measurement_b.and_then(reference::measurement_mm);
            let uncertainty_a_mm = measurement_a.filter(|measurement| measurement.kind != Tool::ObjectCount)
                .and_then(|measurement| measurement.expanded_uncertainty_mm)
                .filter(|value| value.is_finite() && *value > 0.0);
            let uncertainty_b_mm = measurement_b.filter(|measurement| measurement.kind != Tool::ObjectCount)
                .and_then(|measurement| measurement.expanded_uncertainty_mm)
                .filter(|value| value.is_finite() && *value > 0.0);
            let delta_b_minus_a_mm = value_a_mm.zip(value_b_mm).map(|(a, b)| b - a);
            let count_a = measurement_a.filter(|measurement| measurement.kind == Tool::ObjectCount)
                .map(|measurement| measurement.points.len());
            let count_b = measurement_b.filter(|measurement| measurement.kind == Tool::ObjectCount)
                .map(|measurement| measurement.points.len());
            let delta_count_b_minus_a = count_a.zip(count_b)
                .map(|(a, b)| b as i64 - a as i64);
            let status = if !comparable {
                "Écart indicatif · conditions incompatibles".to_owned()
            } else if measurement_a.is_none() {
                "Mesure présente uniquement dans B".to_owned()
            } else if measurement_b.is_none() {
                "Mesure présente uniquement dans A".to_owned()
            } else if measurement_a.is_some_and(|measurement| measurement.kind == Tool::ObjectCount)
                && measurement_b.is_some_and(|measurement| measurement.kind == Tool::ObjectCount)
            {
                if delta_count_b_minus_a == Some(0) {
                    "Comptage identique · à confirmer visuellement".to_owned()
                } else {
                    "Écart de comptage indicatif · à confirmer visuellement".to_owned()
                }
            } else if delta_b_minus_a_mm.is_none() {
                "Valeur en mm indisponible".to_owned()
            } else if uncertainty_a_mm.is_none() || uncertainty_b_mm.is_none() {
                "Écart indicatif · U incomplète".to_owned()
            } else {
                "Écart calculé · décision qualité à établir".to_owned()
            };
            rows.push(AbRow {
                label: if index == 0 { label.clone() } else { format!("{label} · occurrence {}", index + 1) },
                kind: kind.clone(),
                measurement_a: measurement_a.map(|measurement| measurement.id),
                measurement_b: measurement_b.map(|measurement| measurement.id),
                value_a_mm, value_b_mm, uncertainty_a_mm, uncertainty_b_mm,
                delta_b_minus_a_mm, count_a, count_b, delta_count_b_minus_a, status,
            });
        }
    }
    AbComparison { a_name, b_name, comparable, reasons, rows }
}

fn grouped_measurements(measurements: &[Measurement]) -> BTreeMap<(String, String), Vec<&Measurement>> {
    let mut groups = BTreeMap::<(String, String), Vec<&Measurement>>::new();
    for measurement in measurements {
        groups.entry(measurement_comparison_key(measurement)).or_default().push(measurement);
    }
    for group in groups.values_mut() { group.sort_by_key(|measurement| measurement.id); }
    groups
}

fn measurement_comparison_key(measurement: &Measurement) -> (String, String) {
    let kind = measurement.kind.label().to_owned();
    let value = measurement.label.trim();
    let label = if value.is_empty() || value.eq_ignore_ascii_case(&format!("M{}", measurement.id)) {
        kind.clone()
    } else {
        value.to_lowercase()
    };
    (kind, label)
}

fn comparability_reasons(a: &Session, b: &Session) -> Vec<String> {
    let mut reasons = Vec::new();
    let identity_a = if a.device_id.trim().is_empty() { &a.device } else { &a.device_id };
    let identity_b = if b.device_id.trim().is_empty() { &b.device } else { &b.device_id };
    if identity_a.trim().is_empty() || identity_b.trim().is_empty() || identity_a != identity_b {
        reasons.push("Appareil ou identifiant stable différent".to_owned());
    }
    if a.optical_setup.trim().is_empty() || a.optical_setup != b.optical_setup {
        reasons.push("Configuration optique différente ou absente".to_owned());
    }
    match (&a.protocol, &b.protocol) {
        (Some(protocol_a), Some(protocol_b)) => {
            let a_json = serde_json::to_value(protocol_a).ok();
            let b_json = serde_json::to_value(protocol_b).ok();
            if protocol_a.id != protocol_b.id || protocol_a.version != protocol_b.version || a_json != b_json {
                reasons.push("Protocole de mesure ou version différent".to_owned());
            }
            let protocol_measurements_match = |session: &Session, protocol: &crate::storage::MeasurementProtocol| {
                session.measurements.iter().all(|measurement|
                    measurement.protocol_id.as_deref() == Some(protocol.id.as_str())
                        && measurement.protocol_version == Some(protocol.version))
            };
            if !protocol_measurements_match(a, protocol_a) || !protocol_measurements_match(b, protocol_b) {
                reasons.push("Au moins une mesure ne suit pas la version de protocole enregistrée".to_owned());
            }
        }
        _ => reasons.push("Protocole versionné absent d'une session".to_owned()),
    }
    if crate::folder_export::session_protocol_notice(a).is_some()
        || crate::folder_export::session_protocol_notice(b).is_some()
    {
        reasons.push("Au moins une session ne respecte pas les caractéristiques de son protocole versionné".to_owned());
    }
    let requires_dimensional_calibration = a.measurements.iter().chain(&b.measurements)
        .any(|measurement| measurement.kind != Tool::ObjectCount);
    if requires_dimensional_calibration {
        match (&a.calibration, &b.calibration) {
            (Some(calibration_a), Some(calibration_b)) => {
                if calibration_a.validation_status == "failed" || calibration_b.validation_status == "failed"
                    || calibration_a.id != calibration_b.id
                    || calibration_a.device_id != calibration_b.device_id
                    || calibration_a.resolution != calibration_b.resolution
                    || calibration_a.optical_setup != calibration_b.optical_setup
                    || calibration_a.scale_x_mm_per_pixel.map(f64::to_bits)
                        != calibration_b.scale_x_mm_per_pixel.map(f64::to_bits)
                    || calibration_a.scale_y_mm_per_pixel.map(f64::to_bits)
                        != calibration_b.scale_y_mm_per_pixel.map(f64::to_bits)
                    || calibration_a.mm_per_pixel.to_bits() != calibration_b.mm_per_pixel.to_bits()
                    || !same_serialized(calibration_a, calibration_b)
                {
                    reasons.push("Profil de calibration ou échelles différentes".to_owned());
                }
            }
            _ => reasons.push("Profil de calibration absent d'une session".to_owned()),
        }
    }
    match (&a.reference_document, &b.reference_document) {
        (Some(_), Some(_)) if reference_signature(a) != reference_signature(b) => {
            reasons.push("Cotes du plan de référence ou associations différentes".to_owned());
        }
        (Some(_), None) | (None, Some(_)) => {
            reasons.push("Plan de référence absent d'une session".to_owned());
        }
        _ => {}
    }
    if !same_serialized(&a.quality_thresholds, &b.quality_thresholds) {
        reasons.push("Seuils d'exploitabilité différents".to_owned());
    }
    if !same_serialized(&a.display_correction, &b.display_correction) {
        reasons.push("Correction d'affichage différente".to_owned());
    }
    if !quality_evaluation_current(a) || !quality_evaluation_current(b) {
        reasons.push("Qualité d'image non exploitable, périmée ou incompatible avec la résolution enregistrée".to_owned());
    }
    if a.measurements.iter().any(|measurement| measurement.kind != Tool::ObjectCount
            && measurement.expanded_uncertainty_mm
            .is_none_or(|uncertainty| !uncertainty.is_finite() || uncertainty <= 0.0))
        || b.measurements.iter().any(|measurement| measurement.kind != Tool::ObjectCount
            && measurement.expanded_uncertainty_mm
            .is_none_or(|uncertainty| !uncertainty.is_finite() || uncertainty <= 0.0))
    {
        reasons.push("Incertitude élargie U manquante pour au moins une mesure".to_owned());
    }
    reasons
}

fn same_serialized<T: serde::Serialize>(a: &T, b: &T) -> bool {
    serde_json::to_value(a).ok() == serde_json::to_value(b).ok()
}

fn quality_evaluation_current(session: &Session) -> bool {
    let expected_resolution = session.calibration.as_ref().map(|calibration| calibration.resolution)
        .or_else(|| session.protocol.as_ref().map(|protocol| protocol.resolution));
    session.quality_assessment.as_ref().is_some_and(|assessment| {
        assessment.is_exploitable
            && assessment.matches_context(&session.quality_thresholds, session.display_correction)
            && expected_resolution.is_some_and(|resolution| assessment.image_dimensions == resolution)
    })
}

fn reference_signature(
    session: &Session,
) -> Option<Vec<(String, u64, u64, u64, bool, Option<(String, String)>)>> {
    let document = session.reference_document.as_ref()?;
    let mut signature = document.dimensions.iter().map(|dimension| {
        let associated_measurement = dimension.measurement_id
            .and_then(|id| session.measurements.iter().find(|measurement| measurement.id == id))
            .map(measurement_comparison_key);
        (
            dimension.label.trim().to_lowercase(),
            dimension.nominal_mm.to_bits(),
            dimension.tolerance_minus_mm.to_bits(),
            dimension.tolerance_plus_mm.to_bits(),
            dimension.confirmed,
            associated_measurement,
        )
    }).collect::<Vec<_>>();
    signature.sort();
    Some(signature)
}

pub fn to_csv(comparison: &AbComparison) -> Result<Vec<u8>, String> {
    let mut output = String::from(
        "source_a;source_b;comparabilite;reserves;mesure;type;id_a;valeur_a_mm;u_a_mm;id_b;valeur_b_mm;u_b_mm;delta_b_moins_a_mm;comptage_a_objets;comptage_b_objets;delta_comptage_b_moins_a;statut\r\n",
    );
    let reasons = comparison.reasons.join(" | ");
    for row in &comparison.rows {
        let fields = [
            comparison.a_name.clone(), comparison.b_name.clone(),
            if comparison.comparable { "oui" } else { "non" }.to_owned(),
            reasons.clone(), row.label.clone(), row.kind.clone(),
            row.measurement_a.map(|value| value.to_string()).unwrap_or_default(),
            decimal(row.value_a_mm), decimal(row.uncertainty_a_mm),
            row.measurement_b.map(|value| value.to_string()).unwrap_or_default(),
            decimal(row.value_b_mm), decimal(row.uncertainty_b_mm),
            decimal(row.delta_b_minus_a_mm),
            row.count_a.map(|value| value.to_string()).unwrap_or_default(),
            row.count_b.map(|value| value.to_string()).unwrap_or_default(),
            row.delta_count_b_minus_a.map(|value| value.to_string()).unwrap_or_default(),
            row.status.clone(),
        ];
        for (index, field) in fields.iter().enumerate() {
            if index > 0 { output.push(';'); }
            output.push_str(&csv_escape(field));
        }
        output.push_str("\r\n");
    }
    if comparison.rows.is_empty() {
        let fields = [comparison.a_name.clone(), comparison.b_name.clone(),
            if comparison.comparable { "oui" } else { "non" }.to_owned(),
            reasons, "Aucune mesure commune ou à rapprocher".to_owned()];
        for index in 0..17 {
            if index > 0 { output.push(';'); }
            if let Some(field) = fields.get(index) { output.push_str(&csv_escape(field)); }
        }
        output.push_str("\r\n");
    }
    Ok(output.into_bytes())
}

fn decimal(value: Option<f64>) -> String {
    value.filter(|value| value.is_finite()).map(|value| format!("{value:.8}")).unwrap_or_default()
}

fn csv_escape(value: &str) -> String {
    if value.chars().any(|character| matches!(character, ';' | '"' | '\r' | '\n')) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}
