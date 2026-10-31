//! Explicit measurement dependencies. Revisions survive geometry undo/redo.
//! This graph records freshness only; it never grants validation or conformity.
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::geometry::Measurement;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct DependencyGraph {
    versions: BTreeMap<u64, u64>,
    stale: BTreeSet<u64>,
}

impl DependencyGraph {
    pub(crate) fn revision(&self, id: u64) -> u64 {
        self.versions.get(&id).copied().unwrap_or(0)
    }

    pub(crate) fn is_stale(&self, id: u64) -> bool {
        self.stale.contains(&id)
    }

    /// Invalidate roots and descendants, including deleted roots. Do not validate
    /// here: deletion can temporarily leave edges pointing to a missing parent.
    pub(crate) fn invalidate(&mut self, measurements: &[Measurement], roots: &[u64]) -> Vec<u64> {
        let mut children: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
        let mut derived = BTreeSet::new();
        for measurement in measurements {
            if !measurement.source_measurement_ids.is_empty() {
                derived.insert(measurement.id);
            }
            for parent in &measurement.source_measurement_ids {
                children.entry(*parent).or_default().push(measurement.id);
            }
        }
        let mut affected = BTreeSet::new();
        let mut pending: VecDeque<u64> = roots.iter().copied().collect();
        while let Some(id) = pending.pop_front() {
            if !affected.insert(id) {
                continue;
            }
            if let Some(descendants) = children.get(&id) {
                pending.extend(descendants.iter().copied());
            }
        }
        for id in &affected {
            let version = self.versions.entry(*id).or_default();
            // Never wrap a generation back to an earlier proof's revision.
            *version = version.saturating_add(1);
            if derived.contains(id) {
                self.stale.insert(*id);
            }
        }
        affected.into_iter().collect()
    }

    /// Only the owner of a newly computed node may call this, never history
    /// restoration. A new node does not refresh any of its descendants.
    pub(crate) fn mark_created(&mut self, id: u64) {
        self.versions.entry(id).or_insert(1);
        self.stale.remove(&id);
    }

    /// Imported generations and freshness claims are not session evidence.
    pub(crate) fn restore_import(measurements: &[Measurement]) -> Result<Self, String> {
        validate_graph(measurements)?;
        let mut graph = Self::default();
        for measurement in measurements {
            graph.versions.insert(measurement.id, 1);
            if !measurement.source_measurement_ids.is_empty() {
                graph.stale.insert(measurement.id);
            }
        }
        Ok(graph)
    }
}

/// Validate structure without requiring derived results to be current.
pub(crate) fn validate_graph(measurements: &[Measurement]) -> Result<(), String> {
    let mut by_id = BTreeMap::new();
    for measurement in measurements {
        if by_id.insert(measurement.id, measurement).is_some() {
            return Err(format!("Identifiant de mesure dupliqué : {}.", measurement.id));
        }
    }
    let mut children: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
    let mut remaining_parents = BTreeMap::new();
    for measurement in measurements {
        let mut unique = BTreeSet::new();
        for parent_id in &measurement.source_measurement_ids {
            if !unique.insert(*parent_id) {
                return Err(format!("Parent {parent_id} dupliqué pour la mesure {}.", measurement.id));
            }
            let parent = by_id.get(parent_id).ok_or_else(|| format!(
                "Parent {parent_id} absent pour la mesure {}.", measurement.id))?;
            if parent.capture_id != measurement.capture_id {
                return Err(format!("La mesure {} et son parent {parent_id} appartiennent à des captures différentes.", measurement.id));
            }
            children.entry(*parent_id).or_default().push(measurement.id);
        }
        remaining_parents.insert(measurement.id, unique.len());
    }
    // Kahn's algorithm handles arbitrarily deep graphs without recursion.
    let mut ready: VecDeque<u64> = remaining_parents.iter()
        .filter_map(|(id, count)| (*count == 0).then_some(*id)).collect();
    let mut visited = 0;
    while let Some(id) = ready.pop_front() {
        visited += 1;
        if let Some(descendants) = children.get(&id) {
            for child in descendants {
                let count = remaining_parents.get_mut(child).unwrap();
                *count -= 1;
                if *count == 0 {
                    ready.push_back(*child);
                }
            }
        }
    }
    if visited != measurements.len() {
        return Err("Le graphe de dépendances des mesures contient un cycle.".into());
    }
    Ok(())
}

