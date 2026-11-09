//! Combined standard-uncertainty budget supplied by the operator.
use serde::{Deserialize, Serialize};
use rmcp::schemars;

/// Units are declared explicitly: no conversion is inferred from these labels.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UncertaintyUnit {
    Millimeter,
    Degree,
    Pixel,
    /// Permitted for inputs only.
    Dimensionless,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SensitivityComponent {
    pub name: String,
    pub input_unit: UncertaintyUnit,
    pub standard_uncertainty: f64,
    /// Signed coefficient in output_unit / input_unit, supplied by the operator.
    pub sensitivity_coefficient: f64,
    pub source: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IndependentUncertaintyBudget {
    pub output_unit: UncertaintyUnit,
    pub coverage_factor: f64,
    pub independence_justification: String,
    pub components: Vec<SensitivityComponent>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SensitivityContribution {
    pub name: String,
    pub input_unit: UncertaintyUnit,
    pub standard_uncertainty: f64,
    pub sensitivity_coefficient: f64,
    pub sensitivity_output_unit: UncertaintyUnit,
    pub sensitivity_input_unit: UncertaintyUnit,
    pub contribution: f64,
    pub contribution_unit: UncertaintyUnit,
    pub source: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IndependentUncertaintyResult {
    pub output_unit: UncertaintyUnit,
    pub coverage_factor: f64,
    pub independence_justification: String,
    pub combination_assumption: String,
    pub components: Vec<SensitivityContribution>,
    pub combined_standard_uncertainty: f64,
    pub expanded_uncertainty: f64,
    /// The calculation alone does not validate a measurement or conformity.
    pub limitation: String,
}

impl IndependentUncertaintyBudget {
    /// Computes RSS of |c_i| u_i for explicitly assumed independent inputs.
    /// Coefficients express output units per input unit; no correlations or
    /// automatic unit conversions are modelled. This is a calculation only.
    pub fn calculate(&self) -> Result<IndependentUncertaintyResult, String> {
        fn valid_text(value: &str, max: usize) -> bool {
            !value.trim().is_empty() && value.chars().count() <= max
        }
        if self.output_unit == UncertaintyUnit::Dimensionless {
            return Err("L'unité de sortie doit être millimeter, degree ou pixel.".into());
        }
        if !self.coverage_factor.is_finite() || self.coverage_factor <= 0.0 {
            return Err("Le facteur de couverture k doit être fini et strictement positif.".into());
        }
        if !valid_text(&self.independence_justification, 4096) {
            return Err("La justification d'indépendance doit contenir de 1 à 4096 caractères, hors texte vide.".into());
        }
        if self.components.is_empty() || self.components.len() > 64 {
            return Err("Le budget doit contenir de 1 à 64 composantes.".into());
        }
        let mut names = std::collections::HashSet::new();
        let mut components = Vec::with_capacity(self.components.len());
        let mut max_contribution: f64 = 0.0;
        for component in &self.components {
            if !valid_text(&component.name, 256) || !names.insert(component.name.trim()) {
                return Err("Les noms doivent être non vides, uniques après retrait des espaces et limités à 256 caractères.".into());
            }
            if !valid_text(&component.source, 4096) {
                return Err("Chaque source doit être non vide et limitée à 4096 caractères.".into());
            }
            let u = component.standard_uncertainty;
            let c = component.sensitivity_coefficient;
            if !u.is_finite() || u < 0.0 || !c.is_finite() {
                return Err("Chaque incertitude-type doit être finie et positive ou nulle, et chaque coefficient fini.".into());
            }
            let contribution = c.abs() * u;
            if !contribution.is_finite() || (c != 0.0 && u != 0.0 && contribution == 0.0) {
                return Err("Une contribution dépasse les limites numériques.".into());
            }
            max_contribution = max_contribution.max(contribution);
            components.push(SensitivityContribution {
                name: component.name.trim().into(),
                input_unit: component.input_unit,
                standard_uncertainty: u,
                sensitivity_coefficient: c,
                sensitivity_output_unit: self.output_unit,
                sensitivity_input_unit: component.input_unit,
                contribution,
                contribution_unit: self.output_unit,
                source: component.source.clone(),
            });
        }
        if max_contribution == 0.0 {
            return Err("Le budget doit avoir une incertitude combinée finie et non nulle.".into());
        }
        // Accumulate in normalized units to avoid rounding every intermediate
        // norm to a subnormal output. Each square is at most one (64 inputs).
        let normalized_sum = components.iter().map(|component| {
            let ratio = component.contribution / max_contribution;
            ratio * ratio
        }).sum::<f64>();
        let combined = max_contribution * normalized_sum.sqrt();
        if !combined.is_finite() || combined == 0.0 {
            return Err("Le budget doit avoir une incertitude combinée finie et non nulle.".into());
        }
        let expanded = combined * self.coverage_factor;
        if !expanded.is_finite() || expanded == 0.0 {
            return Err("L'incertitude élargie dépasse les limites numériques.".into());
        }
        Ok(IndependentUncertaintyResult {
            output_unit: self.output_unit,
            coverage_factor: self.coverage_factor,
            independence_justification: self.independence_justification.clone(),
            combination_assumption: "Composantes supposées indépendantes ; aucune corrélation modélisée.".into(),
            components,
            combined_standard_uncertainty: combined,
            expanded_uncertainty: expanded,
            limitation: "Ce calcul seul ne valide ni une mesure ni sa conformité.".into(),
        })
    }
}

#[cfg(test)]
mod independent_tests {
    use super::*;

    fn component(name: &str, u: f64, c: f64, unit: UncertaintyUnit) -> SensitivityComponent {
        SensitivityComponent {
            name: name.into(), input_unit: unit, standard_uncertainty: u,
            sensitivity_coefficient: c, source: "Certificat ou modèle fourni".into(),
        }
    }

    fn budget(components: Vec<SensitivityComponent>) -> IndependentUncertaintyBudget {
        IndependentUncertaintyBudget {
            output_unit: UncertaintyUnit::Millimeter, coverage_factor: 2.0,
            independence_justification: "Acquisitions indépendantes justifiées par le protocole".into(),
            components,
        }
    }

    #[test]
    fn explicit_sensitivities_units_and_traceability() {
        let input = budget(vec![
            component(" pixels ", 3.0, -0.01, UncertaintyUnit::Pixel),
            component("angle", 2.0, 0.02, UncertaintyUnit::Degree),
            component("inactive", 10.0, 0.0, UncertaintyUnit::Dimensionless),
        ]);
        let result = input.calculate().unwrap();
        assert!((result.combined_standard_uncertainty - 0.05).abs() < 1e-15);
        assert!((result.expanded_uncertainty - 0.1).abs() < 1e-15);
        let first = &result.components[0];
        assert_eq!(first.name, "pixels");
        assert_eq!(first.sensitivity_coefficient, -0.01);
        assert_eq!(first.input_unit, UncertaintyUnit::Pixel);
        assert_eq!(first.sensitivity_input_unit, UncertaintyUnit::Pixel);
        assert_eq!(first.sensitivity_output_unit, UncertaintyUnit::Millimeter);
        assert_eq!(first.contribution_unit, UncertaintyUnit::Millimeter);
        assert_eq!(first.contribution, 0.03);
        assert_eq!(first.source, input.components[0].source);
        assert_eq!(result.components[2].contribution, 0.0);
        assert_eq!(result.coverage_factor, 2.0);
        assert_eq!(result.independence_justification, input.independence_justification);
        assert!(result.combination_assumption.contains("aucune corrélation"));
        assert!(result.limitation.contains("ne valide ni une mesure"));
    }

    #[test]
    fn degree_and_pixel_outputs_do_not_implicitly_use_millimeters() {
        for unit in [UncertaintyUnit::Degree, UncertaintyUnit::Pixel] {
            let mut input = budget(vec![component("lecture", 0.5, 3.0, UncertaintyUnit::Pixel)]);
            input.output_unit = unit;
            let result = input.calculate().unwrap();
            assert_eq!(result.output_unit, unit);
            assert_eq!(result.components[0].contribution_unit, unit);
            assert_eq!(result.components[0].sensitivity_output_unit, unit);
            assert_eq!(result.expanded_uncertainty, 3.0);
        }
    }

    #[test]
    fn normalized_rss_handles_large_and_small_scales() {
        for scale in [1e200, 1e-200] {
            let result = budget(vec![
                component("a", 3.0 * scale, 1.0, UncertaintyUnit::Millimeter),
                component("b", 4.0 * scale, 1.0, UncertaintyUnit::Millimeter),
            ]).calculate().unwrap();
            assert!((result.combined_standard_uncertainty / scale - 5.0).abs() < 1e-14);
            assert!((result.expanded_uncertainty / scale - 10.0).abs() < 1e-14);
        }
    }

    #[test]
    fn combines_minimum_subnormal_contributions_before_rounding_output() {
        let input = budget((0..64).map(|i| {
            component(&format!("c{i}"), f64::from_bits(1), 1.0, UncertaintyUnit::Pixel)
        }).collect());
        let result = input.calculate().unwrap();
        assert_eq!(result.combined_standard_uncertainty.to_bits(), 8);
        assert_eq!(result.expanded_uncertainty.to_bits(), 16);

        // A 3-4-12 triangle also remains accurate in subnormal output units,
        // independently of whether the largest contribution is first or last.
        for values in [[3_u64, 4, 12], [12_u64, 4, 3]] {
            let input = budget(values.iter().enumerate().map(|(i, bits)| {
                component(&format!("c{i}"), f64::from_bits(*bits), 1.0, UncertaintyUnit::Pixel)
            }).collect());
            let result = input.calculate().unwrap();
            assert_eq!(result.combined_standard_uncertainty.to_bits(), 13);
            assert_eq!(result.expanded_uncertainty.to_bits(), 26);
        }
    }

    #[test]
    fn rejects_invalid_structure_and_text() {
        let valid = || budget(vec![component("a", 1.0, 1.0, UncertaintyUnit::Millimeter)]);
        let mut input = valid();
        input.components.clear();
        assert!(input.calculate().is_err());
        input.components = (0..65).map(|i| component(&format!("c{i}"), 1.0, 1.0, UncertaintyUnit::Pixel)).collect();
        assert!(input.calculate().is_err());
        input = valid();
        input.components.push(component(" a ", 1.0, 1.0, UncertaintyUnit::Pixel));
        assert!(input.calculate().is_err());
        for name in ["   ".to_string(), "a".repeat(257)] {
            input = valid(); input.components[0].name = name;
            assert!(input.calculate().is_err());
        }
        for text in [" \n ".to_string(), "a".repeat(4097)] {
            input = valid(); input.components[0].source = text.clone();
            assert!(input.calculate().is_err());
            input = valid(); input.independence_justification = text;
            assert!(input.calculate().is_err());
        }
        input = valid(); input.output_unit = UncertaintyUnit::Dimensionless;
        assert!(input.calculate().is_err());
    }

    #[test]
    fn rejects_invalid_numbers_and_numerical_limits() {
        let valid = || budget(vec![component("a", 1.0, 1.0, UncertaintyUnit::Pixel)]);
        for k in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let mut input = valid(); input.coverage_factor = k;
            assert!(input.calculate().is_err());
        }
        for u in [-1.0, f64::NAN, f64::INFINITY] {
            let mut input = valid(); input.components[0].standard_uncertainty = u;
            assert!(input.calculate().is_err());
        }
        for c in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut input = valid(); input.components[0].sensitivity_coefficient = c;
            assert!(input.calculate().is_err());
        }
        for (u, c, k) in [
            (0.0, 1.0, 2.0), (1.0, 0.0, 2.0),
            (f64::MAX, 2.0, 1.0), (f64::MAX, 1.0, 2.0),
            (f64::MIN_POSITIVE, f64::MIN_POSITIVE, 1.0),
            (f64::from_bits(1), 1.0, 0.5),
        ] {
            let mut input = valid();
            input.components[0].standard_uncertainty = u;
            input.components[0].sensitivity_coefficient = c;
            input.coverage_factor = k;
            assert!(input.calculate().is_err(), "u={u}, c={c}, k={k}");
        }
        let input = budget(vec![
            component("a", f64::MAX, 1.0, UncertaintyUnit::Pixel),
            component("b", f64::MAX, 1.0, UncertaintyUnit::Pixel),
        ]);
        assert!(input.calculate().is_err());
    }

    #[test]
    fn serialization_is_explicit_and_rejects_unknown_fields() {
        let input = budget(vec![component("a", 1.0, 1.0, UncertaintyUnit::Pixel)]);
        let mut json = serde_json::to_value(&input).unwrap();
        assert_eq!(json["output_unit"], "millimeter");
        assert_eq!(json["components"][0]["input_unit"], "pixel");
        assert_eq!(serde_json::from_value::<IndependentUncertaintyBudget>(json.clone()).unwrap(), input);
        json["extra"] = true.into();
        assert!(serde_json::from_value::<IndependentUncertaintyBudget>(json).is_err());
        let mut json = serde_json::to_value(&input).unwrap();
        json["components"][0]["extra"] = true.into();
        assert!(serde_json::from_value::<IndependentUncertaintyBudget>(json).is_err());
        assert!(serde_json::from_value::<UncertaintyUnit>("radian".into()).is_err());
        let result = input.calculate().unwrap();
        let mut json = serde_json::to_value(&result).unwrap();
        json["extra"] = true.into();
        assert!(serde_json::from_value::<IndependentUncertaintyResult>(json).is_err());
        let mut json = serde_json::to_value(&result.components[0]).unwrap();
        json["extra"] = true.into();
        assert!(serde_json::from_value::<SensitivityContribution>(json).is_err());
        let schema = serde_json::to_value(schemars::schema_for!(IndependentUncertaintyBudget)).unwrap();
        assert_eq!(schema["additionalProperties"], false);
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct UncertaintyBudgetComponent {
    pub name: String,
    pub standard_uncertainty_mm: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct UncertaintyBudget {
    pub method: String,
    pub coverage_factor: f64,
    pub components: Vec<UncertaintyBudgetComponent>,
    pub combination_assumption: String,
}

impl UncertaintyBudget {
    /// Combines user-supplied standard uncertainties by root-sum-square.
    pub fn expanded_uncertainty_mm(&self) -> Result<f64, String> {
        if !self.coverage_factor.is_finite() || self.coverage_factor <= 0.0 {
            return Err("Le facteur de couverture k doit être fini et strictement positif.".into());
        }
        if self.components.is_empty()
            || self.components.iter().any(|component| {
                component.name.trim().is_empty()
                    || !component.standard_uncertainty_mm.is_finite()
                    || component.standard_uncertainty_mm < 0.0
            })
        {
            return Err("Chaque composante doit avoir un nom et une incertitude-type finie et positive ou nulle.".into());
        }
        let sum_of_squares = self.components.iter()
            .map(|component| component.standard_uncertainty_mm.powi(2))
            .sum::<f64>();
        if !sum_of_squares.is_finite() || sum_of_squares <= 0.0 {
            return Err("Le budget doit contenir au moins une composante non nulle.".into());
        }
        let expanded = self.coverage_factor * sum_of_squares.sqrt();
        if !expanded.is_finite() || expanded <= 0.0 {
            return Err("Le calcul du budget d'incertitude dépasse les limites numériques.".into());
        }
        Ok(expanded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn component(name: &str, uncertainty: f64) -> UncertaintyBudgetComponent {
        UncertaintyBudgetComponent { name: name.into(), standard_uncertainty_mm: uncertainty }
    }

    #[test]
    fn combines_independent_components_by_root_sum_square_then_coverage_factor() {
        let budget = UncertaintyBudget {
            method: "Essai".into(),
            coverage_factor: 2.0,
            components: vec![component("étalon", 0.03), component("lecture", 0.04)],
            combination_assumption: "Indépendantes".into(),
        };

        assert!((budget.expanded_uncertainty_mm().unwrap() - 0.1).abs() < 1e-12);
    }

    #[test]
    fn rejects_empty_zero_invalid_and_overflowing_budgets() {
        let make_budget = |components, coverage_factor| UncertaintyBudget {
            method: "Essai".into(), coverage_factor, components,
            combination_assumption: "Indépendantes".into(),
        };
        assert!(make_budget(Vec::new(), 2.0).expanded_uncertainty_mm().is_err());
        assert!(make_budget(vec![component("zéro", 0.0)], 2.0).expanded_uncertainty_mm().is_err());
        assert!(make_budget(vec![component("", 0.1)], 2.0).expanded_uncertainty_mm().is_err());
        assert!(make_budget(vec![component("négative", -0.1)], 2.0).expanded_uncertainty_mm().is_err());
        assert!(make_budget(vec![component("non finie", f64::NAN)], 2.0).expanded_uncertainty_mm().is_err());
        assert!(make_budget(vec![component("trop grande", f64::MAX)], 2.0).expanded_uncertainty_mm().is_err());
        assert!(make_budget(vec![component("valide", 0.1)], f64::INFINITY).expanded_uncertainty_mm().is_err());
    }
}
