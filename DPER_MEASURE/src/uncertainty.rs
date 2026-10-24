//! Combined standard-uncertainty budget supplied by the operator.
use serde::{Deserialize, Serialize};

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
