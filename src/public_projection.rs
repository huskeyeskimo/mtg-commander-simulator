//! Exact, finite-schema public incidence. Runtime identities construct joins;
//! they never enter descriptors, canonical preferences or relationship bytes.
//! A decision witness is derived, immutable and invalid after mutation.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::marker::PhantomData;

use bincode::Options;
use serde::{Deserialize, Serialize};

use crate::card::{ExactObjectRef, ObjectId, ZoneType};
use crate::game::{GameState, PlayerView, StackSource, Target, TriggerContext};
use crate::simulation::TerminationReason;

pub const PROJECTION_SCHEMA: u16 = 1;
type EncodingResult<T> = Result<T, TerminationReason>;

pub fn encode<T: Serialize>(value: &T) -> EncodingResult<Vec<u8>> {
    bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_big_endian()
        .serialize(value)
        .map_err(|_| TerminationReason::StateEncoding)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum VertexKind {
    CurrentObject,
    HistoricalIdentity,
    HistoricalFrame,
    ContinuousEffect,
    Occurrence,
    StackOrOperation,
    LegacyReference,
    Replacement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum EdgeKind {
    LiveAttached,
    HistoricalAttached,
    FrameOf,
    EffectSource,
    EffectTarget,
    OccurrenceSource,
    OccurrenceSubject,
    OccurrenceAfter,
    LinkedExile,
    StackSource,
    StackTarget,
    Blocks,
    DamageAssignment,
    ContinuationMember,
    ContinuationTarget,
    ReplacementSource,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Vertex {
    pub kind: VertexKind,
    pub facts: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Edge {
    pub source: usize,
    pub target: usize,
    pub kind: EdgeKind,
    pub payload: Vec<u8>,
}

/// Bounded diagnostics/oracle input, with only the admitted typed schema.
#[derive(Debug, Clone)]
pub struct PublicGraph {
    pub schema: u16,
    pub vertices: Vec<Vertex>,
    pub edges: Vec<Edge>,
}

impl Default for PublicGraph {
    fn default() -> Self {
        Self {
            schema: PROJECTION_SCHEMA,
            vertices: Vec::new(),
            edges: Vec::new(),
        }
    }
}

impl PublicGraph {
    pub fn vertex<T: Serialize>(&mut self, kind: VertexKind, facts: &T) -> EncodingResult<usize> {
        let index = self.vertices.len();
        self.vertices.push(Vertex {
            kind,
            facts: encode(facts)?,
        });
        Ok(index)
    }
    pub fn edge<T: Serialize>(
        &mut self,
        source: usize,
        target: usize,
        kind: EdgeKind,
        payload: &T,
    ) -> EncodingResult<()> {
        if source >= self.vertices.len() || target >= self.vertices.len() {
            return Err(TerminationReason::StateEncoding);
        }
        self.edges.push(Edge {
            source,
            target,
            kind,
            payload: encode(payload)?,
        });
        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct ProjectionDiagnostics {
    pub vertices: usize,
    pub edges_by_type: BTreeMap<EdgeKind, usize>,
    pub refinement_rounds: usize,
    pub unresolved_cell_sizes: Vec<usize>,
    pub exact_search_nodes: usize,
    pub candidate_encodings: usize,
    pub verified_automorphisms: usize,
    pub pruned_branches: usize,
    pub isolated_current_components: usize,
    pub isolated_encoding_reuses: usize,
    pub general_tree_components: usize,
    pub full_aggregate_encodings: usize,
    pub inventory_nanos: u128,
    pub component_nanos: u128,
    pub tree_nanos: u128,
    pub cyclic_nanos: u128,
    pub witness_nanos: u128,
    pub supplement_nanos: u128,
    pub total_nanos: u128,
}

#[derive(Debug, Clone)]
pub struct ComponentMetadata {
    pub vertices: Vec<usize>,
    pub encoding: Vec<u8>,
    pub is_tree: bool,
    pub relational: bool,
}

#[derive(Debug, Clone)]
pub struct GraphNormalization {
    pub encoding: Vec<u8>,
    pub components: Vec<ComponentMetadata>,
    pub diagnostics: ProjectionDiagnostics,
}

struct ComponentNormalization {
    components: Vec<ComponentMetadata>,
    diagnostics: ProjectionDiagnostics,
}

#[cfg(test)]
mod approved_performance_tests {
    use super::*;

    #[test]
    fn isolated_components_preserve_general_leaf_bytes_and_multiplicity() {
        let mut graph = PublicGraph::default();
        for facts in [4u8, 4, 8, 4, 8, 12] {
            graph.vertex(VertexKind::CurrentObject, &facts).unwrap();
        }
        graph.vertex(VertexKind::HistoricalIdentity, &4u8).unwrap();
        let normalized = normalize_graph(&graph).unwrap();
        assert_eq!(normalized.components.len(), graph.vertices.len());
        assert_eq!(normalized.diagnostics.isolated_current_components, 6);
        assert_eq!(normalized.diagnostics.isolated_encoding_reuses, 3);
        assert_eq!(normalized.diagnostics.general_tree_components, 1);
        assert_eq!(normalized.diagnostics.full_aggregate_encodings, 1);
        assert_eq!(normalized.diagnostics.exact_search_nodes, 0);
        let mut represented = Vec::new();
        for component in &normalized.components {
            assert_eq!(component.vertices.len(), 1);
            let v = component.vertices[0];
            represented.push(v);
            let local = PublicGraph {
                vertices: vec![graph.vertices[v].clone()],
                ..PublicGraph::default()
            };
            let adjacency = Adjacency::new(&local).unwrap();
            let order = tree_order(&local, &adjacency).unwrap().unwrap();
            assert_eq!(
                component.encoding,
                complete_encoding(&local, &order).unwrap()
            );
            assert_eq!(component.relational, v == 6);
        }
        represented.sort_unstable();
        assert_eq!(represented, (0..graph.vertices.len()).collect::<Vec<_>>());
    }

    #[test]
    fn complete_incidence_prevents_singleton_reuse() {
        let mut graph = PublicGraph::default();
        for _ in 0..4 {
            graph.vertex(VertexKind::CurrentObject, &0u8).unwrap();
        }
        graph.edge(0, 0, EdgeKind::StackTarget, &7u8).unwrap();
        graph.edge(1, 2, EdgeKind::LiveAttached, &()).unwrap();
        graph.edge(2, 1, EdgeKind::EffectSource, &3u8).unwrap();
        let normalized = normalize_graph(&graph).unwrap();
        assert_eq!(normalized.diagnostics.isolated_current_components, 1);
        assert_eq!(normalized.diagnostics.isolated_encoding_reuses, 0);
        assert_eq!(normalized.diagnostics.general_tree_components, 2);
        assert_eq!(
            normalized
                .components
                .iter()
                .filter(|c| c.relational)
                .count(),
            2
        );
        let mut changed = graph.clone();
        changed.edges[0].payload = encode(&8u8).unwrap();
        assert_ne!(
            normalized.encoding,
            normalize_graph(&changed).unwrap().encoding
        );
    }

    #[test]
    fn private_components_match_public_output_without_aggregate() {
        let mut graph = PublicGraph::default();
        for _ in 0..5 {
            graph.vertex(VertexKind::CurrentObject, &0u8).unwrap();
        }
        graph.edge(0, 1, EdgeKind::LiveAttached, &()).unwrap();
        let private = normalize_components(&graph).unwrap();
        let public = normalize_graph(&graph).unwrap();
        assert_eq!(private.diagnostics.full_aggregate_encodings, 0);
        assert_eq!(public.diagnostics.full_aggregate_encodings, 1);
        assert_eq!(private.components.len(), public.components.len());
        for (a, b) in private.components.iter().zip(&public.components) {
            assert_eq!(a.vertices, b.vertices);
            assert_eq!(a.encoding, b.encoding);
            assert_eq!(a.is_tree, b.is_tree);
            assert_eq!(a.relational, b.relational);
        }
        assert_eq!(
            public.encoding,
            encode(&(
                PROJECTION_SCHEMA,
                private
                    .components
                    .iter()
                    .map(|c| &c.encoding)
                    .collect::<Vec<_>>(),
            ))
            .unwrap()
        );
    }

    #[test]
    fn structural_failures_precede_isolated_reuse() {
        let mut graph = PublicGraph::default();
        for _ in 0..3 {
            graph.vertex(VertexKind::CurrentObject, &0u8).unwrap();
        }
        graph.edges.push(Edge {
            source: 0,
            target: 3,
            kind: EdgeKind::LiveAttached,
            payload: vec![],
        });
        assert!(matches!(
            normalize_components(&graph),
            Err(TerminationReason::StateEncoding)
        ));
        assert!(matches!(
            normalize_graph(&graph),
            Err(TerminationReason::StateEncoding)
        ));
        graph.edges.clear();
        graph.schema += 1;
        assert!(matches!(
            normalize_components(&graph),
            Err(TerminationReason::StateEncoding)
        ));
        assert!(matches!(
            normalize_graph(&graph),
            Err(TerminationReason::StateEncoding)
        ));
    }
}

fn complete_encoding(graph: &PublicGraph, order: &[usize]) -> EncodingResult<Vec<u8>> {
    if order.len() != graph.vertices.len() {
        return Err(TerminationReason::StateEncoding);
    }
    let mut inverse = vec![usize::MAX; order.len()];
    for (rank, &vertex) in order.iter().enumerate() {
        if vertex >= order.len() || inverse[vertex] != usize::MAX {
            return Err(TerminationReason::StateEncoding);
        }
        inverse[vertex] = rank;
    }
    let vertices: Vec<_> = order.iter().map(|&v| &graph.vertices[v]).collect();
    let mut edges: Vec<_> = graph
        .edges
        .iter()
        .map(|edge| {
            (
                inverse[edge.source],
                inverse[edge.target],
                edge.kind,
                edge.payload.as_slice(),
            )
        })
        .collect();
    edges.sort();
    encode(&(PROJECTION_SCHEMA, vertices, edges))
}

struct Adjacency {
    incoming: Vec<Vec<(EdgeKind, Vec<u8>, usize)>>,
    outgoing: Vec<Vec<(EdgeKind, Vec<u8>, usize)>>,
    support: Vec<BTreeSet<usize>>,
    loops: Vec<Vec<(EdgeKind, Vec<u8>)>>,
    bundles: BTreeMap<(usize, usize), Vec<(u8, EdgeKind, Vec<u8>)>>,
}

impl Adjacency {
    fn new(graph: &PublicGraph) -> EncodingResult<Self> {
        let n = graph.vertices.len();
        let mut result = Self {
            incoming: vec![Vec::new(); n],
            outgoing: vec![Vec::new(); n],
            support: vec![BTreeSet::new(); n],
            loops: vec![Vec::new(); n],
            bundles: BTreeMap::new(),
        };
        for edge in &graph.edges {
            if edge.source >= n || edge.target >= n {
                return Err(TerminationReason::StateEncoding);
            }
            result.outgoing[edge.source].push((edge.kind, edge.payload.clone(), edge.target));
            result.incoming[edge.target].push((edge.kind, edge.payload.clone(), edge.source));
            if edge.source == edge.target {
                result.loops[edge.source].push((edge.kind, edge.payload.clone()));
            } else {
                result.support[edge.source].insert(edge.target);
                result.support[edge.target].insert(edge.source);
                result
                    .bundles
                    .entry((edge.source, edge.target))
                    .or_default()
                    .push((0, edge.kind, edge.payload.clone()));
                result
                    .bundles
                    .entry((edge.target, edge.source))
                    .or_default()
                    .push((1, edge.kind, edge.payload.clone()));
            }
        }
        for loops in &mut result.loops {
            loops.sort();
        }
        for bundle in result.bundles.values_mut() {
            bundle.sort();
        }
        Ok(result)
    }
}

fn labels<T: Ord + Clone>(facts: &[T]) -> Vec<usize> {
    let classes: BTreeMap<_, _> = facts
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .enumerate()
        .map(|(index, facts)| (facts, index))
        .collect();
    facts.iter().map(|fact| classes[fact]).collect()
}

fn refine(
    graph: &PublicGraph,
    adjacency: &Adjacency,
    marks: &[Option<usize>],
    diagnostics: &mut ProjectionDiagnostics,
) -> Vec<usize> {
    let facts: Vec<_> = graph
        .vertices
        .iter()
        .zip(marks)
        .map(|(vertex, &mark)| (vertex.clone(), mark))
        .collect();
    let mut colors = labels(&facts);
    loop {
        diagnostics.refinement_rounds += 1;
        let signatures: Vec<_> = (0..colors.len())
            .map(|v| {
                let mut outgoing: Vec<_> = adjacency.outgoing[v]
                    .iter()
                    .map(|(kind, payload, target)| (*kind, payload.clone(), colors[*target]))
                    .collect();
                let mut incoming: Vec<_> = adjacency.incoming[v]
                    .iter()
                    .map(|(kind, payload, source)| (*kind, payload.clone(), colors[*source]))
                    .collect();
                outgoing.sort();
                incoming.sort();
                (colors[v], incoming, outgoing)
            })
            .collect();
        let next = labels(&signatures);
        // Including old colors prevents merges. Equal class count therefore
        // proves partition stability, never vertex interchangeability.
        let stable = next.iter().copied().collect::<BTreeSet<_>>().len()
            == colors.iter().copied().collect::<BTreeSet<_>>().len();
        colors = next;
        if stable {
            return colors;
        }
    }
}

fn tree_order(graph: &PublicGraph, adjacency: &Adjacency) -> EncodingResult<Option<Vec<usize>>> {
    let n = graph.vertices.len();
    if n == 0 {
        return Ok(Some(Vec::new()));
    }
    if adjacency.support.iter().map(BTreeSet::len).sum::<usize>() != 2 * (n - 1) {
        return Ok(None);
    }
    // Called on a complete connected component. Parallel/reciprocal edges and
    // loops do not add simple-support edges, but all remain in the encoding.
    let mut alive: BTreeSet<_> = (0..n).collect();
    let mut degree: Vec<_> = adjacency.support.iter().map(BTreeSet::len).collect();
    let mut frontier: Vec<_> = alive.iter().copied().filter(|&v| degree[v] <= 1).collect();
    while alive.len() > 2 {
        if frontier.is_empty() {
            return Err(TerminationReason::StateEncoding);
        }
        let mut next = Vec::new();
        for vertex in frontier {
            alive.remove(&vertex);
            for &neighbor in &adjacency.support[vertex] {
                if alive.contains(&neighbor) {
                    degree[neighbor] -= 1;
                    if degree[neighbor] == 1 {
                        next.push(neighbor);
                    }
                }
            }
        }
        frontier = next;
    }
    fn rooted(
        graph: &PublicGraph,
        adjacency: &Adjacency,
        vertex: usize,
        parent: Option<usize>,
    ) -> EncodingResult<(Vec<u8>, Vec<usize>)> {
        let mut children = Vec::new();
        for &neighbor in &adjacency.support[vertex] {
            if Some(neighbor) == parent {
                continue;
            }
            let (code, order) = rooted(graph, adjacency, neighbor, Some(vertex))?;
            let bundle = adjacency
                .bundles
                .get(&(vertex, neighbor))
                .ok_or(TerminationReason::StateEncoding)?;
            children.push((encode(&(bundle, code))?, order));
        }
        children.sort_by(|a, b| a.0.cmp(&b.0));
        let code = encode(&(
            &graph.vertices[vertex],
            &adjacency.loops[vertex],
            children.iter().map(|(code, _)| code).collect::<Vec<_>>(),
        ))?;
        let mut order = vec![vertex];
        for (_, child) in children {
            order.extend(child);
        }
        Ok((code, order))
    }
    let mut best: Option<(Vec<u8>, Vec<usize>)> = None;
    for center in alive {
        let candidate = rooted(graph, adjacency, center, None)?;
        if best.as_ref().is_none_or(|(code, _)| candidate.0 < *code) {
            best = Some(candidate);
        }
    }
    Ok(Some(best.ok_or(TerminationReason::StateEncoding)?.1))
}

struct ExactSearch<'a> {
    graph: &'a PublicGraph,
    adjacency: &'a Adjacency,
    representatives: BTreeMap<Vec<u8>, Vec<usize>>,
    automorphisms: Vec<Vec<usize>>,
}

impl ExactSearch<'_> {
    fn orbit(&self, vertex: usize, marks: &[Option<usize>]) -> HashSet<usize> {
        let generators: Vec<_> = self
            .automorphisms
            .iter()
            .filter(|permutation| {
                marks
                    .iter()
                    .enumerate()
                    .all(|(v, mark)| mark.is_none() || permutation[v] == v)
            })
            .collect();
        let mut orbit = HashSet::from([vertex]);
        let mut todo = vec![vertex];
        while let Some(v) = todo.pop() {
            for permutation in &generators {
                let image = permutation[v];
                if orbit.insert(image) {
                    todo.push(image);
                }
            }
        }
        orbit
    }

    fn visit(
        &mut self,
        marks: &mut [Option<usize>],
        depth: usize,
        diagnostics: &mut ProjectionDiagnostics,
    ) -> EncodingResult<(Vec<u8>, Vec<usize>)> {
        diagnostics.exact_search_nodes += 1;
        let colors = refine(self.graph, self.adjacency, marks, diagnostics);
        let mut cells = BTreeMap::<usize, Vec<usize>>::new();
        for (v, color) in colors.iter().copied().enumerate() {
            cells.entry(color).or_default().push(v);
        }
        let tied = cells
            .iter()
            .filter(|(_, cell)| cell.len() > 1)
            .min_by_key(|(color, cell)| (cell.len(), **color));
        let Some((_, cell)) = tied else {
            let mut order: Vec<_> = (0..colors.len()).collect();
            order.sort_by_key(|&v| colors[v]);
            let code = complete_encoding(self.graph, &order)?;
            diagnostics.candidate_encodings += 1;
            if let Some(previous) = self.representatives.get(&code) {
                let mut permutation: Vec<_> = (0..colors.len()).collect();
                for (&from, &to) in previous.iter().zip(&order) {
                    permutation[from] = to;
                }
                let identity: Vec<_> = (0..colors.len()).collect();
                // Full typed encoding verifies the complete permutation. A
                // refinement color or a hash match is never this proof.
                if complete_encoding(self.graph, &permutation)?
                    != complete_encoding(self.graph, &identity)?
                {
                    return Err(TerminationReason::StateEncoding);
                }
                if permutation != identity {
                    self.automorphisms.push(permutation);
                    diagnostics.verified_automorphisms += 1;
                }
            } else {
                self.representatives.insert(code.clone(), order.clone());
            }
            return Ok((code, order));
        };
        let candidates = cell.clone();
        let mut searched = HashSet::new();
        let mut best: Option<(Vec<u8>, Vec<usize>)> = None;
        for vertex in candidates {
            if !self.orbit(vertex, marks).is_disjoint(&searched) {
                diagnostics.pruned_branches += 1;
                continue;
            }
            marks[vertex] = Some(depth);
            let candidate = self.visit(marks, depth + 1, diagnostics)?;
            marks[vertex] = None;
            if best.as_ref().is_none_or(|(code, _)| candidate.0 < *code) {
                best = Some(candidate);
            }
            searched.insert(vertex);
        }
        best.ok_or(TerminationReason::StateEncoding)
    }
}

pub fn normalize_graph(graph: &PublicGraph) -> EncodingResult<GraphNormalization> {
    let start = std::time::Instant::now();
    let mut normalized = normalize_components(graph)?;
    let encoding = encode(&(
        PROJECTION_SCHEMA,
        normalized
            .components
            .iter()
            .map(|c| &c.encoding)
            .collect::<Vec<_>>(),
    ))?;
    normalized.diagnostics.full_aggregate_encodings += 1;
    normalized.diagnostics.total_nanos = start.elapsed().as_nanos();
    Ok(GraphNormalization {
        encoding,
        components: normalized.components,
        diagnostics: normalized.diagnostics,
    })
}

fn normalize_components(graph: &PublicGraph) -> EncodingResult<ComponentNormalization> {
    let start = std::time::Instant::now();
    if graph.schema != PROJECTION_SCHEMA {
        return Err(TerminationReason::StateEncoding);
    }
    let adjacency = Adjacency::new(graph)?;
    let mut diagnostics = ProjectionDiagnostics {
        vertices: graph.vertices.len(),
        ..Default::default()
    };
    for edge in &graph.edges {
        *diagnostics.edges_by_type.entry(edge.kind).or_default() += 1;
    }
    let extraction = std::time::Instant::now();
    let mut seen = HashSet::new();
    let mut members = Vec::new();
    for v in 0..graph.vertices.len() {
        if !seen.insert(v) {
            continue;
        }
        let mut component = vec![v];
        let mut cursor = 0;
        while cursor < component.len() {
            for &neighbor in &adjacency.support[component[cursor]] {
                if seen.insert(neighbor) {
                    component.push(neighbor);
                }
            }
            cursor += 1;
        }
        members.push(component);
    }
    diagnostics.component_nanos = extraction.elapsed().as_nanos();
    let mut components = Vec::new();
    // The complete inventory and validated adjacency establish isolation first.
    // This cache stores only exact descriptor bytes within this invocation;
    // each concrete vertex still gets its own component and coordinate.
    let mut isolated_encodings = BTreeMap::<Vertex, Vec<u8>>::new();
    for member in members {
        if member.len() == 1 {
            let v = member[0];
            let vertex = &graph.vertices[v];
            if vertex.kind == VertexKind::CurrentObject
                && adjacency.incoming[v].is_empty()
                && adjacency.outgoing[v].is_empty()
            {
                let timed = std::time::Instant::now();
                diagnostics.isolated_current_components += 1;
                let code = if let Some(code) = isolated_encodings.get(vertex) {
                    diagnostics.isolated_encoding_reuses += 1;
                    code.clone()
                } else {
                    // An edgeless singleton has exactly one local order.
                    // Use the same complete encoding as a general tree leaf.
                    let local = PublicGraph {
                        vertices: vec![vertex.clone()],
                        ..PublicGraph::default()
                    };
                    let code = complete_encoding(&local, &[0])?;
                    diagnostics.candidate_encodings += 1;
                    isolated_encodings.insert(vertex.clone(), code.clone());
                    code
                };
                diagnostics.tree_nanos += timed.elapsed().as_nanos();
                components.push(ComponentMetadata {
                    vertices: member,
                    encoding: code,
                    is_tree: true,
                    relational: false,
                });
                continue;
            }
        }
        let inverse: HashMap<_, _> = member
            .iter()
            .enumerate()
            .map(|(local, &global)| (global, local))
            .collect();
        let mut local = PublicGraph::default();
        local.vertices = member.iter().map(|&v| graph.vertices[v].clone()).collect();
        for &v in &member {
            for (kind, payload, target) in &adjacency.outgoing[v] {
                local.edges.push(Edge {
                    source: inverse[&v],
                    target: *inverse
                        .get(target)
                        .ok_or(TerminationReason::StateEncoding)?,
                    kind: *kind,
                    payload: payload.clone(),
                });
            }
        }
        let local_adjacency = Adjacency::new(&local)?;
        let colors = refine(
            &local,
            &local_adjacency,
            &vec![None; member.len()],
            &mut diagnostics,
        );
        let mut counts = BTreeMap::new();
        for color in colors {
            *counts.entry(color).or_insert(0usize) += 1;
        }
        diagnostics
            .unresolved_cell_sizes
            .extend(counts.into_values().filter(|&n| n > 1));
        let timed = std::time::Instant::now();
        let (code, order, is_tree) = if let Some(order) = tree_order(&local, &local_adjacency)? {
            diagnostics.general_tree_components += 1;
            let code = complete_encoding(&local, &order)?;
            diagnostics.candidate_encodings += 1;
            diagnostics.tree_nanos += timed.elapsed().as_nanos();
            (code, order, true)
        } else {
            let mut search = ExactSearch {
                graph: &local,
                adjacency: &local_adjacency,
                representatives: BTreeMap::new(),
                automorphisms: Vec::new(),
            };
            let (code, order) = search.visit(&mut vec![None; member.len()], 0, &mut diagnostics)?;
            diagnostics.cyclic_nanos += timed.elapsed().as_nanos();
            (code, order, false)
        };
        components.push(ComponentMetadata {
            vertices: order.iter().map(|&v| member[v]).collect(),
            encoding: code,
            is_tree,
            relational: !local.edges.is_empty()
                || local
                    .vertices
                    .iter()
                    .any(|v| v.kind != VertexKind::CurrentObject),
        });
    }
    components.sort_by(|a, b| a.encoding.cmp(&b.encoding));
    diagnostics.unresolved_cell_sizes.sort_unstable();
    diagnostics.total_nanos = start.elapsed().as_nanos();
    Ok(ComponentNormalization {
        components,
        diagnostics,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum CoordinateNamespace {
    Relational,
    ActionOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SemanticCoordinate {
    pub namespace: CoordinateNamespace,
    pub ordinal: usize,
}

#[derive(Debug, Clone)]
pub struct JointPublicNormalization<'a> {
    data: JointPublicData,
    immutable_decision: PhantomData<&'a GameState>,
}

#[derive(Debug, Clone)]
pub struct JointPublicData {
    pub encoding: Vec<u8>,
    pub current_ids: HashSet<ObjectId>,
    pub exact_to_coordinate: HashMap<ExactObjectRef, SemanticCoordinate>,
    pub coordinate_to_exact: BTreeMap<SemanticCoordinate, ExactObjectRef>,
    pub occurrence_coordinates: Vec<SemanticCoordinate>,
    pub resolving_occurrence: Option<crate::rules::transitions::ZoneOccurrenceInfo>,
    pub components: Vec<ComponentMetadata>,
    pub diagnostics: ProjectionDiagnostics,
    pub retained: crate::rules::transitions::RetainedNormalization,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
struct StableIdentityFacts {
    definition: Option<u64>,
    owner: Option<usize>,
    token: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum LegacyHandle {
    Object(ObjectId),
    Exact(ExactObjectRef),
    Stack(u64),
}

struct Inventory<'v, 'a> {
    view: &'v PlayerView<'a>,
    graph: PublicGraph,
    identities: HashMap<ExactObjectRef, usize>,
    stable: HashMap<ExactObjectRef, StableIdentityFacts>,
    current: HashMap<ObjectId, usize>,
    legacy: HashMap<LegacyHandle, usize>,
    frames: HashMap<(u64, ExactObjectRef), (usize, Vec<u8>)>,
    frame_links: HashMap<(u64, ExactObjectRef), Vec<Vec<u8>>>,
    groups: HashMap<u64, usize>,
    historical_sources: HashMap<(u64, ExactObjectRef), crate::card::AttachmentEvidence>,
    historical_links: HashSet<(
        u64,
        ExactObjectRef,
        ExactObjectRef,
        crate::card::AttachmentKind,
    )>,
    cast_records: HashMap<u64, (usize, Vec<u8>)>,
    pending_vertices: Vec<usize>,
    stack_vertices: Vec<usize>,
    resolving_entry_vertex: Option<usize>,
    stack_occurrence_vertices: Vec<Option<usize>>,
    resolving_occurrence_vertex: Option<usize>,
}

fn characteristics_facts(
    chars: &crate::layers::ComputedCharacteristics,
) -> EncodingResult<Vec<u8>> {
    let mut types = chars.card_types.clone();
    types.sort_by_key(|v| *v as u8);
    let mut subtypes = chars.subtypes.clone();
    subtypes.sort_by(|a, b| a.0.cmp(&b.0));
    let mut colors = chars.colors.clone();
    colors.sort_by_key(|v| *v as u8);
    let mut keywords = chars.keywords.clone();
    keywords.sort_by_key(|v| *v as u8);
    encode(&(
        types,
        subtypes,
        colors,
        keywords,
        chars.power,
        chars.toughness,
        chars.controller,
        chars.abilities_removed,
    ))
}

impl<'v, 'a> Inventory<'v, 'a> {
    // Preserve every pre-shortcut structural failure without canonical labeling.
    // The closed byte-valued graph is the sole input to the unchanged finisher.
    fn validate_complete(&self) -> EncodingResult<()> {
        let fail = TerminationReason::StateEncoding;
        let n = self.graph.vertices.len();
        if self.graph.schema != PROJECTION_SCHEMA {
            return Err(fail);
        }
        let mut occurrence_edges = HashMap::<usize, [Option<usize>; 3]>::new();
        let mut stack_sources = HashSet::new();
        for edge in &self.graph.edges {
            if edge.source >= n || edge.target >= n {
                return Err(fail);
            }
            if edge.kind == EdgeKind::StackSource && edge.payload.is_empty() {
                stack_sources.insert((edge.source, edge.target));
            }
            if self.graph.vertices[edge.source].kind == VertexKind::Occurrence
                && edge.payload.is_empty()
            {
                let slot = match edge.kind {
                    EdgeKind::OccurrenceSource => 0,
                    EdgeKind::OccurrenceSubject => 1,
                    EdgeKind::OccurrenceAfter => 2,
                    _ => continue,
                };
                if occurrence_edges.entry(edge.source).or_default()[slot]
                    .replace(edge.target).is_some()
                {
                    return Err(fail);
                }
            }
        }
        let mut identities = HashSet::with_capacity(self.identities.len());
        for &vertex in self.identities.values() {
            if vertex >= n || !identities.insert(vertex) {
                return Err(fail);
            }
        }
        if self.pending_vertices.len() != self.view.pending_triggers.len()
            || self.stack_vertices.len() != self.view.stack.len()
            || self.stack_occurrence_vertices.len() != self.view.stack.len()
        {
            return Err(fail);
        }
        let mut seen = HashSet::new();
        let mut check = |vertex: usize, source_id: ObjectId, context: &TriggerContext| {
            if self.graph.vertices.get(vertex).map(|v| v.kind) != Some(VertexKind::Occurrence)
                || !seen.insert(vertex)
            {
                return Err(fail);
            }
            let expected = if let Some(zone) = &context.zone_transition {
                if zone.source_before.object.id != source_id
                    || !self.groups.contains_key(&zone.group_id)
                {
                    return Err(fail);
                }
                [
                    Some(self.frames.get(&(zone.group_id, zone.source_before.object)).ok_or(fail)?.0),
                    Some(self.frames.get(&(zone.group_id, zone.subject.before.object)).ok_or(fail)?.0),
                    Some(*self.identities.get(&zone.subject.after).ok_or(fail)?),
                ]
            } else {
                [Some(*self.identities.get(&ExactObjectRef {
                    id: source_id,
                    generation: context.source_generation,
                }).ok_or(fail)?), None, None]
            };
            if occurrence_edges.get(&vertex) != Some(&expected) {
                return Err(fail);
            }
            Ok(())
        };
        for (trigger, &vertex) in self.view.pending_triggers.iter().zip(&self.pending_vertices) {
            check(vertex, trigger.source_id, &trigger.context)?;
        }
        let mut stack_records = HashSet::new();
        for (slot, entry) in self.view.stack.iter().enumerate() {
            let record = self.stack_vertices[slot];
            if self.graph.vertices.get(record).map(|v| v.kind) != Some(VertexKind::StackOrOperation)
                || !stack_records.insert(record)
            {
                return Err(fail);
            }
            match (&entry.source, self.stack_occurrence_vertices[slot]) {
                (StackSource::TriggeredAbility { source_id, context, .. }, Some(vertex)) => {
                    check(vertex, *source_id, context)?;
                    if !stack_sources.contains(&(record, vertex)) {
                        return Err(fail);
                    }
                }
                (StackSource::TriggeredAbility { .. }, None) | (_, Some(_)) => return Err(fail),
                (_, None) => {}
            }
        }
        let resolving = self.view.pending_copy_order.and_then(|op| op.resolving_entry());
        if resolving.is_some() != self.resolving_entry_vertex.is_some() {
            return Err(fail);
        }
        if let Some(record) = self.resolving_entry_vertex {
            if self.graph.vertices.get(record).map(|v| v.kind) != Some(VertexKind::StackOrOperation)
                || stack_records.contains(&record)
            {
                return Err(fail);
            }
        }
        match (resolving.map(|entry| &entry.source), self.resolving_occurrence_vertex) {
            (Some(StackSource::TriggeredAbility { source_id, context, .. }), Some(vertex)) => {
                check(vertex, *source_id, context)?;
                let record = self.resolving_entry_vertex.ok_or(fail)?;
                if !stack_sources.contains(&(record, vertex)) {
                    return Err(fail);
                }
            }
            (Some(StackSource::TriggeredAbility { .. }), None) | (_, Some(_)) => return Err(fail),
            (_, None) => {}
        }
        if seen.len() != self.graph.vertices.iter().filter(|v| v.kind == VertexKind::Occurrence).count() {
            return Err(fail);
        }
        Ok(())
    }

    fn new(view: &'v PlayerView<'a>) -> Self {
        let mut groups: Vec<_> = view
            .pending_triggers
            .iter()
            .map(|t| &t.context)
            .chain(view.stack.iter().filter_map(|entry| match &entry.source {
                StackSource::TriggeredAbility { context, .. } => Some(context.as_ref()),
                _ => None,
            }))
            .chain(
                view.pending_copy_order
                    .and_then(|operation| operation.resolving_entry())
                    .and_then(|entry| match &entry.source {
                        StackSource::TriggeredAbility { context, .. } => Some(context.as_ref()),
                        _ => None,
                    }),
            )
            .filter_map(|context| context.zone_transition.as_ref().map(|zone| zone.group_id))
            .collect();
        groups.sort_unstable();
        groups.dedup();
        Self {
            view,
            graph: PublicGraph::default(),
            identities: HashMap::new(),
            stable: HashMap::new(),
            current: HashMap::new(),
            legacy: HashMap::new(),
            frames: HashMap::new(),
            frame_links: HashMap::new(),
            groups: groups
                .into_iter()
                .enumerate()
                .map(|(rank, group)| (group, rank))
                .collect(),
            historical_sources: HashMap::new(),
            historical_links: HashSet::new(),
            cast_records: HashMap::new(),
            pending_vertices: Vec::new(),
            stack_vertices: Vec::new(),
            resolving_entry_vertex: None,
            stack_occurrence_vertices: Vec::new(),
            resolving_occurrence_vertex: None,
        }
    }

    fn current_objects(&mut self) -> EncodingResult<()> {
        let mut locations = HashMap::new();
        for &(id, zone, seat) in &self.view.visible_locations {
            if locations.insert(id, (zone, seat)).is_some() {
                return Err(TerminationReason::StateEncoding);
            }
        }
        if locations.len() != self.view.objects.len() {
            return Err(TerminationReason::StateEncoding);
        }
        for &(id, _, _) in &self.view.visible_locations {
            let inst = self
                .view
                .objects
                .get(&id)
                .ok_or(TerminationReason::StateEncoding)?;
            if inst.object_id != id || self.view.card_db.get(inst.card_def_id).is_none() {
                return Err(TerminationReason::StateEncoding);
            }
            let location = locations.get(&id).ok_or(TerminationReason::StateEncoding)?;
            let chars = self
                .view
                .represented_characteristics
                .get(&id)
                .ok_or(TerminationReason::StateEncoding)?;
            let mut temporary = inst.temp_keywords.clone();
            temporary.sort_by_key(|v| *v as u8);
            let facts = (
                location,
                inst.card_def_id,
                inst.owner,
                inst.controller,
                characteristics_facts(chars)?,
                (
                    inst.plus_counters,
                    inst.minus_counters,
                    inst.damage_marked,
                    inst.tapped,
                    inst.summoning_sick,
                ),
                (
                    inst.temp_power_mod,
                    inst.temp_toughness_mod,
                    temporary,
                    inst.loyalty_counters,
                    inst.loyalty_activated_this_turn,
                ),
                inst.is_token,
                (
                    self.view.commander_objects.contains(&id),
                    self.view.commander_roles.get(&id),
                ),
                self.view.combat.attackers.contains(&id),
            );
            let vertex = self.graph.vertex(VertexKind::CurrentObject, &facts)?;
            let exact = ExactObjectRef {
                id,
                generation: inst.zone_change_count,
            };
            self.identities.insert(exact, vertex);
            self.current.insert(id, vertex);
            self.stable.insert(
                exact,
                StableIdentityFacts {
                    definition: Some(inst.card_def_id),
                    owner: Some(inst.owner),
                    token: Some(inst.is_token),
                },
            );
        }
        for &(id, _, _) in &self.view.visible_locations {
            let inst = self
                .view
                .objects
                .get(&id)
                .ok_or(TerminationReason::StateEncoding)?;
            if let Some(link) = inst.attachment_link() {
                let definition = self
                    .view
                    .card_db
                    .get(inst.card_def_id)
                    .ok_or(TerminationReason::StateEncoding)?;
                if link.target.id == id
                    || match link.kind {
                        crate::card::AttachmentKind::Equipment => !definition.is_equipment(),
                        crate::card::AttachmentKind::Aura => !definition.is_aura(),
                    }
                {
                    return Err(TerminationReason::StateEncoding);
                }
                let source = ExactObjectRef {
                    id,
                    generation: inst.zone_change_count,
                };
                if link.source_generation != source.generation
                    || !self.view.battlefield.contains(&id)
                    || !self.view.battlefield.contains(&link.target.id)
                    || self.view.visible_current_generation(link.target.id)
                        != Some(link.target.generation)
                {
                    return Err(TerminationReason::StateEncoding);
                }
                let source = *self
                    .identities
                    .get(&source)
                    .ok_or(TerminationReason::StateEncoding)?;
                let target = *self
                    .identities
                    .get(&link.target)
                    .ok_or(TerminationReason::StateEncoding)?;
                self.graph
                    .edge(source, target, EdgeKind::LiveAttached, &link.kind)?;
            }
            if locations[&id].0 == ZoneType::Exile {
                if let Some(source) = inst.exiled_by {
                    let source = self.raw(source)?;
                    self.graph
                        .edge(self.current[&id], source, EdgeKind::LinkedExile, &())?;
                }
            }
        }
        Ok(())
    }

    fn raw(&mut self, id: ObjectId) -> EncodingResult<usize> {
        if let Some(&vertex) = self.current.get(&id) {
            return Ok(vertex);
        }
        self.opaque(LegacyHandle::Object(id), 0)
    }

    fn opaque(&mut self, handle: LegacyHandle, tag: u8) -> EncodingResult<usize> {
        if let Some(&vertex) = self.legacy.get(&handle) {
            return Ok(vertex);
        }
        let vertex = self.graph.vertex(VertexKind::LegacyReference, &tag)?;
        self.legacy.insert(handle, vertex);
        Ok(vertex)
    }

    fn exact(
        &mut self,
        exact: ExactObjectRef,
        facts: StableIdentityFacts,
    ) -> EncodingResult<usize> {
        if self
            .view
            .visible_current_generation(exact.id)
            .is_some_and(|current| current < exact.generation)
        {
            return Err(TerminationReason::StateEncoding);
        }
        let previous = self.stable.entry(exact).or_default();
        fn merge<T: Copy + Eq>(old: &mut Option<T>, new: Option<T>) -> EncodingResult<()> {
            if old.is_some() && new.is_some() && *old != new {
                return Err(TerminationReason::StateEncoding);
            }
            *old = old.or(new);
            Ok(())
        }
        merge(&mut previous.definition, facts.definition)?;
        merge(&mut previous.owner, facts.owner)?;
        merge(&mut previous.token, facts.token)?;
        if let Some(&vertex) = self.identities.get(&exact) {
            if self.graph.vertices[vertex].kind == VertexKind::HistoricalIdentity {
                self.graph.vertices[vertex].facts = encode(previous)?;
            }
            return Ok(vertex);
        }
        let vertex = if let Some(&vertex) = self.legacy.get(&LegacyHandle::Exact(exact)) {
            self.graph.vertices[vertex] = Vertex {
                kind: VertexKind::HistoricalIdentity,
                facts: encode(previous)?,
            };
            vertex
        } else {
            self.graph
                .vertex(VertexKind::HistoricalIdentity, previous)?
        };
        self.identities.insert(exact, vertex);
        Ok(vertex)
    }

    fn legacy_exact(&mut self, exact: ExactObjectRef) -> EncodingResult<usize> {
        if self
            .view
            .visible_current_generation(exact.id)
            .is_some_and(|current| current < exact.generation)
        {
            return Err(TerminationReason::StateEncoding);
        }
        if let Some(&vertex) = self.identities.get(&exact) {
            return Ok(vertex);
        }
        self.opaque(LegacyHandle::Exact(exact), 1)
    }

    fn frame(
        &mut self,
        group: u64,
        object: &crate::rules::transitions::LastKnownObject,
    ) -> EncodingResult<usize> {
        if object.attachment.malformed_link.is_some() {
            return Err(TerminationReason::StateEncoding);
        }
        // IDs here only establish equality of repeated owned frame inventories.
        // They never enter descriptors, labels, or canonical preference.
        let mut links: Vec<Vec<u8>> = object
            .attachment
            .links
            .iter()
            .map(encode)
            .collect::<EncodingResult<_>>()?;
        links.sort();
        if links.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(TerminationReason::StateEncoding);
        }
        if let Some(previous) = self.frame_links.get(&(group, object.object)) {
            if *previous != links {
                return Err(TerminationReason::StateEncoding);
            }
        } else {
            self.frame_links.insert((group, object.object), links);
        }
        let identity = self.exact(
            object.object,
            StableIdentityFacts {
                definition: Some(object.card_id),
                owner: Some(object.owner),
                token: Some(object.is_token),
            },
        )?;
        let rank = *self
            .groups
            .get(&group)
            .ok_or(TerminationReason::StateEncoding)?;
        let mut types = object.card_types.clone();
        types.sort_by_key(|v| *v as u8);
        let mut subtypes = object.subtypes.clone();
        subtypes.sort_by(|a, b| a.0.cmp(&b.0));
        let mut colors = object.colors.clone();
        colors.sort_by_key(|v| *v as u8);
        let mut keywords = object.keywords.clone();
        keywords.sort_by_key(|v| *v as u8);
        let facts = encode(&(
            0u8,
            rank,
            (
                object.card_id,
                object.owner,
                object.controller,
                object.location,
                object.is_token,
            ),
            (
                types,
                subtypes,
                colors,
                keywords,
                object.power,
                object.toughness,
            ),
            (object.plus_counters, object.minus_counters, object.tapped),
        ))?;
        if let Some((vertex, previous)) = self.frames.get(&(group, object.object)) {
            if *previous != facts {
                return Err(TerminationReason::StateEncoding);
            }
            return Ok(*vertex);
        }
        let vertex = self.graph.vertex(VertexKind::HistoricalFrame, &facts)?;
        self.graph.edge(vertex, identity, EdgeKind::FrameOf, &())?;
        self.frames.insert((group, object.object), (vertex, facts));
        Ok(vertex)
    }

    fn history(&mut self) -> EncodingResult<()> {
        let contexts: Vec<_> = self
            .view
            .pending_triggers
            .iter()
            .map(|t| &t.context)
            .chain(
                self.view
                    .stack
                    .iter()
                    .filter_map(|entry| match &entry.source {
                        StackSource::TriggeredAbility { context, .. } => Some(context.as_ref()),
                        _ => None,
                    }),
            )
            .chain(
                self.view
                    .pending_copy_order
                    .and_then(|operation| operation.resolving_entry())
                    .and_then(|entry| match &entry.source {
                        StackSource::TriggeredAbility { context, .. } => Some(context.as_ref()),
                        _ => None,
                    }),
            )
            .collect();
        // Full frozen frames first. Attachment evidence never recovers a frame
        // from a current raw-ID lookup, even when that current object is public.
        for context in &contexts {
            if let Some(zone) = &context.zone_transition {
                if zone.source_before.card_id != context.source_card_id
                    || zone.source_before.object.generation != context.source_generation
                    || zone.subject.after.id != zone.subject.before.object.id
                    || zone.subject.before.object.generation.checked_add(1)
                        != Some(zone.subject.after.generation)
                    || zone.source_was_subject
                        != (zone.source_before.object == zone.subject.before.object)
                {
                    return Err(TerminationReason::StateEncoding);
                }
                self.frame(zone.group_id, &zone.source_before)?;
                self.frame(zone.group_id, &zone.subject.before)?;
                self.exact(
                    zone.subject.after,
                    StableIdentityFacts {
                        definition: Some(zone.subject.before.card_id),
                        owner: Some(zone.subject.before.owner),
                        token: Some(zone.subject.before.is_token),
                    },
                )?;
            }
        }
        let mut inventoried_frames = HashSet::new();
        for context in contexts {
            let Some(zone) = &context.zone_transition else {
                continue;
            };
            for object in [&zone.source_before, &zone.subject.before] {
                if !inventoried_frames.insert((zone.group_id, object.object)) {
                    continue;
                }
                for link in &object.attachment.links {
                    if link.source == link.target
                        || (link.source != object.object && link.target != object.object)
                    {
                        return Err(TerminationReason::StateEncoding);
                    }
                    let actual = (
                        object.card_id,
                        object.owner,
                        object.controller,
                        object.is_token,
                    );
                    let expected = if link.source == object.object {
                        (
                            link.source_card_id,
                            link.source_owner,
                            link.source_controller,
                            link.source_is_token,
                        )
                    } else {
                        (
                            link.target_card_id,
                            link.target_owner,
                            link.target_controller,
                            link.target_is_token,
                        )
                    };
                    if actual != expected {
                        return Err(TerminationReason::StateEncoding);
                    }
                    if let Some(previous) = self
                        .historical_sources
                        .insert((zone.group_id, link.source), link.clone())
                    {
                        if previous != *link {
                            return Err(TerminationReason::StateEncoding);
                        }
                    }
                    self.exact(
                        link.source,
                        StableIdentityFacts {
                            definition: Some(link.source_card_id),
                            owner: Some(link.source_owner),
                            token: Some(link.source_is_token),
                        },
                    )?;
                    let target = self.exact(
                        link.target,
                        StableIdentityFacts {
                            definition: Some(link.target_card_id),
                            owner: Some(link.target_owner),
                            token: Some(link.target_is_token),
                        },
                    )?;
                    let source_frame =
                        if let Some((frame, _)) = self.frames.get(&(zone.group_id, link.source)) {
                            *frame
                        } else {
                            let rank = *self
                                .groups
                                .get(&zone.group_id)
                                .ok_or(TerminationReason::StateEncoding)?;
                            let identity = *self
                                .identities
                                .get(&link.source)
                                .ok_or(TerminationReason::StateEncoding)?;
                            let facts = encode(&(
                                1u8,
                                rank,
                                link.source_card_id,
                                link.source_owner,
                                link.source_controller,
                                link.source_is_token,
                            ))?;
                            let frame = self.graph.vertex(VertexKind::HistoricalFrame, &facts)?;
                            self.graph.edge(frame, identity, EdgeKind::FrameOf, &())?;
                            self.frames
                                .insert((zone.group_id, link.source), (frame, facts));
                            frame
                        };
                    if self.historical_links.insert((
                        zone.group_id,
                        link.source,
                        link.target,
                        link.kind,
                    )) {
                        self.graph.edge(
                            source_frame,
                            target,
                            EdgeKind::HistoricalAttached,
                            &(
                                link.kind,
                                (
                                    link.source_card_id,
                                    link.source_owner,
                                    link.source_controller,
                                    link.source_is_token,
                                ),
                                (
                                    link.target_card_id,
                                    link.target_owner,
                                    link.target_controller,
                                    link.target_is_token,
                                ),
                            ),
                        )?;
                    }
                }
            }
        }
        Ok(())
    }

    fn occurrence(
        &mut self,
        source_id: ObjectId,
        ability: usize,
        controller: usize,
        context: &TriggerContext,
        stack_position: Option<usize>,
    ) -> EncodingResult<usize> {
        let group = context
            .zone_transition
            .as_ref()
            .and_then(|zone| self.groups.get(&zone.group_id))
            .copied();
        let transition = context.zone_transition.as_ref().map(|zone| {
            (
                &zone.subject.kind,
                &zone.subject.destination,
                &zone.subject.sba_causes,
            )
        });
        let vertex = self.graph.vertex(
            VertexKind::Occurrence,
            &(
                controller,
                ability,
                context.source_card_id,
                encode(&context.effect)?,
                group,
                transition,
                stack_position,
            ),
        )?;
        if let Some(zone) = &context.zone_transition {
            if zone.source_before.object.id != source_id {
                return Err(TerminationReason::StateEncoding);
            }
            let source = self
                .frames
                .get(&(zone.group_id, zone.source_before.object))
                .ok_or(TerminationReason::StateEncoding)?
                .0;
            let subject = self
                .frames
                .get(&(zone.group_id, zone.subject.before.object))
                .ok_or(TerminationReason::StateEncoding)?
                .0;
            let after = *self
                .identities
                .get(&zone.subject.after)
                .ok_or(TerminationReason::StateEncoding)?;
            self.graph
                .edge(vertex, source, EdgeKind::OccurrenceSource, &())?;
            self.graph
                .edge(vertex, subject, EdgeKind::OccurrenceSubject, &())?;
            self.graph
                .edge(vertex, after, EdgeKind::OccurrenceAfter, &())?;
        } else {
            let source = self.exact(
                ExactObjectRef {
                    id: source_id,
                    generation: context.source_generation,
                },
                StableIdentityFacts {
                    definition: Some(context.source_card_id),
                    ..Default::default()
                },
            )?;
            self.graph
                .edge(vertex, source, EdgeKind::OccurrenceSource, &())?;
        }
        if let Some(spell) = context.cast_spell.as_ref() {
            let facts = encode(&(
                5u8,
                &spell.definition,
                spell.controller,
                &spell.historical_object_targets,
            ))?;
            let snapshot = if let Some((record, previous)) = self.cast_records.get(&spell.stack_id)
            {
                if *previous != facts {
                    return Err(TerminationReason::StateEncoding);
                }
                *record
            } else {
                let record = self.graph.vertex(VertexKind::StackOrOperation, &facts)?;
                self.cast_records.insert(spell.stack_id, (record, facts));
                for (position, target) in spell.targets.iter().enumerate() {
                    let endpoint = match target {
                        Target::Object(id) => {
                            let historical = spell
                                .historical_object_targets
                                .get(position)
                                .copied()
                                .flatten();
                            if let Some(generation) =
                                spell.target_generations.get(position).copied().flatten()
                            {
                                self.exact(
                                    ExactObjectRef {
                                        id: *id,
                                        generation,
                                    },
                                    StableIdentityFacts {
                                        definition: historical.map(|(id, _)| id),
                                        ..Default::default()
                                    },
                                )?
                            } else {
                                self.opaque(LegacyHandle::Object(*id), 0)?
                            }
                        }
                        Target::StackEntry(id) => self.opaque(LegacyHandle::Stack(*id), 2)?,
                        Target::Player(player) => {
                            self.graph.edge(
                                record,
                                record,
                                EdgeKind::StackTarget,
                                &(position, 0u8, player),
                            )?;
                            continue;
                        }
                    };
                    self.graph
                        .edge(record, endpoint, EdgeKind::StackTarget, &(position, 1u8))?;
                }
                record
            };
            self.graph
                .edge(vertex, snapshot, EdgeKind::OccurrenceSubject, &1u8)?;
        }
        Ok(vertex)
    }

    fn effects(&mut self) -> EncodingResult<()> {
        use crate::layers::AffectedObjects;
        let mut timestamps: Vec<_> = self
            .view
            .continuous_effects
            .iter()
            .map(|e| e.timestamp)
            .collect();
        timestamps.sort_unstable();
        timestamps.dedup();
        for (index, effect) in self.view.continuous_effects.iter().enumerate() {
            // Explicit identity selectors are edges. Remaining currently
            // represented selectors carry only their typed scalar parameters.
            let selector = match &effect.affected {
                AffectedObjects::Specific(_) => AffectedObjects::Specific(0),
                AffectedObjects::SpecificIncarnation { .. } => {
                    AffectedObjects::SpecificIncarnation {
                        object_id: 0,
                        zone_change_count: 0,
                    }
                }
                AffectedObjects::Source => AffectedObjects::Source,
                AffectedObjects::AllCreatures => AffectedObjects::AllCreatures,
                AffectedObjects::CreaturesControlledBy(p) => {
                    AffectedObjects::CreaturesControlledBy(*p)
                }
                AffectedObjects::OtherCreatures => AffectedObjects::OtherCreatures,
                AffectedObjects::OtherCreaturesControlledBy(p) => {
                    AffectedObjects::OtherCreaturesControlledBy(*p)
                }
                AffectedObjects::OtherNonHumanCreaturesControlledBy(p) => {
                    AffectedObjects::OtherNonHumanCreaturesControlledBy(*p)
                }
                AffectedObjects::AllPermanents => AffectedObjects::AllPermanents,
                AffectedObjects::PermanentsControlledBy(p) => {
                    AffectedObjects::PermanentsControlledBy(*p)
                }
                AffectedObjects::OtherCreaturesWithSubtypeControlledBy(t, p) => {
                    AffectedObjects::OtherCreaturesWithSubtypeControlledBy(t.clone(), *p)
                }
                AffectedObjects::AttachedTo => AffectedObjects::AttachedTo,
                AffectedObjects::OtherCreaturesWithSubtypeControlledBySource(t) => {
                    AffectedObjects::OtherCreaturesWithSubtypeControlledBySource(t.clone())
                }
            };
            let rank = timestamps
                .binary_search(&effect.timestamp)
                .map_err(|_| TerminationReason::StateEncoding)?;
            // Equal timestamps use the existing stable layer order. Retain its
            // precedence where represented operators do not commute. Additive
            // records (including the composed-effect regression) acquire no
            // artificial source labels from inventory order.
            let competing = self.view.continuous_effects.iter().any(|other| {
                other.timestamp == effect.timestamp
                    && other.modification.layer() == effect.modification.layer()
                    && !commute(&effect.modification, &other.modification)
            });
            let tie_rank = competing.then(|| {
                self.view.continuous_effects[..index]
                    .iter()
                    .filter(|other| {
                        other.timestamp == effect.timestamp
                            && other.modification.layer() == effect.modification.layer()
                    })
                    .count()
            });
            let record = self.graph.vertex(
                VertexKind::ContinuousEffect,
                &(
                    selector,
                    &effect.modification,
                    &effect.duration,
                    effect.controller,
                    rank,
                    tie_rank,
                ),
            )?;
            let source = self.raw(effect.source_id)?;
            self.graph
                .edge(record, source, EdgeKind::EffectSource, &())?;
            let target = match effect.affected {
                AffectedObjects::Source => Some(source),
                AffectedObjects::Specific(id) => Some(self.raw(id)?),
                AffectedObjects::SpecificIncarnation {
                    object_id,
                    zone_change_count,
                } => Some(self.legacy_exact(ExactObjectRef {
                    id: object_id,
                    generation: zone_change_count,
                })?),
                AffectedObjects::AttachedTo => self
                    .view
                    .objects
                    .get(&effect.source_id)
                    .and_then(|inst| inst.attachment_link())
                    .and_then(|link| self.identities.get(&link.target))
                    .copied(),
                _ => None,
            };
            if let Some(target) = target {
                self.graph
                    .edge(record, target, EdgeKind::EffectTarget, &())?;
            }
        }
        for replacement in self.view.replacement_effects {
            let record = self.graph.vertex(
                VertexKind::Replacement,
                &(
                    replacement.controller,
                    &replacement.applies_to,
                    &replacement.action,
                    replacement.is_self_replacement,
                ),
            )?;
            let source = self.raw(replacement.source_id)?;
            self.graph
                .edge(record, source, EdgeKind::ReplacementSource, &())?;
        }
        Ok(())
    }

    fn targets(
        &mut self,
        record: usize,
        targets: &[Target],
        generations: &[Option<u32>],
        kind: EdgeKind,
        stack_records: &HashMap<u64, usize>,
    ) -> EncodingResult<()> {
        if !generations.is_empty() && generations.len() != targets.len() {
            return Err(TerminationReason::StateEncoding);
        }
        for (position, target) in targets.iter().enumerate() {
            let endpoint = match target {
                Target::Object(id) => Some(
                    if let Some(generation) = generations.get(position).copied().flatten() {
                        self.legacy_exact(ExactObjectRef {
                            id: *id,
                            generation,
                        })?
                    } else {
                        self.raw(*id)?
                    },
                ),
                Target::StackEntry(id) => Some(if let Some(&vertex) = stack_records.get(id) {
                    vertex
                } else {
                    self.opaque(LegacyHandle::Stack(*id), 2)?
                }),
                Target::Player(player) => {
                    self.graph
                        .edge(record, record, kind, &(position, 0u8, *player))?;
                    None
                }
            };
            if let Some(endpoint) = endpoint {
                self.graph.edge(
                    record,
                    endpoint,
                    kind,
                    &(
                        position,
                        1u8,
                        generations.get(position).copied().flatten().is_some(),
                    ),
                )?;
            }
        }
        Ok(())
    }

    fn equip_endpoints(&mut self, entry: &crate::game::StackEntry) -> EncodingResult<(usize, usize)> {
        let Some(target) = crate::targeting::equip_entry_target(entry, self.view.card_db,
            self.view.player_count, |id| self.view.objects.get(&id).copied())? else {
            return Err(TerminationReason::StateEncoding);
        };
        let StackSource::EquipAbility { source, source_card_id, target_card_id } = &entry.source else { unreachable!() };
        let source = self.exact(*source, StableIdentityFacts { definition: Some(*source_card_id), ..Default::default() })?;
        let target = self.exact(target, StableIdentityFacts { definition: Some(*target_card_id), ..Default::default() })?;
        Ok((source, target))
    }

    fn operations(&mut self) -> EncodingResult<()> {
        for trigger in self.view.pending_triggers {
            let occurrence = self.occurrence(
                trigger.source_id,
                trigger.ability_index,
                trigger.controller,
                &trigger.context,
                None,
            )?;
            self.pending_vertices.push(occurrence);
        }
        let mut records = HashMap::new();
        for (position, entry) in self.view.stack.iter().enumerate() {
            let source_facts = match &entry.source {
                StackSource::Spell(_) => encode(&(0u8,))?,
                StackSource::ActivatedAbility { ability_index, .. } => {
                    encode(&(1u8, ability_index))?
                }
                StackSource::TriggeredAbility { ability_index, .. } => {
                    encode(&(2u8, ability_index))?
                }
                StackSource::SpellCopy { definition } => encode(&(3u8, definition))?,
                StackSource::EquipAbility { source_card_id, target_card_id, .. } => encode(&(4u8, source_card_id, target_card_id))?,
            };
            let record = self.graph.vertex(
                VertexKind::StackOrOperation,
                &(0u8, position, entry.controller, source_facts),
            )?;
            self.stack_vertices.push(record);
            if records.insert(entry.id, record).is_some() {
                return Err(TerminationReason::StateEncoding);
            }
        }
        for (position, entry) in self.view.stack.iter().enumerate() {
            let record = records[&entry.id];
            let (source, occurrence) = match &entry.source {
                StackSource::Spell(id) | StackSource::ActivatedAbility { source_id: id, .. } => {
                    (Some(self.raw(*id)?), None)
                }
                StackSource::TriggeredAbility {
                    source_id,
                    ability_index,
                    context,
                } => {
                    let occurrence = self.occurrence(
                        *source_id,
                        *ability_index,
                        entry.controller,
                        context,
                        Some(position),
                    )?;
                    (Some(occurrence), Some(occurrence))
                }
                StackSource::SpellCopy { .. } => (None, None),
                StackSource::EquipAbility { .. } => {
                    let (source, target) = self.equip_endpoints(entry)?;
                    self.graph.edge(record, target, EdgeKind::StackTarget, &(0usize, 0u8, true))?;
                    (Some(source), None)
                }
            };
            self.stack_occurrence_vertices.push(occurrence);
            if let Some(source) = source {
                self.graph
                    .edge(record, source, EdgeKind::StackSource, &())?;
            }
            if !matches!(entry.source, StackSource::EquipAbility { .. }) {
                self.targets(record, &entry.targets, &entry.target_generations, EdgeKind::StackTarget, &records)?;
            }
        }
        if let Some(operation) = self.view.pending_copy_order {
            let record = self.graph.vertex(
                VertexKind::StackOrOperation,
                &(
                    1u8,
                    operation.controller(),
                    operation.selected_order(),
                    self.view.stack.len(),
                ),
            )?;
            for (position, item) in operation.items().iter().enumerate() {
                let member = self.graph.vertex(
                    VertexKind::StackOrOperation,
                    &(
                        2u8,
                        item.controller(),
                        item.definition(),
                        &item.target_descriptions,
                    ),
                )?;
                self.graph
                    .edge(record, member, EdgeKind::ContinuationMember, &position)?;
                self.targets(
                    member,
                    item.targets(),
                    item.target_generations(),
                    EdgeKind::ContinuationTarget,
                    &records,
                )?;
            }
            if let Some(entry) = operation.resolving_entry() {
                let source_facts = match &entry.source {
                    StackSource::Spell(_) => encode(&(0u8,))?,
                    StackSource::ActivatedAbility { ability_index, .. } => {
                        encode(&(1u8, ability_index))?
                    }
                    StackSource::TriggeredAbility { ability_index, .. } => {
                        encode(&(2u8, ability_index))?
                    }
                    StackSource::SpellCopy { definition } => encode(&(3u8, definition))?,
                    StackSource::EquipAbility { source_card_id, target_card_id, .. } => encode(&(4u8, source_card_id, target_card_id))?,
                };
                let member = self.graph.vertex(
                    VertexKind::StackOrOperation,
                    &(3u8, entry.controller, source_facts),
                )?;
                self.resolving_entry_vertex = Some(member);
                self.graph
                    .edge(record, member, EdgeKind::ContinuationMember, &())?;
                let source = match &entry.source {
                    StackSource::Spell(id) => Some(
                        if let Some(generation) = operation.resolving_source_generation {
                            self.legacy_exact(ExactObjectRef {
                                id: *id,
                                generation,
                            })?
                        } else {
                            self.opaque(LegacyHandle::Object(*id), 0)?
                        },
                    ),
                    StackSource::ActivatedAbility { source_id: id, .. } => Some(self.raw(*id)?),
                    StackSource::TriggeredAbility {
                        source_id,
                        ability_index,
                        context,
                    } => {
                        let occurrence = self.occurrence(
                            *source_id,
                            *ability_index,
                            entry.controller,
                            context,
                            None,
                        )?;
                        self.resolving_occurrence_vertex = Some(occurrence);
                        Some(occurrence)
                    }
                    StackSource::EquipAbility { .. } => {
                        let (source, target) = self.equip_endpoints(entry)?;
                        self.graph.edge(member, target, EdgeKind::ContinuationTarget, &(0usize, 0u8, true))?;
                        Some(source)
                    }
                    StackSource::SpellCopy { definition } => Some(
                        self.graph
                            .vertex(VertexKind::StackOrOperation, &(4u8, definition))?,
                    ),
                };
                if let Some(source) = source {
                    self.graph
                        .edge(member, source, EdgeKind::StackSource, &())?;
                }
                if !matches!(entry.source, StackSource::EquipAbility { .. }) {
                    self.targets(member, &entry.targets, &entry.target_generations, EdgeKind::ContinuationTarget, &records)?;
                }
            }
        }
        for (&id, &(snapshot, _)) in &self.cast_records {
            if let Some(&current) = records.get(&id) {
                self.graph
                    .edge(snapshot, current, EdgeKind::StackSource, &1u8)?;
            }
            if let Some(&handle) = self.legacy.get(&LegacyHandle::Stack(id)) {
                self.graph
                    .edge(snapshot, handle, EdgeKind::StackSource, &2u8)?;
            }
        }
        for (&id, &record) in &records {
            if let Some(&handle) = self.legacy.get(&LegacyHandle::Stack(id)) {
                self.graph
                    .edge(record, handle, EdgeKind::StackSource, &2u8)?;
            }
        }
        for (&blocker, &attacker) in &self.view.combat.blockers {
            let blocker = self.raw(blocker)?;
            let attacker = self.raw(attacker)?;
            self.graph.edge(blocker, attacker, EdgeKind::Blocks, &0u8)?;
        }
        for (&attacker, blockers) in &self.view.combat.attacker_blockers {
            let attacker = self.raw(attacker)?;
            for (position, &blocker) in blockers.iter().enumerate() {
                let blocker = self.raw(blocker)?;
                self.graph
                    .edge(attacker, blocker, EdgeKind::Blocks, &(1u8, position))?;
            }
        }
        for (&attacker, assignments) in &self.view.combat.damage_assignment {
            let attacker = self.raw(attacker)?;
            for (position, &(blocker, amount)) in assignments.iter().enumerate() {
                let blocker = self.raw(blocker)?;
                self.graph.edge(
                    attacker,
                    blocker,
                    EdgeKind::DamageAssignment,
                    &(position, amount),
                )?;
            }
        }
        Ok(())
    }
}

/// Complete validated inventory for one immutable view. Never persisted or
/// reused after a decision; completion consumes the inventory exactly once.
pub(crate) struct PreparedPublicNormalization<'v, 'a> {
    inventory: Inventory<'v, 'a>,
    start: std::time::Instant,
    inventory_nanos: u128,
}

impl<'v, 'a> PreparedPublicNormalization<'v, 'a> {
    pub(crate) fn from_view(view: &'v PlayerView<'a>) -> EncodingResult<Self> {
        if view.state_encoding_failed {
            return Err(TerminationReason::StateEncoding);
        }
        let start = std::time::Instant::now();
        let mut inventory = Inventory::new(view);
        inventory.current_objects()?;
        inventory.history()?;
        inventory.effects()?;
        inventory.operations()?;
        inventory.validate_complete()?;
        let inventory_nanos = start.elapsed().as_nanos();
        Ok(Self { inventory, start, inventory_nanos })
    }

    pub(crate) fn validate_state(state: &GameState, player: usize) -> EncodingResult<()> {
        if player >= state.players.len() || state.players.len() < 2 {
            return Err(TerminationReason::StateEncoding);
        }
        PreparedPublicNormalization::from_view(&state.visible_state(player)).map(drop)
    }

    pub(crate) fn finish(self) -> EncodingResult<JointPublicNormalization<'a>> {
        let Self { inventory, start, inventory_nanos } = self;
        let view = inventory.view;
        let mut canonical = normalize_components(&inventory.graph)?;
        canonical.diagnostics.inventory_nanos = inventory_nanos;
        let witness_time = std::time::Instant::now();
        let mut vertex_coordinates = HashMap::new();
        let mut relational = 0;
        let mut action_only = 0;
        for component in &canonical.components {
            for &vertex in &component.vertices {
                let coordinate = if component.relational {
                    let coordinate = SemanticCoordinate {
                        namespace: CoordinateNamespace::Relational,
                        ordinal: relational,
                    };
                    relational += 1;
                    coordinate
                } else {
                    let coordinate = SemanticCoordinate {
                        namespace: CoordinateNamespace::ActionOnly,
                        ordinal: action_only,
                    };
                    action_only += 1;
                    coordinate
                };
                vertex_coordinates.insert(vertex, coordinate);
            }
        }
        let mut exact_to_coordinate = HashMap::new();
        let mut coordinate_to_exact = BTreeMap::new();
        for (exact, vertex) in &inventory.identities {
            let coordinate = *vertex_coordinates
                .get(vertex)
                .ok_or(TerminationReason::StateEncoding)?;
            exact_to_coordinate.insert(*exact, coordinate);
            if coordinate_to_exact.insert(coordinate, *exact).is_some() {
                return Err(TerminationReason::StateEncoding);
            }
        }
        let occurrence_coordinates: Vec<_> = inventory
            .pending_vertices
            .iter()
            .map(|vertex| vertex_coordinates[vertex])
            .collect();
        canonical.diagnostics.witness_nanos = witness_time.elapsed().as_nanos();
        let supplement_time = std::time::Instant::now();
        let encoding = encode(&(
            PROJECTION_SCHEMA,
            canonical
                .components
                .iter()
                .filter(|c| c.relational)
                .map(|c| &c.encoding)
                .collect::<Vec<_>>(),
        ))?;
        canonical.diagnostics.supplement_nanos = supplement_time.elapsed().as_nanos();
        let retained_sources: HashSet<_> = view
            .pending_triggers
            .iter()
            .map(|trigger| &trigger.context)
            .chain(view.stack.iter().filter_map(|entry| match &entry.source {
                StackSource::TriggeredAbility { context, .. } => Some(context.as_ref()),
                _ => None,
            }))
            .chain(
                view.pending_copy_order
                    .and_then(|operation| operation.resolving_entry())
                    .and_then(|entry| match &entry.source {
                        StackSource::TriggeredAbility { context, .. } => Some(context.as_ref()),
                        _ => None,
                    }),
            )
            .filter_map(|context| {
                context
                    .zone_transition
                    .as_ref()
                    .map(|zone| zone.source_before.object)
            })
            .collect();
        let source_ranks: HashMap<_, _> = exact_to_coordinate
            .iter()
            .filter(|(exact, coordinate)| {
                retained_sources.contains(exact)
                    && coordinate.namespace == CoordinateNamespace::Relational
            })
            .map(|(&exact, coordinate)| (exact, coordinate.ordinal))
            .collect();
        let pending_occurrences: Vec<_> = view
            .pending_triggers
            .iter()
            .enumerate()
            .map(|(slot, trigger)| {
                occurrence_info(
                    view,
                    &trigger.context,
                    trigger.source_id,
                    trigger.ability_index,
                    trigger.controller,
                    &source_ranks,
                    &inventory.groups,
                    Some(occurrence_coordinates[slot]),
                )
            })
            .collect::<EncodingResult<_>>()?;
        let stack_occurrences: Vec<_> = view
            .stack
            .iter()
            .enumerate()
            .map(|(slot, entry)| match &entry.source {
                StackSource::TriggeredAbility {
                    source_id,
                    ability_index,
                    context,
                } if context.zone_transition.is_some() => occurrence_info(
                    view,
                    context,
                    *source_id,
                    *ability_index,
                    entry.controller,
                    &source_ranks,
                    &inventory.groups,
                    inventory.stack_occurrence_vertices[slot].map(|v| vertex_coordinates[&v]),
                ),
                _ => Ok(None),
            })
            .collect::<EncodingResult<_>>()?;
        let resolving_occurrence = match view
            .pending_copy_order
            .and_then(|operation| operation.resolving_entry())
        {
            Some(crate::game::StackEntry {
                controller,
                source:
                    StackSource::TriggeredAbility {
                        source_id,
                        ability_index,
                        context,
                    },
                ..
            }) if context.zone_transition.is_some() => occurrence_info(
                view,
                context,
                *source_id,
                *ability_index,
                *controller,
                &source_ranks,
                &inventory.groups,
                inventory
                    .resolving_occurrence_vertex
                    .map(|v| vertex_coordinates[&v]),
            )?,
            _ => None,
        };
        canonical.diagnostics.total_nanos = start.elapsed().as_nanos();
        let retained = crate::rules::transitions::RetainedNormalization {
            encoding: encoding.clone(),
            source_ranks,
            group_ranks: inventory.groups,
            stats: crate::rules::transitions::NormalizationStats {
                normalization_calls: 1,
                search_nodes: canonical.diagnostics.exact_search_nodes,
                refinement_rounds: canonical.diagnostics.refinement_rounds,
                tied_cell_sizes: canonical.diagnostics.unresolved_cell_sizes.clone(),
                component_sizes: canonical
                    .components
                    .iter()
                    .map(|c| c.vertices.len())
                    .collect(),
                encoded_candidates: canonical.diagnostics.candidate_encodings,
                elapsed_nanos: canonical.diagnostics.total_nanos,
            },
            pending_occurrences,
            stack_occurrences,
        };
        #[cfg(test)]
        crate::rules::transitions::NORMALIZATION_CALLS.with(|calls| calls.set(calls.get() + 1));
        Ok(JointPublicNormalization {
            data: JointPublicData {
                encoding,
                current_ids: view.objects.keys().copied().collect(),
                exact_to_coordinate,
                coordinate_to_exact,
                occurrence_coordinates,
                resolving_occurrence,
                components: canonical.components,
                diagnostics: canonical.diagnostics,
                retained,
            },
            immutable_decision: PhantomData,
        })
    }
}

impl<'a> JointPublicNormalization<'a> {
    pub fn for_state(state: &'a GameState, player: usize) -> EncodingResult<Self> {
        if player >= state.players.len() || state.players.len() < 2 {
            return Err(TerminationReason::StateEncoding);
        }
        Self::from_view(&state.visible_state(player))
    }

    pub fn from_view(view: &PlayerView<'a>) -> EncodingResult<Self> {
        PreparedPublicNormalization::from_view(view)?.finish()
    }
}

fn occurrence_info(
    view: &PlayerView,
    context: &TriggerContext,
    source_id: ObjectId,
    ability: usize,
    controller: usize,
    sources: &HashMap<ExactObjectRef, usize>,
    groups: &HashMap<u64, usize>,
    coordinate: Option<SemanticCoordinate>,
) -> EncodingResult<Option<crate::rules::transitions::ZoneOccurrenceInfo>> {
    use crate::rules::transitions::{SubjectSourceRelation, ZoneOccurrenceInfo};
    let zone = context.zone_transition.as_ref();
    let source = ExactObjectRef {
        id: source_id,
        generation: context.source_generation,
    };
    let source_info = zone.map(|zone| {
        let exact = zone.source_before.object;
        let live = if view.visible_current_generation(exact.id) == Some(exact.generation)
            && view.battlefield.contains(&exact.id)
        {
            view.zone_live_sources.get(&exact.id)
        } else {
            None
        };
        zone.source_public_info(live)
    });
    Ok(Some(ZoneOccurrenceInfo {
        source_card_id: context.source_card_id,
        controller,
        ability_index: ability,
        effect_encoding: encode(&context.effect)?,
        source: source_info,
        subject: zone
            .map(|zone| zone.public_info(view.visible_current_generation(zone.subject.after.id))),
        group_rank: zone.and_then(|zone| groups.get(&zone.group_id)).copied(),
        source_class_rank: zone.and_then(|_| sources.get(&source)).copied(),
        subject_source_relation: zone.map(|zone| {
            if zone.source_was_subject {
                SubjectSourceRelation::SelfSource
            } else if let Some(&rank) = sources.get(&zone.subject.before.object) {
                SubjectSourceRelation::OtherRetainedSource(rank)
            } else {
                SubjectSourceRelation::External
            }
        }),
        legacy_source_index: if zone.is_none() {
            sources.get(&source).copied()
        } else {
            None
        },
        decision_coordinate: coordinate,
    }))
}

impl std::ops::Deref for JointPublicNormalization<'_> {
    type Target = JointPublicData;
    fn deref(&self) -> &Self::Target {
        &self.data
    }
}
impl std::ops::Deref for JointPublicData {
    type Target = crate::rules::transitions::RetainedNormalization;
    fn deref(&self) -> &Self::Target {
        &self.retained
    }
}

fn commute(
    left: &crate::layers::LayerModification,
    right: &crate::layers::LayerModification,
) -> bool {
    use crate::layers::LayerModification::*;
    left == right
        || matches!(
            (left, right),
            (ModifyPT(..), ModifyPT(..))
                | (AddType(_), AddType(_))
                | (RemoveType(_), RemoveType(_))
                | (AddSubtype(_), AddSubtype(_))
                | (AddColor(_), AddColor(_))
                | (AddKeyword(_), AddKeyword(_))
                | (RemoveKeyword(_), RemoveKeyword(_))
        )
}

#[cfg(test)]
mod two_stage_tests {
    use super::*;
    use crate::{card::{sample, ZoneType}, game::{PendingTrigger, PendingCopyOrder, StackEntry}};
    fn state() -> GameState {
        let mut s = GameState::new(2);
        s.card_db = Some(std::sync::Arc::new(sample::build_sample_db()));
        let source = s.create_card_in_zone(sample::ids::ELVISH_VISIONARY, 0, ZoneType::Battlefield);
        s.pending_triggers.push(PendingTrigger::from_source(&s, source, 0, 0, vec![]).unwrap());
        s.create_card_in_zone(sample::ids::BLOOD_ARTIST, 0, ZoneType::Battlefield);
        let target = s.create_card_in_zone(sample::ids::LLANOWAR_ELVES, 0, ZoneType::Battlefield);
        let exact = s.exact_object(target).unwrap();
        crate::rules::transitions::transition_batch(&mut s, &[crate::rules::transitions::TransitionRequest {
            object: exact, from: ZoneType::Battlefield, to: ZoneType::Graveyard,
            kind: crate::rules::transitions::MovementKind::Put,
        }]).unwrap();
        let trigger = s.pending_triggers.iter().find(|t| t.context.zone_transition.is_some()).unwrap();
        let entry = StackEntry { id: 100, controller: 0, targets: vec![], target_generations: vec![],
            source: StackSource::TriggeredAbility { source_id: trigger.source_id,
                ability_index: trigger.ability_index, context: Box::new(trigger.context.clone()) } };
        s.stack.push(entry.clone());
        s.pending_copy_order = Some(PendingCopyOrder { controller: 0, items: vec![], selected_order: vec![],
            expected_stack_len: 1, expected_next_stack_id: s.next_stack_id,
            resolving_entry: Some(Box::new(entry)), resolving_source_generation: None });
        s
    }
    fn inventory<'v, 'a>(view: &'v PlayerView<'a>) -> Inventory<'v, 'a> {
        PreparedPublicNormalization::from_view(view).unwrap().inventory
    }
    #[test]
    fn pending_stacked_and_resolving_indices_are_explicitly_validated() {
        let s = state(); let view = s.visible_state(0);
        for corrupt in 0..14 {
            let mut i = inventory(&view);
            match corrupt {
                0 => { i.pending_vertices.pop(); }
                1 => { i.pending_vertices[0] = i.graph.vertices.len(); }
                2 => { i.pending_vertices[0] = i.pending_vertices[1]; }
                3 => { i.stack_occurrence_vertices.clear(); }
                4 => { i.stack_occurrence_vertices[0] = None; }
                5 => { i.stack_occurrence_vertices[0] = Some(i.graph.vertices.len()); }
                6 => { i.stack_occurrence_vertices[0] = Some(i.pending_vertices[0]); }
                7 => { i.stack_vertices[0] = i.graph.vertices.len(); }
                8 => { i.resolving_occurrence_vertex = None; }
                9 => { i.resolving_occurrence_vertex = Some(i.graph.vertices.len()); }
                10 => { i.resolving_entry_vertex = Some(i.graph.vertices.len()); }
                11 => { i.groups.clear(); }
                12 => { i.frames.clear(); }
                _ => { i.graph.edges.retain(|e| e.kind != EdgeKind::OccurrenceAfter); }
            }
            assert_eq!(i.validate_complete(), Err(TerminationReason::StateEncoding), "case {corrupt}");
        }
    }
    #[test]
    fn completed_preparation_preserves_full_bytes_maps_and_occurrences() {
        let s = state(); let view = s.visible_state(0);
        let direct = JointPublicNormalization::from_view(&view).unwrap();
        let staged = PreparedPublicNormalization::from_view(&view).unwrap().finish().unwrap();
        assert!(staged.components.iter().any(|component| !component.is_tree));
        assert!(staged.diagnostics.exact_search_nodes > 0);
        assert_eq!(direct.encoding, staged.encoding);
        assert_eq!(direct.exact_to_coordinate, staged.exact_to_coordinate);
        assert_eq!(direct.coordinate_to_exact, staged.coordinate_to_exact);
        assert_eq!(direct.occurrence_coordinates, staged.occurrence_coordinates);
        assert_eq!(direct.retained.pending_occurrences, staged.retained.pending_occurrences);
        assert_eq!(direct.retained.stack_occurrences, staged.retained.stack_occurrences);
        assert_eq!(direct.resolving_occurrence, staged.resolving_occurrence);
        for (a,b) in direct.components.iter().zip(&staged.components) {
            assert_eq!(a.vertices, b.vertices); assert_eq!(a.encoding, b.encoding);
        }
    }
    #[test]
    fn schema_endpoints_and_exact_reference_injectivity_precede_completion() {
        let s = state(); let view = s.visible_state(0);
        for corrupt in 0..3 {
            let mut i = inventory(&view);
            match corrupt {
                0 => { i.graph.schema += 1; }
                1 => { i.graph.edges[0].target = i.graph.vertices.len(); }
                _ => { let vertex = *i.identities.values().next().unwrap(); i.identities.insert(ExactObjectRef { id: u64::MAX, generation: 0 }, vertex); }
            }
            assert_eq!(i.validate_complete(), Err(TerminationReason::StateEncoding));
        }
    }
    #[test]
    fn malformed_pending_stacked_and_resolving_contexts_fail_during_preparation() {
        for location in 0..3 {
            let mut s = state();
            let context = match location {
                0 => &mut s.pending_triggers.iter_mut().find(|t| t.context.zone_transition.is_some()).unwrap().context,
                1 => match &mut s.stack[0].source {
                    StackSource::TriggeredAbility { context, .. } => context,
                    _ => unreachable!(),
                },
                _ => match &mut s.pending_copy_order.as_mut().unwrap().resolving_entry.as_mut().unwrap().source {
                    StackSource::TriggeredAbility { context, .. } => context,
                    _ => unreachable!(),
                },
            };
            context.zone_transition.as_mut().unwrap().subject.after.generation += 1;
            assert!(s.validate_attachment_structure().is_ok());
            let mut restored = s.clone();
            restored.restore(s.snapshot()).unwrap();
            let before = bincode::serialize(&restored).unwrap();
            let view = restored.visible_state(0);
            assert!(matches!(PreparedPublicNormalization::from_view(&view), Err(TerminationReason::StateEncoding)));
            assert!(matches!(JointPublicNormalization::from_view(&view), Err(TerminationReason::StateEncoding)));
            assert_eq!(before, bincode::serialize(&restored).unwrap());
            assert!(!restored.game_over);
        }
    }
}
