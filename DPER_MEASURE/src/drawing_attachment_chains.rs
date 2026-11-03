//! Complete, bounded endpoint topology for explicitly scoped source paths.
//! Joining is a geometric hypothesis; source endpoints are never snapped.
use super::*;

pub(super) const JOINT_TOLERANCE_PT: f64 = 0.25;
pub(super) const MAX_CHAIN_PATHS: usize = 8;
pub(super) const CHAIN_PREFIX: &str = "endpoint_attachment_chain_hypothesis:";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ChainSupportTrace {
    pub support_id: String,
    pub support_fingerprint: String,
    pub source: crate::dper_pdf_geometry::VectorSource,
    pub style: crate::dper_pdf_geometry::VectorStyle,
    pub directed_points: Vec<Point>,
    pub start_endpoint: Point,
    pub end_endpoint: Point,
    pub start_endpoint_index: usize,
    pub end_endpoint_index: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ChainJointTrace {
    pub from_support_id: String,
    pub to_support_id: String,
    pub from_endpoint: Point,
    pub to_endpoint: Point,
    pub gap_pt: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ChainTrace {
    pub schema_version: u32,
    pub scope_fingerprint: String,
    pub ordered_supports: Vec<ChainSupportTrace>,
    pub joints: Vec<ChainJointTrace>,
    pub joint_tolerance_pt: f64,
    pub tolerance_pt: f64,
    pub text_endpoint: Point,
    pub circle_endpoint: Point,
    pub text_gap_pt: f64,
    pub circle_gap_pt: f64,
    pub matching_feature_ids: Vec<String>,
    pub matching_geometry_ids: Vec<String>,
    pub unique_within_selected_view: bool,
    pub requires_review: bool,
    pub semantics: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TopologyError { Budget, Ambiguous, ComponentTooLong }

pub(super) struct ChainTopology {
    /// (support index, source point order is forward), from one free end to the other.
    pub components: Vec<Vec<(usize, bool)>>,
    pub multi_path_supports: BTreeSet<String>,
    /// Rejected contact components do not invalidate disconnected components.
    pub rejected_component: Option<TopologyError>,
    pub remaining_work: usize,
}

fn charge(remaining: &mut usize) -> Result<(), TopologyError> {
    if *remaining == 0 { return Err(TopologyError::Budget); }
    *remaining -= 1;
    Ok(())
}

fn endpoint(path: &VectorPrimitive, end: usize) -> Point {
    path.points[if end == 0 { 0 } else { path.points.len() - 1 }]
}

pub(super) fn point_segment_distance(point: Point, start: Point, end: Point) -> f64 {
    let (dx, dy) = (end.x - start.x, end.y - start.y);
    let parameter = (((point.x - start.x) * dx + (point.y - start.y) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    (point.x - (start.x + parameter * dx)).hypot(point.y - (start.y + parameter * dy))
}

// A gap may join nearby endpoints, but a real intersection is permissible only
// at BOTH original endpoints. The joint tolerance must never turn an interior
// crossing or T contact into an endpoint contact.
fn intersection_is_local_joint(a: Point, b: Point, c: Point, d: Point, left: Point, right: Point) -> bool {
    let denominator = (b.x - a.x) * (d.y - c.y) - (b.y - a.y) * (d.x - c.x);
    let local = |point: Point| point.dist(left) <= 1e-9 && point.dist(right) <= 1e-9;
    if denominator.abs() > 1e-12 {
        let parameter = ((c.x - a.x) * (d.y - c.y) - (c.y - a.y) * (d.x - c.x)) / denominator;
        local(Point { x: a.x + parameter * (b.x - a.x), y: a.y + parameter * (b.y - a.y) })
    } else {
        let contacts: Vec<_> = [a, b, c, d].into_iter().filter(|point|
            touches_segment(a, b, *point) && touches_segment(c, d, *point)).collect();
        !contacts.is_empty() && contacts.into_iter().all(local)
    }
}

pub(super) fn topology(supports: &[&VectorPrimitive], mut remaining: usize) -> Result<ChainTopology, TopologyError> {
    let mut neighbours = vec![[None; 2]; supports.len()];
    let mut contacts = vec![BTreeSet::new(); supports.len()];
    let mut invalid = vec![false; supports.len()];
    // Complete endpoint enumeration: no greedy pairing, transitive snapping or
    // use of source ordering to choose between several possible neighbours.
    for left in 0..supports.len() {
        for right in left + 1..supports.len() {
            for left_end in 0..2 {
                for right_end in 0..2 {
                    charge(&mut remaining)?;
                    if endpoint(supports[left], left_end).dist(endpoint(supports[right], right_end)) <= JOINT_TOLERANCE_PT {
                        contacts[left].insert(right);
                        contacts[right].insert(left);
                        if neighbours[left][left_end].is_some() || neighbours[right][right_end].is_some() {
                            invalid[left] = true;
                            invalid[right] = true;
                        } else {
                            neighbours[left][left_end] = Some((right, right_end));
                            neighbours[right][right_end] = Some((left, left_end));
                        }
                    }
                }
            }
        }
    }
    for left in 0..supports.len() {
        for right in left + 1..supports.len() {
            let (a, b) = (supports[left], supports[right]);
            for (path, other) in [(a, b), (b, a)] {
                for end in 0..2 {
                    let point = endpoint(path, end);
                    for segment in other.points.windows(2) {
                        charge(&mut remaining)?;
                        if point_segment_distance(point, segment[0], segment[1]) <= JOINT_TOLERANCE_PT
                            && point.dist(endpoint(other, 0)) > JOINT_TOLERANCE_PT
                            && point.dist(endpoint(other, 1)) > JOINT_TOLERANCE_PT {
                            contacts[left].insert(right);
                            contacts[right].insert(left);
                            invalid[left] = true;
                            invalid[right] = true; // endpoint to interior / T contact
                        }
                    }
                }
            }
            for first in a.points.windows(2) {
                for second in b.points.windows(2) {
                    charge(&mut remaining)?;
                    if segments_intersect(first[0], first[1], second[0], second[1]) {
                        contacts[left].insert(right);
                        contacts[right].insert(left);
                        let local_joint = (0..2).any(|end| neighbours[left][end].is_some_and(|(other, other_end)|
                            other == right && intersection_is_local_joint(first[0], first[1], second[0], second[1],
                                endpoint(a, end), endpoint(b, other_end))));
                        if !local_joint { invalid[left] = true; invalid[right] = true; }
                    }
                }
            }
        }
    }
    let mut visited = vec![false; supports.len()];
    let mut result = ChainTopology { components: Vec::new(), multi_path_supports: BTreeSet::new(),
        rejected_component: None, remaining_work: 0 };
    for seed in 0..supports.len() {
        if visited[seed] { continue; }
        let mut pending = vec![seed];
        let mut component = Vec::new();
        visited[seed] = true;
        while let Some(path) = pending.pop() {
            component.push(path);
            for neighbour in &contacts[path] {
                if !visited[*neighbour] { visited[*neighbour] = true; pending.push(*neighbour); }
            }
        }
        if component.len() > 1 {
            for path in &component { result.multi_path_supports.insert(supports[*path].id.clone()); }
        }
        if component.len() > MAX_CHAIN_PATHS {
            result.rejected_component = Some(TopologyError::ComponentTooLong);
            continue;
        }
        let mut free = Vec::new();
        for path in &component {
            for end in 0..2 { if neighbours[*path][end].is_none() { free.push((*path, end)); } }
        }
        if component.iter().any(|path| invalid[*path]) || free.len() != 2 {
            result.rejected_component.get_or_insert(TopologyError::Ambiguous);
            continue; // includes closed cycles and every path touching a bad component
        }
        let mut order = Vec::new();
        let (mut path, mut entry) = free[0];
        loop {
            if order.iter().any(|(seen, _)| *seen == path) { return Err(TopologyError::Ambiguous); }
            order.push((path, entry == 0));
            match neighbours[path][1 - entry] {
                Some((next, next_entry)) => { path = next; entry = next_entry; },
                None => break,
            }
        }
        if order.len() != component.len() { return Err(TopologyError::Ambiguous); }
        result.components.push(order);
    }
    if result.components.is_empty() {
        if let Some(error) = result.rejected_component { return Err(error); }
    }
    result.remaining_work = remaining;
    Ok(result)
}

pub(super) fn directed_sources(supports: &[&VectorPrimitive], order: &[(usize, bool)])
    -> Result<(Vec<ChainSupportTrace>, Vec<ChainJointTrace>), String> {
    let mut directed = Vec::new();
    let mut joints = Vec::new();
    for (index, forward) in order {
        let support = supports[*index];
        let mut points = support.points.clone();
        if !*forward { points.reverse(); }
        let start = points[0];
        let end = *points.last().unwrap();
        if let Some(previous) = directed.last() {
            let previous: &ChainSupportTrace = previous;
            joints.push(ChainJointTrace { from_support_id: previous.support_id.clone(), to_support_id: support.id.clone(),
                from_endpoint: previous.end_endpoint, to_endpoint: start, gap_pt: previous.end_endpoint.dist(start) });
        }
        directed.push(ChainSupportTrace {
            support_id: support.id.clone(), support_fingerprint: primitive_fingerprint(support)?,
            source: support.source.clone(), style: support.style.clone(), directed_points: points,
            start_endpoint: start, end_endpoint: end,
            start_endpoint_index: if *forward { 0 } else { support.points.len() - 1 },
            end_endpoint_index: if *forward { support.points.len() - 1 } else { 0 },
        });
    }
    Ok((directed, joints))
}

pub(super) fn validate_evidence(encoded: &str, primitives: &BTreeMap<&str, &VectorPrimitive>) -> Result<(), String> {
    let trace: ChainTrace = serde_json::from_str(encoded)
        .map_err(|_| "Trace de chaîne d'attachement invalide ; reproposez l'association.".to_string())?;
    if trace.schema_version != 1 || !trace.requires_review || trace.semantics != NO_SEMANTICS
        || trace.tolerance_pt != ATTACHMENT_TOLERANCE_PT || trace.joint_tolerance_pt != JOINT_TOLERANCE_PT
        || !(2..=MAX_CHAIN_PATHS).contains(&trace.ordered_supports.len())
        || trace.joints.len() + 1 != trace.ordered_supports.len()
        || trace.joints.iter().any(|joint| !joint.gap_pt.is_finite() || joint.gap_pt > JOINT_TOLERANCE_PT || joint.gap_pt < 0.0) {
        return Err("Trace de chaîne d'attachement incohérente ; reproposez l'association.".into());
    }
    validate_source_traces(&trace.ordered_supports, &trace.joints, primitives)?;
    if trace.text_endpoint != trace.ordered_supports[0].start_endpoint
        || trace.circle_endpoint != trace.ordered_supports.last().unwrap().end_endpoint {
        return Err("Extrémités de chaîne incohérentes.".into());
    }
    Ok(())
}

pub(super) fn validate_source_traces(sources: &[ChainSupportTrace], joints: &[ChainJointTrace],
    primitives: &BTreeMap<&str, &VectorPrimitive>) -> Result<(), String> {
    if !(1..=MAX_CHAIN_PATHS).contains(&sources.len()) || joints.len() + 1 != sources.len() {
        return Err("Supports ou joints d'attachement incohérents.".into());
    }
    let mut seen = BTreeSet::new();
    for source in sources {
        if !seen.insert(&source.support_id) { return Err("Support de chaîne dupliqué.".into()); }
        let support = primitives.get(source.support_id.as_str()).ok_or("Support de chaîne absent ; reproposez l'association.")?;
        if support.points.len() < 2 || primitive_fingerprint(support)? != source.support_fingerprint
            || fingerprint(&support.source)? != fingerprint(&source.source)?
            || fingerprint(&support.style)? != fingerprint(&source.style)? {
            return Err("La source d'un support de chaîne a changé ; reproposez l'association et sa revue.".into());
        }
        let last = support.points.len() - 1;
        let mut points = support.points.clone();
        match (source.start_endpoint_index, source.end_endpoint_index) {
            (0, end) if end == last => {},
            (start, 0) if start == last => points.reverse(),
            _ => return Err("Orientation de support de chaîne incohérente.".into()),
        }
        if source.directed_points != points || source.start_endpoint != points[0]
            || source.end_endpoint != points[last] {
            return Err("Coordonnées de support de chaîne incohérentes.".into());
        }
    }
    for (joint, pair) in joints.iter().zip(sources.windows(2)) {
        if joint.from_support_id != pair[0].support_id || joint.to_support_id != pair[1].support_id
            || joint.from_endpoint != pair[0].end_endpoint || joint.to_endpoint != pair[1].start_endpoint
            || !joint.gap_pt.is_finite() || joint.gap_pt < 0.0 || joint.gap_pt > JOINT_TOLERANCE_PT
            || joint.gap_pt != joint.from_endpoint.dist(joint.to_endpoint) {
            return Err("Joint de chaîne incohérent.".into());
        }
    }
    Ok(())
}
