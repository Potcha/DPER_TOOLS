//! Operator-supplied scalar correlation budget; no matrix repair or conformity decision.
use crate::uncertainty::{IndependentUncertaintyBudget, SensitivityComponent, SensitivityContribution, UncertaintyUnit};
use rmcp::schemars;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CorrelatedUncertaintyBudget {
    pub output_unit: UncertaintyUnit,
    pub coverage_factor: f64,
    pub correlation_justification: String,
    pub components: Vec<SensitivityComponent>,
    /// Complete matrix in component order, including explicit zero entries.
    pub correlation_matrix: Vec<Vec<f64>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CorrelatedUncertaintyResult {
    pub output_unit: UncertaintyUnit,
    pub coverage_factor: f64,
    pub correlation_justification: String,
    pub components: Vec<SensitivityContribution>,
    /// Signed c_i u_i, in component order and output units. Components retain
    /// their absolute contribution as in the independent-budget result.
    pub signed_contributions: Vec<f64>,
    pub correlation_matrix: Vec<Vec<f64>>,
    pub combination_assumption: String,
    pub combined_standard_uncertainty: f64,
    pub expanded_uncertainty: f64,
    pub numerical_policy: String,
    /// Strict lower bound for accepted dimensionless Cholesky pivots.
    pub minimum_positive_pivot: f64,
    /// Results at or below this normalized standard uncertainty are refused.
    pub cancellation_threshold: f64,
    pub limitation: String,
}

fn compensated_sum(values: impl Iterator<Item = f64>) -> f64 {
    let (mut sum, mut correction) = (0.0_f64, 0.0_f64);
    for value in values {
        let next = sum + value;
        correction += if sum.abs() >= value.abs() {
            (sum - next) + value
        } else {
            (value - next) + sum
        };
        sum = next;
    }
    sum + correction
}

impl CorrelatedUncertaintyBudget {
    pub fn calculate(&self) -> Result<CorrelatedUncertaintyResult, String> {
        if self.correlation_justification.trim().is_empty()
            || self.correlation_justification.chars().count() > 4096
        {
            return Err("La justification des corrélations doit contenir de 1 à 4096 caractères, hors texte vide.".into());
        }
        // Shared component validation only: this internal text is never returned
        // as an independence assumption for the correlated budget.
        let base = IndependentUncertaintyBudget {
            output_unit: self.output_unit,
            coverage_factor: self.coverage_factor,
            independence_justification: "Validation interne des composantes".into(),
            components: self.components.clone(),
        }.calculate()?;
        let n = self.components.len();
        let matrix = &self.correlation_matrix;
        if matrix.len() != n || matrix.iter().any(|row| row.len() != n) {
            return Err("La matrice complète doit être carrée et suivre exactement l'ordre des composantes.".into());
        }
        for i in 0..n {
            for j in 0..n {
                let rho = matrix[i][j];
                if !rho.is_finite() || !(-1.0..=1.0).contains(&rho)
                    || (i == j && rho != 1.0) || rho != matrix[j][i]
                {
                    return Err("Corrélations finies dans [-1,1], diagonale exactement 1 et symétrie exacte requises ; aucune réparation automatique.".into());
                }
            }
        }
        let signed: Vec<f64> = self.components.iter()
            .map(|c| c.sensitivity_coefficient * c.standard_uncertainty).collect();
        let scale = base.components.iter().map(|c| c.contribution).fold(0.0_f64, f64::max);
        let normalized: Vec<f64> = signed.iter().map(|v| v / scale).collect();
        if signed.iter().zip(&normalized).any(|(v, ratio)| *v != 0.0 && *ratio == 0.0) {
            return Err("La dynamique des contributions dépasse la résolution numérique de normalisation.".into());
        }

        // Remove only algebraically exact duplicate/opposite FULL original rows.
        // Their Gram vectors are necessarily identical/opposite. This supports
        // exact rho=+/-1 singularities without accepting an uncertain zero pivot
        // or changing any operator-supplied matrix entry.
        let mut representatives: Vec<usize> = Vec::new();
        let mut groups: Vec<Vec<f64>> = Vec::new();
        for i in 0..n {
            let matched = representatives.iter().enumerate().find_map(|(g, &r)| {
                if (0..n).all(|j| matrix[i][j] == matrix[r][j]) { Some((g, 1.0)) }
                else if (0..n).all(|j| matrix[i][j] == -matrix[r][j]) { Some((g, -1.0)) }
                else { None }
            });
            if let Some((group, sign)) = matched {
                groups[group].push(sign * normalized[i]);
            } else {
                representatives.push(i);
                groups.push(vec![normalized[i]]);
            }
        }
        let m = representatives.len();
        let mut weights: Vec<f64> = groups.iter()
            .map(|g| compensated_sum(g.iter().copied())).collect();
        let mut residual: Vec<Vec<f64>> = representatives.iter().map(|&i| {
            representatives.iter().map(|&j| matrix[i][j]).collect()
        }).collect();
        let mut factor = vec![vec![0.0; m]; m];
        let pivot_threshold = 64.0 * f64::EPSILON * n as f64;
        // Complete diagonal pivoting; positive pivots have a margin above the
        // floating-point error scale. No negative/near-zero pivot is clamped.
        for k in 0..m {
            let pivot = (k..m).max_by(|&i, &j| residual[i][i].total_cmp(&residual[j][j])).unwrap();
            if residual[pivot][pivot] <= pivot_threshold || !residual[pivot][pivot].is_finite() {
                return Err("Matrice non positive semi-définie ou numériquement indécidable : pivot trop petit/non positif. Seules les dépendances de lignes exactement identiques/opposées sont prises en charge.".into());
            }
            residual.swap(k, pivot);
            for row in &mut residual { row.swap(k, pivot); }
            factor.swap(k, pivot);
            weights.swap(k, pivot);
            factor[k][k] = residual[k][k].sqrt();
            for i in k + 1..m { factor[i][k] = residual[i][k] / factor[k][k]; }
            for i in k + 1..m {
                for j in i..m {
                    let next = residual[j][i] - factor[i][k] * factor[j][k];
                    residual[j][i] = next;
                    residual[i][j] = next;
                }
            }
        }
        // ||L^T w|| avoids subtracting large covariance terms. In particular,
        // coefficient signs are retained, including after row grouping/pivoting.
        let projected: Vec<f64> = (0..m).map(|k| {
            compensated_sum((k..m).map(|i| factor[i][k] * weights[i]))
        }).collect();
        let norm = projected.iter().fold(0.0_f64, |acc, v| acc.hypot(*v));
        let cancellation_threshold = pivot_threshold
            * compensated_sum(normalized.iter().map(|v| v.abs()));
        if !norm.is_finite() || norm <= cancellation_threshold {
            return Err("Incertitude corrélée nulle ou annulation trop proche de la résolution numérique ; aucune décision de conformité possible.".into());
        }
        let combined = scale * norm;
        let expanded = combined * self.coverage_factor;
        if !combined.is_finite() || combined == 0.0 || !expanded.is_finite() || expanded == 0.0 {
            return Err("L'incertitude corrélée ou élargie dépasse les limites numériques.".into());
        }
        Ok(CorrelatedUncertaintyResult {
            output_unit: self.output_unit,
            coverage_factor: self.coverage_factor,
            correlation_justification: self.correlation_justification.clone(),
            components: base.components,
            signed_contributions: signed,
            correlation_matrix: matrix.clone(),
            combination_assumption: "Propagation scalaire linéaire : u_c² = somme_i,j (c_i u_i) rho_ij (c_j u_j), corrélations fournies par l'opérateur.".into(),
            combined_standard_uncertainty: combined,
            expanded_uncertainty: expanded,
            numerical_policy: "Symétrie/diagonale exactes ; aucune correction de matrice. Réduction des lignes exactement identiques/opposées puis Cholesky à pivot diagonal maximal ; pivots strictement supérieurs à 64 epsilon n. Contributions normalisées par leur maximum, sommes compensées, norme hypot ; annulation <= 64 epsilon n somme|c_i u_i/max| refusée. Ces seuils numériques ne sont pas des incertitudes métrologiques.".into(),
            minimum_positive_pivot: pivot_threshold,
            cancellation_threshold,
            limitation: "Calcul scalaire linéaire fourni par l'opérateur, sans conversion implicite d'unités ni estimation des corrélations. Certaines matrices PSD singulières ou mal conditionnées sont refusées par prudence. Les limites du validateur indépendant s'appliquent aussi (dont dépassement de sa RSS et de son incertitude élargie). Arrondi IEEE-754, notamment sous-normal ; aucune garantie d'exactitude relative à cette échelle. Ce calcul seul ne valide ni une mesure ni sa conformité.".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn budget(values: &[f64], rho: f64) -> CorrelatedUncertaintyBudget {
        let n = values.len();
        CorrelatedUncertaintyBudget {
            output_unit: UncertaintyUnit::Millimeter, coverage_factor: 2.0,
            correlation_justification: "Modèle fourni et documenté".into(),
            components: values.iter().enumerate().map(|(i, &c)| SensitivityComponent {
                name: format!("c{i}"), input_unit: UncertaintyUnit::Pixel,
                standard_uncertainty: c.abs(), sensitivity_coefficient: c.signum(),
                source: "Certificat fourni".into(),
            }).collect(),
            correlation_matrix: (0..n).map(|i| (0..n).map(|j| if i == j { 1.0 } else { rho }).collect()).collect(),
        }
    }

    #[test]
    fn analytic_zero_and_half_correlations_keep_signed_coefficients() {
        for (rho, expected) in [(0.0, 5.0), (0.5, 13.0_f64.sqrt())] {
            let input = budget(&[3.0, -4.0], rho);
            let result = input.calculate().unwrap();
            assert!((result.combined_standard_uncertainty - expected).abs() < 1e-14);
            assert_eq!(result.signed_contributions, vec![3.0, -4.0]);
            assert_eq!(result.components[1].sensitivity_coefficient, -1.0);
            assert_eq!(result.components[1].source, input.components[1].source);
            assert_eq!(result.components[1].sensitivity_input_unit, UncertaintyUnit::Pixel);
            assert_eq!(result.correlation_matrix, input.correlation_matrix);
            assert!(!result.combination_assumption.contains("indépendantes"));
        }
    }

    #[test]
    fn exact_perfect_correlations_and_opposite_signs() {
        for (rho, expected) in [(1.0, 1.0), (-1.0, 7.0)] {
            let result = budget(&[3.0, -4.0], rho).calculate().unwrap();
            assert!((result.combined_standard_uncertainty - expected).abs() < 1e-14);
        }
        assert!(budget(&[3.0, -3.0], 1.0).calculate().unwrap_err().contains("annulation"));
        assert!(budget(&[3.0, 3.0], -1.0).calculate().is_err());
    }

    #[test]
    fn rejects_indefinite_matrix_even_when_requested_variance_is_positive() {
        let mut input = budget(&[10.0, 1.0, 1.0], 0.0);
        input.correlation_matrix = vec![vec![1.0, 0.9, 0.9], vec![0.9, 1.0, -0.9], vec![0.9, -0.9, 1.0]];
        assert!(input.calculate().unwrap_err().contains("pivot"));
    }

    #[test]
    fn rank_deficient_repeated_rows_and_pivot_permutation() {
        let mut input = budget(&[2.0, -1.0, 3.0], 0.0);
        input.correlation_matrix = vec![vec![1.0, 0.5, 1.0], vec![0.5, 1.0, 0.5], vec![1.0, 0.5, 1.0]];
        let result = input.calculate().unwrap();
        assert!((result.combined_standard_uncertainty - 21.0_f64.sqrt()).abs() < 1e-14);
        // A nontrivial reduced positive-definite matrix exercises row pivoting.
        input.correlation_matrix = vec![vec![1.0, 0.9, 0.1], vec![0.9, 1.0, 0.1], vec![0.1, 0.1, 1.0]];
        assert!((input.calculate().unwrap().combined_standard_uncertainty - 11.0_f64.sqrt()).abs() < 1e-14);
    }

    #[test]
    fn rejects_inexact_structure_nonfinite_and_out_of_bounds() {
        for matrix in [vec![], vec![vec![1.0]], vec![vec![1.0, 0.0], vec![0.0]],
            vec![vec![1.0, 0.1], vec![0.1 + f64::EPSILON, 1.0]],
            vec![vec![0.999, 0.0], vec![0.0, 1.0]],
            vec![vec![1.0, 1.01], vec![1.01, 1.0]],
            vec![vec![1.0, f64::NAN], vec![f64::NAN, 1.0]],
            vec![vec![1.0, f64::INFINITY], vec![f64::INFINITY, 1.0]]] {
            let mut input = budget(&[1.0, 2.0], 0.0);
            input.correlation_matrix = matrix;
            assert!(input.calculate().is_err());
        }
    }

    #[test]
    fn shares_input_source_unit_and_numeric_validation() {
        let valid = || budget(&[1.0, 2.0], 0.0);
        let mut input = valid(); input.components[1].name = " c0 ".into();
        assert!(input.calculate().is_err());
        input = valid(); input.components[0].source = " ".into();
        assert!(input.calculate().is_err());
        input = valid(); input.output_unit = UncertaintyUnit::Dimensionless;
        assert!(input.calculate().is_err());
        input = valid(); input.components[0].standard_uncertainty = f64::NAN;
        assert!(input.calculate().is_err());
        input = valid(); input.components[0].sensitivity_coefficient = f64::INFINITY;
        assert!(input.calculate().is_err());
        input = valid(); input.coverage_factor = -1.0;
        assert!(input.calculate().is_err());
        input = valid(); input.correlation_justification = " ".into();
        assert!(input.calculate().is_err());
        for unit in [UncertaintyUnit::Degree, UncertaintyUnit::Pixel] {
            input = valid(); input.output_unit = unit;
            assert_eq!(input.calculate().unwrap().output_unit, unit);
        }
    }

    #[test]
    fn normalized_calculation_handles_large_small_and_subnormal_scales() {
        for scale in [1e200, 1e-200] {
            let result = budget(&[3.0 * scale, -4.0 * scale], 0.5).calculate().unwrap();
            assert!((result.combined_standard_uncertainty / scale - 13.0_f64.sqrt()).abs() < 1e-14);
        }
        let tiny = f64::from_bits(1);
        let result = budget(&vec![tiny; 64], 0.0).calculate().unwrap();
        assert_eq!(result.combined_standard_uncertainty.to_bits(), 8);
        assert_eq!(result.expanded_uncertainty.to_bits(), 16);
        let mut input = budget(&[tiny, tiny], 1.0);
        input.coverage_factor = 1.0;
        assert_eq!(input.calculate().unwrap().combined_standard_uncertainty.to_bits(), 2);
        assert!(budget(&[1e200, tiny], 0.0).calculate().is_err());
    }

    #[test]
    fn refuses_uncertain_near_singularity_and_near_cancellation() {
        assert!(budget(&[1.0, 1.0], 1.0 - f64::EPSILON).calculate().is_err());
        assert!(budget(&[1.0, -1.0 + f64::EPSILON], 1.0).calculate().is_err());
        // Valid singular Gram matrix without exact duplicate/opposite rows is
        // deliberately refused rather than repairing a roundoff-sized pivot.
        let mut input = budget(&[1.0, 1.0, 1.0], 0.0);
        input.correlation_matrix = vec![vec![1.0, 0.0, 0.6], vec![0.0, 1.0, 0.8], vec![0.6, 0.8, 1.0]];
        assert!(input.calculate().is_err());
    }

    #[test]
    fn explicit_serialization_and_schema_reject_extras() {
        let input = budget(&[1.0, 2.0], 0.0);
        let mut json = serde_json::to_value(&input).unwrap();
        assert_eq!(serde_json::from_value::<CorrelatedUncertaintyBudget>(json.clone()).unwrap(), input);
        json["extra"] = true.into();
        assert!(serde_json::from_value::<CorrelatedUncertaintyBudget>(json).is_err());
        let mut json = serde_json::to_value(&input).unwrap();
        json["components"][0]["extra"] = true.into();
        assert!(serde_json::from_value::<CorrelatedUncertaintyBudget>(json).is_err());
        let json = serde_json::to_value(input.calculate().unwrap()).unwrap();
        assert!(json.get("independence_justification").is_none());
        assert_eq!(json["correlation_matrix"][0][1], 0.0);
        let schema = serde_json::to_value(schemars::schema_for!(CorrelatedUncertaintyBudget)).unwrap();
        assert_eq!(schema["additionalProperties"], false);
    }
}