/// Check explicit inputs before creating a new derived node. Freshness is the
/// owner's graph projection on Measurement, not a caller-supplied claim.
pub(crate) fn validate_sources(
    measurements: &[Measurement],
    capture_id: &str,
    parent_ids: &[u64],
) -> Result<(), String> {
    let mut unique = BTreeSet::new();
    for parent_id in parent_ids {
        if !unique.insert(*parent_id) {
            return Err(format!("Parent de mesure dupliqué : {parent_id}."));
        }
        let mut matches = measurements.iter().filter(|measurement| measurement.id == *parent_id);
        let parent = matches.next().ok_or_else(|| format!("Mesure source {parent_id} absente."))?;
        if matches.next().is_some() {
            return Err(format!("Identifiant de mesure source ambigu : {parent_id}."));
        }
        if parent.capture_id != capture_id {
            return Err(format!("La mesure source {parent_id} appartient à une autre capture."));
        }
        if parent.dependency_stale {
            return Err(format!("La mesure source {parent_id} doit être recalculée."));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn measurement(id: u64, parents: &[u64]) -> Measurement {
        serde_json::from_value(serde_json::json!({
            "id": id, "kind": "Distance", "points": [{"x": 0.0, "y": 0.0}, {"x": 1.0, "y": 0.0}],
            "label": format!("M{id}"), "capture_id": "capture-a",
            "source_measurement_ids": parents, "dependency_stale": false
        })).unwrap()
    }

    #[test]
    fn invalidation_reaches_multiple_levels_and_leaves_unrelated_sibling_current() {
        let measurements = vec![measurement(1, &[]), measurement(2, &[1]),
            measurement(3, &[2]), measurement(4, &[1]), measurement(5, &[2, 3])];
        let mut graph = DependencyGraph::default();
        for measurement in &measurements { graph.mark_created(measurement.id); }
        assert_eq!(graph.invalidate(&measurements, &[2, 3, 2]), vec![2, 3, 5]);
        assert_eq!(graph.revision(1), 1);
        assert_eq!(graph.revision(4), 1);
        assert!(!graph.is_stale(1));
        assert!(!graph.is_stale(4));
        for id in [2, 3, 5] {
            assert!(graph.is_stale(id));
            assert_eq!(graph.revision(id), 2);
        }
        graph.mark_created(2);
        assert!(!graph.is_stale(2));
        assert!(graph.is_stale(3));
        assert!(graph.is_stale(5));
        assert_eq!(graph.revision(2), 2);
    }

    #[test]
    fn deleted_root_tombstone_and_undo_keep_increasing_generations() {
        let parent = measurement(1, &[]);
        let child = measurement(2, &[1]);
        let mut graph = DependencyGraph::default();
        graph.mark_created(1);
        graph.mark_created(2);
        assert_eq!(graph.invalidate(&[child.clone()], &[1]), vec![1, 2]);
        assert_eq!(graph.revision(1), 2);
        // Undo restores geometry, then invalidates; it never calls mark_created.
        graph.invalidate(&[parent, child], &[1]);
        assert_eq!(graph.revision(1), 3);
        assert_eq!(graph.revision(2), 3);
        assert!(graph.is_stale(2));
        assert!(!graph.is_stale(1));
        assert_eq!(graph.invalidate(&[], &[99]), vec![99]);
        assert_eq!(graph.revision(99), 1);
        assert_eq!(graph.revision(100), 0);
    }

    #[test]
    fn graph_rejects_cycles_missing_duplicate_and_cross_capture_edges() {
        assert!(validate_graph(&[measurement(1, &[1])]).is_err());
        assert!(validate_graph(&[measurement(1, &[2]), measurement(2, &[1])]).is_err());
        assert!(validate_graph(&[measurement(1, &[99])]).is_err());
        assert!(validate_graph(&[measurement(1, &[]), measurement(1, &[])]).is_err());
        assert!(validate_graph(&[measurement(1, &[]), measurement(2, &[1, 1])]).is_err());
        let mut other_capture = measurement(2, &[1]);
        other_capture.capture_id = "capture-b".into();
        assert!(validate_graph(&[measurement(1, &[]), other_capture]).is_err());
        assert!(validate_graph(&[measurement(3, &[1, 2]), measurement(1, &[]), measurement(2, &[1])]).is_ok());
        assert!(validate_graph(&[]).is_ok());
    }

    #[test]
    fn input_validation_rejects_stale_ambiguous_missing_duplicate_or_wrong_capture_sources() {
        let mut parent = measurement(1, &[]);
        assert!(validate_sources(&[parent.clone()], "capture-a", &[1]).is_ok());
        assert!(validate_sources(&[parent.clone()], "capture-a", &[1, 1]).is_err());
        assert!(validate_sources(&[parent.clone()], "capture-a", &[99]).is_err());
        assert!(validate_sources(&[parent.clone()], "capture-b", &[1]).is_err());
        assert!(validate_sources(&[parent.clone(), parent.clone()], "capture-a", &[1]).is_err());
        parent.dependency_stale = true;
        assert!(validate_sources(&[parent], "capture-a", &[1]).is_err());
        assert!(validate_sources(&[], "capture-a", &[]).is_ok());
    }

    #[test]
    fn import_discards_claims_and_marks_every_derived_node_stale() {
        let measurements = vec![measurement(1, &[]), measurement(2, &[1]), measurement(3, &[2])];
        let forged: DependencyGraph = serde_json::from_value(serde_json::json!({
            "versions": {"1": 999, "2": 999, "3": 999}, "stale": []
        })).unwrap();
        assert_eq!(forged.revision(2), 999);
        let restored = DependencyGraph::restore_import(&measurements).unwrap();
        for id in [1, 2, 3] { assert_eq!(restored.revision(id), 1); }
        assert!(!restored.is_stale(1));
        assert!(restored.is_stale(2));
        assert!(restored.is_stale(3));
        assert!(DependencyGraph::restore_import(&[measurement(2, &[99])]).is_err());
    }

    #[test]
    fn invalidation_is_bounded_even_while_graph_structure_is_invalid() {
        let measurements = vec![measurement(1, &[2]), measurement(2, &[1])];
        let mut graph = DependencyGraph::default();
        assert_eq!(graph.invalidate(&measurements, &[1]), vec![1, 2]);
        assert_eq!(graph.revision(1), 1);
        assert_eq!(graph.revision(2), 1);
        assert!(graph.is_stale(1));
        assert!(graph.is_stale(2));
    }
}
