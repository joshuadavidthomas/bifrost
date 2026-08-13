//! Stack-safe, request-bounded algorithms over immutable dense control-flow graphs.

use std::collections::VecDeque;

use crate::analyzer::semantic::{
    CancellationToken, ControlEdgeId, ProcedureSemantics, ProgramPointId,
};
use crate::analyzer::work_budget::{BudgetLedger, WorkBudgetExceeded, define_work_dimensions};

/// Immutable directed graph with dense node identities and canonical adjacency.
///
/// Successor and predecessor iteration must be canonical and every returned edge
/// must have the same endpoints as `edge_endpoints`. Implementations are views:
/// algorithms never require a copied or normalized graph.
pub(crate) trait DenseBidirectionalGraph {
    type Node: Copy + Eq + Ord + std::fmt::Debug;
    type Edge: Copy + Eq + Ord + std::fmt::Debug;

    fn node_count(&self) -> usize;
    fn node_at(&self, index: usize) -> Option<Self::Node>;
    fn node_index(&self, node: Self::Node) -> Option<usize>;
    fn successors(
        &self,
        node: Self::Node,
    ) -> impl DoubleEndedIterator<Item = (Self::Edge, Self::Node)> + ExactSizeIterator + '_;
    fn predecessors(
        &self,
        node: Self::Node,
    ) -> impl DoubleEndedIterator<Item = (Self::Edge, Self::Node)> + ExactSizeIterator + '_;
    fn edge_endpoints(&self, edge: Self::Edge) -> Option<(Self::Node, Self::Node)>;
}

impl DenseBidirectionalGraph for ProcedureSemantics {
    type Node = ProgramPointId;
    type Edge = ControlEdgeId;

    fn node_count(&self) -> usize {
        self.points().len()
    }

    fn node_at(&self, index: usize) -> Option<Self::Node> {
        (index < self.points().len())
            .then(|| ProgramPointId::try_from_index(index).expect("validated point index fits u32"))
    }

    fn node_index(&self, node: Self::Node) -> Option<usize> {
        (node.index() < self.points().len()).then_some(node.index())
    }

    fn successors(
        &self,
        node: Self::Node,
    ) -> impl DoubleEndedIterator<Item = (Self::Edge, Self::Node)> + ExactSizeIterator + '_ {
        self.successor_edges_bidirectional(node)
            .map(|(edge_id, edge)| (edge_id, edge.target_point))
    }

    fn predecessors(
        &self,
        node: Self::Node,
    ) -> impl DoubleEndedIterator<Item = (Self::Edge, Self::Node)> + ExactSizeIterator + '_ {
        self.predecessor_edges_bidirectional(node)
            .map(|(edge_id, edge)| (edge_id, edge.source_point))
    }

    fn edge_endpoints(&self, edge: Self::Edge) -> Option<(Self::Node, Self::Node)> {
        self.control_edge(edge)
            .map(|edge| (edge.source_point, edge.target_point))
    }
}

define_work_dimensions! {
    /// Independently bounded kinds of CFG work.
    #[repr(u8)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub(crate) enum CfgAlgorithmLimit;
    /// Work completed by one or more algorithms sharing a request-local budget.
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
    pub(crate) struct CfgAlgorithmWork;
    all: [2];
    NodeVisits => node_visits = usize::MAX,
    EdgeVisits => edge_visits = usize::MAX,
}

/// Exact failed node- or edge-visit charge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CfgAlgorithmBudgetExceeded {
    pub(crate) limit_kind: CfgAlgorithmLimit,
    pub(crate) limit: usize,
    pub(crate) attempted: usize,
    pub(crate) work: CfgAlgorithmWork,
}

/// Request-local two-dimensional CFG work budget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CfgAlgorithmBudget {
    ledger: BudgetLedger<CfgAlgorithmWork>,
}

impl CfgAlgorithmBudget {
    pub(crate) const fn new(limits: CfgAlgorithmWork) -> Self {
        Self {
            ledger: BudgetLedger::new(limits, CfgAlgorithmWork::uniform(0)),
        }
    }

    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "test convenience for all on-demand algorithms")
    )]
    pub(crate) const fn uniform(limit: usize) -> Self {
        Self::new(CfgAlgorithmWork {
            node_visits: limit,
            edge_visits: limit,
        })
    }

    pub(crate) const fn limits(&self) -> CfgAlgorithmWork {
        self.ledger.limits()
    }

    pub(crate) const fn used(&self) -> CfgAlgorithmWork {
        self.ledger.used()
    }

    fn charge(&mut self, work: CfgAlgorithmWork) -> Result<(), CfgAlgorithmBudgetExceeded> {
        self.ledger
            .charge(work)
            .map_err(|exceeded| budget_exceeded(exceeded, self.ledger.used()))
    }
}

impl Default for CfgAlgorithmBudget {
    fn default() -> Self {
        Self::new(CfgAlgorithmWork::default_limits())
    }
}

fn budget_exceeded(
    exceeded: WorkBudgetExceeded<CfgAlgorithmLimit>,
    work: CfgAlgorithmWork,
) -> CfgAlgorithmBudgetExceeded {
    CfgAlgorithmBudgetExceeded {
        limit_kind: exceeded.dimension(),
        limit: exceeded.limit(),
        attempted: exceeded.attempted(),
        work,
    }
}

/// Complete failure of a bounded algorithm. No variant contains a partial result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CfgAlgorithmError<Node> {
    InvalidNode(Node),
    Cancelled { work: CfgAlgorithmWork },
    ExceededBudget(CfgAlgorithmBudgetExceeded),
}

/// Borrowed controls shared by all CFG algorithms.
#[derive(Debug)]
pub(crate) struct CfgAlgorithmRequest<'request> {
    pub(crate) budget: &'request mut CfgAlgorithmBudget,
    pub(crate) cancellation: &'request CancellationToken,
}

impl<'request> CfgAlgorithmRequest<'request> {
    pub(crate) const fn new(
        budget: &'request mut CfgAlgorithmBudget,
        cancellation: &'request CancellationToken,
    ) -> Self {
        Self {
            budget,
            cancellation,
        }
    }

    fn checkpoint<Node>(&mut self) -> Result<(), CfgAlgorithmError<Node>> {
        if self.cancellation.is_cancelled() {
            Err(CfgAlgorithmError::Cancelled {
                work: self.budget.used(),
            })
        } else {
            Ok(())
        }
    }

    fn visit_node<Node>(&mut self) -> Result<(), CfgAlgorithmError<Node>> {
        self.checkpoint()?;
        self.budget
            .charge(CfgAlgorithmWork {
                node_visits: 1,
                edge_visits: 0,
            })
            .map_err(CfgAlgorithmError::ExceededBudget)
    }

    fn visit_edge<Node>(&mut self) -> Result<(), CfgAlgorithmError<Node>> {
        self.checkpoint()?;
        self.budget
            .charge(CfgAlgorithmWork {
                node_visits: 0,
                edge_visits: 1,
            })
            .map_err(CfgAlgorithmError::ExceededBudget)
    }
}

/// Complete reachability membership with dense-order iteration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Reachability<Node> {
    membership: Box<[bool]>,
    work: CfgAlgorithmWork,
    node: std::marker::PhantomData<Node>,
}

impl<Node: Copy> Reachability<Node> {
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "dense membership is the current production consumer"
        )
    )]
    pub(crate) fn contains<G>(&self, graph: &G, node: Node) -> bool
    where
        G: DenseBidirectionalGraph<Node = Node>,
    {
        graph
            .node_index(node)
            .and_then(|index| self.membership.get(index))
            .copied()
            .unwrap_or(false)
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "dense membership is the current production consumer"
        )
    )]
    pub(crate) fn iter<'graph, G>(
        &'graph self,
        graph: &'graph G,
    ) -> impl Iterator<Item = Node> + 'graph
    where
        G: DenseBidirectionalGraph<Node = Node> + 'graph,
    {
        self.membership
            .iter()
            .enumerate()
            .filter(|(_, reachable)| **reachable)
            .map(|(index, _)| required_node(graph, index))
    }

    pub(crate) fn membership(&self) -> &[bool] {
        &self.membership
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "benchmark and future consumers inspect exact work"
        )
    )]
    pub(crate) const fn work(&self) -> CfgAlgorithmWork {
        self.work
    }
}

#[derive(Debug, Clone, Copy)]
enum Direction {
    Forward,
    Reverse,
}

pub(crate) fn forward_reachability<G>(
    graph: &G,
    start: G::Node,
    request: &mut CfgAlgorithmRequest<'_>,
) -> Result<Reachability<G::Node>, CfgAlgorithmError<G::Node>>
where
    G: DenseBidirectionalGraph,
{
    reachability(graph, start, Direction::Forward, request)
}

pub(crate) fn reverse_reachability<G>(
    graph: &G,
    start: G::Node,
    request: &mut CfgAlgorithmRequest<'_>,
) -> Result<Reachability<G::Node>, CfgAlgorithmError<G::Node>>
where
    G: DenseBidirectionalGraph,
{
    reachability(graph, start, Direction::Reverse, request)
}

fn reachability<G>(
    graph: &G,
    start: G::Node,
    direction: Direction,
    request: &mut CfgAlgorithmRequest<'_>,
) -> Result<Reachability<G::Node>, CfgAlgorithmError<G::Node>>
where
    G: DenseBidirectionalGraph,
{
    request.checkpoint()?;
    let started = request.budget.used();
    let start_index = graph
        .node_index(start)
        .ok_or(CfgAlgorithmError::InvalidNode(start))?;
    let mut membership = vec![false; graph.node_count()];
    membership[start_index] = true;
    request.visit_node()?;
    let mut stack = vec![start];

    while let Some(node) = stack.pop() {
        request.checkpoint()?;
        match direction {
            Direction::Forward => discover_adjacent(
                graph,
                graph.successors(node).rev(),
                &mut membership,
                &mut stack,
                request,
            )?,
            Direction::Reverse => discover_adjacent(
                graph,
                graph.predecessors(node),
                &mut membership,
                &mut stack,
                request,
            )?,
        }
    }

    Ok(Reachability {
        membership: membership.into_boxed_slice(),
        work: request.budget.used().saturating_sub(started),
        node: std::marker::PhantomData,
    })
}

fn discover_adjacent<G>(
    graph: &G,
    adjacent: impl Iterator<Item = (G::Edge, G::Node)>,
    membership: &mut [bool],
    stack: &mut Vec<G::Node>,
    request: &mut CfgAlgorithmRequest<'_>,
) -> Result<(), CfgAlgorithmError<G::Node>>
where
    G: DenseBidirectionalGraph,
{
    for (_, adjacent_node) in adjacent {
        request.visit_edge()?;
        let index = graph
            .node_index(adjacent_node)
            .ok_or(CfgAlgorithmError::InvalidNode(adjacent_node))?;
        if !membership[index] {
            membership[index] = true;
            request.visit_node()?;
            stack.push(adjacent_node);
        }
    }
    Ok(())
}

/// Complete deterministic iterative DFS forest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DepthFirstOrder<Node, Edge> {
    pub(crate) preorder: Box<[Node]>,
    pub(crate) postorder: Box<[Node]>,
    pub(crate) reverse_postorder: Box<[Node]>,
    pub(crate) back_edges: Box<[Edge]>,
    pub(crate) work: CfgAlgorithmWork,
}

type AlgorithmResult<T, Node> = Result<T, CfgAlgorithmError<Node>>;
type ComponentsWithOrder<Node, Edge> = (
    StronglyConnectedComponents<Node>,
    DepthFirstOrder<Node, Edge>,
);
type ShortestPathResult<Node, Edge> = AlgorithmResult<Option<ShortestPath<Node, Edge>>, Node>;

enum DfsAction<Node, Edge> {
    Enter(Node),
    Examine(Edge, Node),
    Finish(Node),
}

pub(crate) fn depth_first_order<G>(
    graph: &G,
    request: &mut CfgAlgorithmRequest<'_>,
) -> AlgorithmResult<DepthFirstOrder<G::Node, G::Edge>, G::Node>
where
    G: DenseBidirectionalGraph,
{
    request.checkpoint()?;
    let started = request.budget.used();
    let mut colors = vec![0_u8; graph.node_count()];
    let mut preorder = Vec::with_capacity(graph.node_count());
    let mut postorder = Vec::with_capacity(graph.node_count());
    let mut back_edges = Vec::new();
    let mut actions = Vec::new();

    for root_index in 0..graph.node_count() {
        if colors[root_index] != 0 {
            continue;
        }
        actions.push(DfsAction::Enter(required_node(graph, root_index)));
        while let Some(action) = actions.pop() {
            request.checkpoint()?;
            match action {
                DfsAction::Enter(node) => {
                    let index = required_index(graph, node)?;
                    if colors[index] != 0 {
                        continue;
                    }
                    request.visit_node()?;
                    colors[index] = 1;
                    preorder.push(node);
                    actions.push(DfsAction::Finish(node));
                    for (edge, target) in graph.successors(node).rev() {
                        request.visit_edge()?;
                        actions.push(DfsAction::Examine(edge, target));
                    }
                }
                DfsAction::Examine(edge, target) => {
                    let target_index = required_index(graph, target)?;
                    match colors[target_index] {
                        0 => actions.push(DfsAction::Enter(target)),
                        1 => back_edges.push(edge),
                        _ => {}
                    }
                }
                DfsAction::Finish(node) => {
                    let index = required_index(graph, node)?;
                    colors[index] = 2;
                    postorder.push(node);
                }
            }
        }
    }

    let reverse_postorder = postorder.iter().rev().copied().collect();
    Ok(DepthFirstOrder {
        preorder: preorder.into_boxed_slice(),
        postorder: postorder.into_boxed_slice(),
        reverse_postorder,
        back_edges: back_edges.into_boxed_slice(),
        work: request.budget.used().saturating_sub(started),
    })
}

/// Canonically ordered strongly connected components and dense membership.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StronglyConnectedComponents<Node> {
    pub(crate) components: Box<[Box<[Node]>]>,
    component_by_node: Box<[usize]>,
    pub(crate) work: CfgAlgorithmWork,
}

impl<Node: Copy> StronglyConnectedComponents<Node> {
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "SCC queries intentionally await a consumer")
    )]
    pub(crate) fn component_of<G>(&self, graph: &G, node: Node) -> Option<usize>
    where
        G: DenseBidirectionalGraph<Node = Node>,
    {
        graph
            .node_index(node)
            .and_then(|index| self.component_by_node.get(index))
            .copied()
    }
}

pub(crate) fn strongly_connected_components<G>(
    graph: &G,
    request: &mut CfgAlgorithmRequest<'_>,
) -> Result<StronglyConnectedComponents<G::Node>, CfgAlgorithmError<G::Node>>
where
    G: DenseBidirectionalGraph,
{
    strongly_connected_components_with_order(graph, request).map(|(components, _)| components)
}

fn strongly_connected_components_with_order<G>(
    graph: &G,
    request: &mut CfgAlgorithmRequest<'_>,
) -> AlgorithmResult<ComponentsWithOrder<G::Node, G::Edge>, G::Node>
where
    G: DenseBidirectionalGraph,
{
    request.checkpoint()?;
    let started = request.budget.used();
    let order = depth_first_order(graph, request)?;
    let mut assigned = vec![false; graph.node_count()];
    let mut raw_component_by_node = vec![usize::MAX; graph.node_count()];
    let mut raw_component_count = 0usize;

    for seed in order.reverse_postorder.iter().copied() {
        let seed_index = required_index(graph, seed)?;
        if assigned[seed_index] {
            continue;
        }
        let raw_component = raw_component_count;
        raw_component_count += 1;
        assigned[seed_index] = true;
        raw_component_by_node[seed_index] = raw_component;
        let mut stack = vec![seed];
        while let Some(node) = stack.pop() {
            request.visit_node()?;
            for (_, predecessor) in graph.predecessors(node).rev() {
                request.visit_edge()?;
                let predecessor_index = required_index(graph, predecessor)?;
                if !assigned[predecessor_index] {
                    assigned[predecessor_index] = true;
                    raw_component_by_node[predecessor_index] = raw_component;
                    stack.push(predecessor);
                }
            }
        }
    }

    let mut members_by_raw = (0..raw_component_count)
        .map(|_| Vec::<G::Node>::new())
        .collect::<Vec<_>>();
    let mut raw_order = Vec::with_capacity(raw_component_count);
    let mut raw_seen = vec![false; raw_component_count];
    for (index, &raw_component) in raw_component_by_node.iter().enumerate() {
        request.checkpoint()?;
        debug_assert_ne!(raw_component, usize::MAX);
        if !raw_seen[raw_component] {
            raw_seen[raw_component] = true;
            raw_order.push(raw_component);
        }
        members_by_raw[raw_component].push(required_node(graph, index));
    }

    let mut canonical_by_raw = vec![usize::MAX; raw_component_count];
    for (canonical, &raw_component) in raw_order.iter().enumerate() {
        request.checkpoint()?;
        canonical_by_raw[raw_component] = canonical;
    }
    let mut component_by_node = Vec::with_capacity(graph.node_count());
    for raw_component in raw_component_by_node {
        request.checkpoint()?;
        component_by_node.push(canonical_by_raw[raw_component]);
    }
    let mut components = Vec::with_capacity(raw_component_count);
    for raw_component in raw_order {
        request.checkpoint()?;
        components.push(std::mem::take(&mut members_by_raw[raw_component]).into_boxed_slice());
    }

    Ok((
        StronglyConnectedComponents {
            components: components.into_boxed_slice(),
            component_by_node: component_by_node.into_boxed_slice(),
            work: request.budget.used().saturating_sub(started),
        },
        order,
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LoopEntryStructure {
    None,
    Single,
    Multiple,
}

/// One cyclic SCC described without an unsupported dominance claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LoopRegion<Node, Edge> {
    pub(crate) members: Box<[Node]>,
    pub(crate) entries: Box<[Node]>,
    pub(crate) entry_structure: LoopEntryStructure,
    pub(crate) back_edges: Box<[Edge]>,
    pub(crate) has_self_loop: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LoopRegions<Node, Edge> {
    pub(crate) regions: Box<[LoopRegion<Node, Edge>]>,
    pub(crate) work: CfgAlgorithmWork,
}

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "issue 819 keeps loop regions available on demand")
)]
pub(crate) fn loop_regions<G>(
    graph: &G,
    request: &mut CfgAlgorithmRequest<'_>,
) -> AlgorithmResult<LoopRegions<G::Node, G::Edge>, G::Node>
where
    G: DenseBidirectionalGraph,
{
    request.checkpoint()?;
    let started = request.budget.used();
    let (components, dfs) = strongly_connected_components_with_order(graph, request)?;
    let mut self_loops = vec![false; components.components.len()];
    let mut entry_membership = vec![false; graph.node_count()];
    let mut back_edges = vec![Vec::<G::Edge>::new(); components.components.len()];

    for edge in dfs.back_edges {
        request.checkpoint()?;
        let (source, target) = graph
            .edge_endpoints(edge)
            .expect("DFS returned an edge belonging to the graph");
        let source_index = required_index(graph, source)?;
        let target_index = required_index(graph, target)?;
        let component = components.component_by_node[source_index];
        if component == components.component_by_node[target_index] {
            back_edges[component].push(edge);
        }
    }

    for source_index in 0..graph.node_count() {
        request.visit_node()?;
        let source = required_node(graph, source_index);
        let source_component = components.component_by_node[source_index];
        for (_, target) in graph.successors(source) {
            request.visit_edge()?;
            let target_index = required_index(graph, target)?;
            let target_component = components.component_by_node[target_index];
            if source == target {
                self_loops[source_component] = true;
            }
            if source_component != target_component {
                entry_membership[target_index] = true;
            }
        }
    }

    let mut regions = Vec::new();
    for (component, members) in components.components.iter().enumerate() {
        request.checkpoint()?;
        if members.len() == 1 && !self_loops[component] {
            continue;
        }
        let mut canonical_members = Vec::with_capacity(members.len());
        let mut canonical_entries = Vec::new();
        for &member in members {
            request.checkpoint()?;
            canonical_members.push(member);
            let member_index = required_index(graph, member)?;
            if entry_membership[member_index] {
                canonical_entries.push(member);
            }
        }
        let entry_structure = match canonical_entries.len() {
            0 => LoopEntryStructure::None,
            1 => LoopEntryStructure::Single,
            _ => LoopEntryStructure::Multiple,
        };
        let internal_back_edges = std::mem::take(&mut back_edges[component]);
        request.checkpoint()?;
        regions.push(LoopRegion {
            members: canonical_members.into_boxed_slice(),
            entries: canonical_entries.into_boxed_slice(),
            entry_structure,
            back_edges: internal_back_edges.into_boxed_slice(),
            has_self_loop: self_loops[component],
        });
    }

    request.checkpoint()?;
    Ok(LoopRegions {
        regions: regions.into_boxed_slice(),
        work: request.budget.used().saturating_sub(started),
    })
}

/// One deterministic shortest path, including the exact selected rich edges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ShortestPath<Node, Edge> {
    pub(crate) nodes: Box<[Node]>,
    pub(crate) edges: Box<[Edge]>,
    pub(crate) work: CfgAlgorithmWork,
}

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "issue 819 keeps shortest paths available on demand"
    )
)]
pub(crate) fn shortest_path<G>(
    graph: &G,
    start: G::Node,
    goal: G::Node,
    request: &mut CfgAlgorithmRequest<'_>,
) -> ShortestPathResult<G::Node, G::Edge>
where
    G: DenseBidirectionalGraph,
{
    request.checkpoint()?;
    let started = request.budget.used();
    let start_index = required_index(graph, start)?;
    let goal_index = required_index(graph, goal)?;
    request.visit_node()?;
    if start_index == goal_index {
        return Ok(Some(ShortestPath {
            nodes: vec![start].into_boxed_slice(),
            edges: Box::default(),
            work: request.budget.used().saturating_sub(started),
        }));
    }

    let mut discovered = vec![false; graph.node_count()];
    let mut parent = vec![None::<(G::Node, G::Edge)>; graph.node_count()];
    let mut queue = VecDeque::new();
    discovered[start_index] = true;
    queue.push_back(start);

    while let Some(node) = queue.pop_front() {
        request.checkpoint()?;
        for (edge, target) in graph.successors(node) {
            request.visit_edge()?;
            let target_index = required_index(graph, target)?;
            if discovered[target_index] {
                continue;
            }
            discovered[target_index] = true;
            parent[target_index] = Some((node, edge));
            request.visit_node()?;
            if target_index == goal_index {
                return Ok(Some(reconstruct_path(
                    graph, start, goal, &parent, started, request,
                )?));
            }
            queue.push_back(target);
        }
    }
    Ok(None)
}

fn reconstruct_path<G>(
    graph: &G,
    start: G::Node,
    goal: G::Node,
    parent: &[Option<(G::Node, G::Edge)>],
    started: CfgAlgorithmWork,
    request: &mut CfgAlgorithmRequest<'_>,
) -> AlgorithmResult<ShortestPath<G::Node, G::Edge>, G::Node>
where
    G: DenseBidirectionalGraph,
{
    let mut nodes = vec![goal];
    let mut edges = Vec::new();
    let mut cursor = goal;
    while cursor != start {
        let index = required_index(graph, cursor)?;
        let (previous, edge) = parent[index].expect("discovered path node has a parent");
        request.visit_edge()?;
        request.visit_node()?;
        edges.push(edge);
        nodes.push(previous);
        cursor = previous;
    }
    nodes.reverse();
    edges.reverse();
    Ok(ShortestPath {
        nodes: nodes.into_boxed_slice(),
        edges: edges.into_boxed_slice(),
        work: request.budget.used().saturating_sub(started),
    })
}

/// Complete immediate-dominator relation for one entry, dense by node index.
///
/// Unreachable nodes carry no dominator: they dominate nothing and nothing
/// dominates them. The entry stores itself internally so the chain walk has a
/// fixed point, and reports `None` externally because no node dominates it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Dominators<Node> {
    immediate: Box<[Option<usize>]>,
    work: CfgAlgorithmWork,
    node: std::marker::PhantomData<Node>,
}

impl<Node: Copy> Dominators<Node> {
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "issue 1480 milestone 2 flow-state derivation is the pending consumer"
        )
    )]
    pub(crate) fn immediate_dominator<G>(&self, graph: &G, node: Node) -> Option<Node>
    where
        G: DenseBidirectionalGraph<Node = Node>,
    {
        let index = graph.node_index(node)?;
        let parent = self.immediate.get(index).copied().flatten()?;
        (parent != index).then(|| required_node(graph, parent))
    }

    /// Reflexive dominance: `a` dominates itself, and dominates `b` when every
    /// entry-to-`b` path passes through `a`. The idom chain is walked
    /// iteratively and strictly ascends, so the walk always terminates.
    pub(crate) fn dominates<G>(&self, graph: &G, a: Node, b: Node) -> bool
    where
        G: DenseBidirectionalGraph<Node = Node>,
    {
        let (Some(a_index), Some(b_index)) = (graph.node_index(a), graph.node_index(b)) else {
            return false;
        };
        if self.immediate.get(a_index).copied().flatten().is_none() {
            return false;
        }
        let mut cursor = b_index;
        loop {
            let Some(parent) = self.immediate.get(cursor).copied().flatten() else {
                return false;
            };
            if cursor == a_index {
                return true;
            }
            if parent == cursor {
                return false;
            }
            cursor = parent;
        }
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "benchmark and future consumers inspect exact work"
        )
    )]
    pub(crate) const fn work(&self) -> CfgAlgorithmWork {
        self.work
    }
}

/// Cooper-Harvey-Kennedy iterative dominance over the nodes reachable from `entry`.
pub(crate) fn dominators<G>(
    graph: &G,
    entry: G::Node,
    request: &mut CfgAlgorithmRequest<'_>,
) -> AlgorithmResult<Dominators<G::Node>, G::Node>
where
    G: DenseBidirectionalGraph,
{
    request.checkpoint()?;
    let started = request.budget.used();
    let entry_index = required_index(graph, entry)?;
    let order = reverse_postorder_from(graph, entry, request)?;
    debug_assert_eq!(
        order.first().copied(),
        Some(entry),
        "reverse postorder from the entry must start at the entry"
    );
    let mut rpo_position = vec![usize::MAX; graph.node_count()];
    for (position, node) in order.iter().copied().enumerate() {
        request.checkpoint()?;
        rpo_position[required_index(graph, node)?] = position;
    }

    let mut immediate = vec![None::<usize>; graph.node_count()];
    immediate[entry_index] = Some(entry_index);
    let mut changed = true;
    while changed {
        changed = false;
        for node in order.iter().copied().skip(1) {
            let index = required_index(graph, node)?;
            request.visit_node()?;
            let mut candidate = None::<usize>;
            for (_, predecessor) in graph.predecessors(node) {
                request.visit_edge()?;
                let predecessor_index = required_index(graph, predecessor)?;
                if immediate[predecessor_index].is_none() {
                    continue;
                }
                candidate = Some(match candidate {
                    None => predecessor_index,
                    Some(current) => intersect_dominators(
                        &immediate,
                        &rpo_position,
                        current,
                        predecessor_index,
                        request,
                    )?,
                });
            }
            let candidate = candidate.expect(
                "reverse postorder reaches a node only after its depth-first tree predecessor",
            );
            if immediate[index] != Some(candidate) {
                immediate[index] = Some(candidate);
                changed = true;
            }
        }
    }

    Ok(Dominators {
        immediate: immediate.into_boxed_slice(),
        work: request.budget.used().saturating_sub(started),
        node: std::marker::PhantomData,
    })
}

fn intersect_dominators<Node>(
    immediate: &[Option<usize>],
    rpo_position: &[usize],
    mut first: usize,
    mut second: usize,
    request: &mut CfgAlgorithmRequest<'_>,
) -> Result<usize, CfgAlgorithmError<Node>> {
    while first != second {
        while rpo_position[first] > rpo_position[second] {
            request.visit_node::<Node>()?;
            first = immediate[first].expect("a processed node has an immediate dominator");
        }
        while rpo_position[second] > rpo_position[first] {
            request.visit_node::<Node>()?;
            second = immediate[second].expect("a processed node has an immediate dominator");
        }
    }
    Ok(first)
}

/// Deterministic reverse postorder over exactly the nodes reachable from `entry`.
fn reverse_postorder_from<G>(
    graph: &G,
    entry: G::Node,
    request: &mut CfgAlgorithmRequest<'_>,
) -> AlgorithmResult<Vec<G::Node>, G::Node>
where
    G: DenseBidirectionalGraph,
{
    required_index(graph, entry)?;
    let mut colors = vec![0_u8; graph.node_count()];
    let mut postorder = Vec::new();
    let mut actions = vec![DfsAction::Enter(entry)];

    while let Some(action) = actions.pop() {
        request.checkpoint()?;
        match action {
            DfsAction::Enter(node) => {
                let index = required_index(graph, node)?;
                if colors[index] != 0 {
                    continue;
                }
                request.visit_node()?;
                colors[index] = 1;
                actions.push(DfsAction::Finish(node));
                for (edge, target) in graph.successors(node).rev() {
                    request.visit_edge()?;
                    actions.push(DfsAction::Examine(edge, target));
                }
            }
            DfsAction::Examine(_, target) => {
                let target_index = required_index(graph, target)?;
                if colors[target_index] == 0 {
                    actions.push(DfsAction::Enter(target));
                }
            }
            DfsAction::Finish(node) => {
                let index = required_index(graph, node)?;
                colors[index] = 2;
                postorder.push(node);
            }
        }
    }

    postorder.reverse();
    Ok(postorder)
}

/// Definition identities per dense bitset word.
const DEFINITIONS_PER_WORD: usize = u64::BITS as usize;

/// Dense per-node generated and killed definition sets.
///
/// Definition identities are dense `usize` values minted by the caller. The
/// node count must match the graph the facts are solved over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GenKillFacts {
    node_count: usize,
    definition_count: usize,
    words_per_node: usize,
    generated: Box<[u64]>,
    killed: Box<[u64]>,
}

impl GenKillFacts {
    pub(crate) fn new(node_count: usize, definition_count: usize) -> Self {
        let words_per_node = definition_count.div_ceil(DEFINITIONS_PER_WORD);
        let words = node_count
            .checked_mul(words_per_node)
            .expect("dense gen/kill words fit in memory");
        Self {
            node_count,
            definition_count,
            words_per_node,
            generated: vec![0; words].into_boxed_slice(),
            killed: vec![0; words].into_boxed_slice(),
        }
    }

    pub(crate) fn record_generated(&mut self, node_index: usize, definition: usize) {
        let (word, bit) = self.locate(node_index, definition);
        self.generated[word] |= bit;
    }

    pub(crate) fn record_killed(&mut self, node_index: usize, definition: usize) {
        let (word, bit) = self.locate(node_index, definition);
        self.killed[word] |= bit;
    }

    fn locate(&self, node_index: usize, definition: usize) -> (usize, u64) {
        assert!(
            node_index < self.node_count,
            "node index {node_index} outside {} nodes",
            self.node_count
        );
        assert!(
            definition < self.definition_count,
            "definition {definition} outside {} definitions",
            self.definition_count
        );
        (
            node_index * self.words_per_node + definition / DEFINITIONS_PER_WORD,
            1_u64 << (definition % DEFINITIONS_PER_WORD),
        )
    }
}

/// Complete may-reaching definition sets at every node's entry.
///
/// Only the IN sets are retained: a read at a program point consults the
/// definitions that reach that point, and the OUT set is the caller's own
/// transfer of IN through the node's gen/kill facts. Nodes unreachable from
/// the entry carry the empty set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReachingSets {
    node_count: usize,
    definition_count: usize,
    words_per_node: usize,
    entry_sets: Box<[u64]>,
    work: CfgAlgorithmWork,
}

impl ReachingSets {
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "issue 1480 milestone 2 flow-state derivation is the pending consumer"
        )
    )]
    pub(crate) fn reaches_in(&self, node_index: usize, definition: usize) -> bool {
        assert!(
            node_index < self.node_count,
            "node index {node_index} outside {} nodes",
            self.node_count
        );
        assert!(
            definition < self.definition_count,
            "definition {definition} outside {} definitions",
            self.definition_count
        );
        let word = node_index * self.words_per_node + definition / DEFINITIONS_PER_WORD;
        self.entry_sets[word] & (1_u64 << (definition % DEFINITIONS_PER_WORD)) != 0
    }

    /// Ascending definition identities reaching the node's entry.
    pub(crate) fn reaching_in(&self, node_index: usize) -> impl Iterator<Item = usize> + '_ {
        assert!(
            node_index < self.node_count,
            "node index {node_index} outside {} nodes",
            self.node_count
        );
        let start = node_index * self.words_per_node;
        self.entry_sets[start..start + self.words_per_node]
            .iter()
            .copied()
            .enumerate()
            .flat_map(|(word, bits)| {
                (0..DEFINITIONS_PER_WORD)
                    .filter(move |bit| bits & (1_u64 << bit) != 0)
                    .map(move |bit| word * DEFINITIONS_PER_WORD + bit)
            })
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "benchmark and future consumers inspect exact work"
        )
    )]
    pub(crate) const fn work(&self) -> CfgAlgorithmWork {
        self.work
    }
}

/// Classic forward may gen/kill fixed point seeded in reverse postorder.
///
/// The entry's IN set is the empty boundary set even when the entry has
/// predecessors, so a definition never reaches the procedure's own start.
pub(crate) fn reaching_definitions<G>(
    graph: &G,
    entry: G::Node,
    facts: &GenKillFacts,
    request: &mut CfgAlgorithmRequest<'_>,
) -> AlgorithmResult<ReachingSets, G::Node>
where
    G: DenseBidirectionalGraph,
{
    request.checkpoint()?;
    let started = request.budget.used();
    assert_eq!(
        facts.node_count,
        graph.node_count(),
        "gen/kill facts must cover exactly the graph's nodes"
    );
    let entry_index = required_index(graph, entry)?;
    let stride = facts.words_per_node;
    let order = reverse_postorder_from(graph, entry, request)?;

    let mut in_sets = vec![0_u64; graph.node_count() * stride];
    let mut out_sets = vec![0_u64; graph.node_count() * stride];
    let mut queued = vec![false; graph.node_count()];
    let mut worklist = VecDeque::with_capacity(order.len());
    for node in order {
        request.checkpoint()?;
        queued[required_index(graph, node)?] = true;
        worklist.push_back(node);
    }
    let mut incoming = vec![0_u64; stride];
    let mut transferred = vec![0_u64; stride];

    while let Some(node) = worklist.pop_front() {
        let index = required_index(graph, node)?;
        request.visit_node()?;
        queued[index] = false;
        let row = index * stride;

        incoming.fill(0);
        if index != entry_index {
            for (_, predecessor) in graph.predecessors(node) {
                request.visit_edge()?;
                let predecessor_row = required_index(graph, predecessor)? * stride;
                for word in 0..stride {
                    incoming[word] |= out_sets[predecessor_row + word];
                }
            }
        }
        in_sets[row..row + stride].copy_from_slice(&incoming);

        let mut changed = false;
        for word in 0..stride {
            let value = (incoming[word] & !facts.killed[row + word]) | facts.generated[row + word];
            transferred[word] = value;
            changed |= value != out_sets[row + word];
        }
        if !changed {
            continue;
        }
        out_sets[row..row + stride].copy_from_slice(&transferred);
        for (_, successor) in graph.successors(node) {
            request.visit_edge()?;
            let successor_index = required_index(graph, successor)?;
            if !queued[successor_index] {
                queued[successor_index] = true;
                worklist.push_back(successor);
            }
        }
    }

    Ok(ReachingSets {
        node_count: facts.node_count,
        definition_count: facts.definition_count,
        words_per_node: stride,
        entry_sets: in_sets.into_boxed_slice(),
        work: request.budget.used().saturating_sub(started),
    })
}

fn required_node<G>(graph: &G, index: usize) -> G::Node
where
    G: DenseBidirectionalGraph,
{
    graph
        .node_at(index)
        .expect("dense graph must map every in-range index to a node")
}

fn required_index<G>(graph: &G, node: G::Node) -> Result<usize, CfgAlgorithmError<G::Node>>
where
    G: DenseBidirectionalGraph,
{
    graph
        .node_index(node)
        .filter(|index| *index < graph.node_count())
        .ok_or(CfgAlgorithmError::InvalidNode(node))
}

#[cfg(test)]
mod test_support {
    use super::DenseBidirectionalGraph;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    pub(crate) struct SyntheticEdgeId(pub(crate) usize);

    #[derive(Debug, Clone)]
    struct SyntheticEdge {
        source: usize,
        target: usize,
        label: u8,
    }

    #[derive(Debug)]
    pub(crate) struct SyntheticGraph {
        nodes: usize,
        edges: Box<[SyntheticEdge]>,
        outgoing: Box<[Box<[SyntheticEdgeId]>]>,
        incoming: Box<[Box<[SyntheticEdgeId]>]>,
    }

    impl SyntheticGraph {
        pub(crate) fn new(nodes: usize, edges: &[(usize, usize, u8)]) -> Self {
            Self::from_edges(nodes, edges.iter().copied())
        }

        pub(crate) fn from_edges(
            nodes: usize,
            edges: impl IntoIterator<Item = (usize, usize, u8)>,
        ) -> Self {
            let mut edges = edges
                .into_iter()
                .map(|(source, target, label)| SyntheticEdge {
                    source,
                    target,
                    label,
                })
                .collect::<Vec<_>>();
            edges.sort_unstable_by_key(|edge| (edge.source, edge.target, edge.label));
            let mut outgoing = vec![Vec::new(); nodes];
            let mut incoming = vec![Vec::new(); nodes];
            for (index, edge) in edges.iter().enumerate() {
                assert!(edge.source < nodes && edge.target < nodes);
                let id = SyntheticEdgeId(index);
                outgoing[edge.source].push(id);
                incoming[edge.target].push(id);
            }
            Self {
                nodes,
                edges: edges.into_boxed_slice(),
                outgoing: outgoing.into_iter().map(Vec::into_boxed_slice).collect(),
                incoming: incoming.into_iter().map(Vec::into_boxed_slice).collect(),
            }
        }

        pub(crate) fn edge_count(&self) -> usize {
            self.edges.len()
        }

        pub(crate) fn edge_label(&self, edge: SyntheticEdgeId) -> Option<u8> {
            self.edges.get(edge.0).map(|edge| edge.label)
        }
    }

    impl DenseBidirectionalGraph for SyntheticGraph {
        type Node = usize;
        type Edge = SyntheticEdgeId;

        fn node_count(&self) -> usize {
            self.nodes
        }

        fn node_at(&self, index: usize) -> Option<Self::Node> {
            (index < self.nodes).then_some(index)
        }

        fn node_index(&self, node: Self::Node) -> Option<usize> {
            (node < self.nodes).then_some(node)
        }

        fn successors(
            &self,
            node: Self::Node,
        ) -> impl DoubleEndedIterator<Item = (Self::Edge, Self::Node)> + ExactSizeIterator + '_
        {
            self.outgoing[node]
                .iter()
                .copied()
                .map(|id| (id, self.edges[id.0].target))
        }

        fn predecessors(
            &self,
            node: Self::Node,
        ) -> impl DoubleEndedIterator<Item = (Self::Edge, Self::Node)> + ExactSizeIterator + '_
        {
            self.incoming[node]
                .iter()
                .copied()
                .map(|id| (id, self.edges[id.0].source))
        }

        fn edge_endpoints(&self, edge: Self::Edge) -> Option<(Self::Node, Self::Node)> {
            self.edges
                .get(edge.0)
                .map(|edge| (edge.source, edge.target))
        }
    }
}

#[cfg(test)]
mod benchmark;

#[cfg(test)]
mod tests {
    use super::test_support::SyntheticGraph as TestGraph;
    use super::*;

    fn request<'request>(
        budget: &'request mut CfgAlgorithmBudget,
        cancellation: &'request CancellationToken,
    ) -> CfgAlgorithmRequest<'request> {
        CfgAlgorithmRequest::new(budget, cancellation)
    }

    #[test]
    fn reachability_is_dense_ordered_and_preserves_parallel_edge_work() {
        let graph = TestGraph::new(6, &[(2, 3, 4), (0, 2, 3), (0, 1, 9), (0, 1, 2), (1, 3, 1)]);
        let cancellation = CancellationToken::default();
        let mut budget = CfgAlgorithmBudget::uniform(100);
        let forward = forward_reachability(&graph, 0, &mut request(&mut budget, &cancellation))
            .expect("forward reachability");
        assert_eq!(forward.iter(&graph).collect::<Vec<_>>(), vec![0, 1, 2, 3]);
        assert!(forward.contains(&graph, 2));
        assert!(!forward.contains(&graph, 5));
        assert_eq!(
            forward.work(),
            CfgAlgorithmWork {
                node_visits: 4,
                edge_visits: 5
            }
        );

        let mut budget = CfgAlgorithmBudget::uniform(100);
        let reverse = reverse_reachability(&graph, 3, &mut request(&mut budget, &cancellation))
            .expect("reverse reachability");
        assert_eq!(reverse.iter(&graph).collect::<Vec<_>>(), vec![0, 1, 2, 3]);
    }

    #[test]
    fn dfs_rpo_and_back_edges_are_deterministic_for_permuted_edges() {
        let first = TestGraph::new(6, &[(3, 1, 0), (0, 2, 0), (2, 3, 0), (0, 1, 0), (1, 3, 0)]);
        let second = TestGraph::new(6, &[(0, 1, 0), (1, 3, 0), (0, 2, 0), (3, 1, 0), (2, 3, 0)]);
        let cancellation = CancellationToken::default();
        let run = |graph: &TestGraph| {
            let mut budget = CfgAlgorithmBudget::uniform(100);
            depth_first_order(graph, &mut request(&mut budget, &cancellation)).unwrap()
        };
        let first_order = run(&first);
        let second_order = run(&second);
        assert_eq!(first_order, second_order);
        assert_eq!(&*first_order.preorder, &[0, 1, 3, 2, 4, 5]);
        assert_eq!(&*first_order.reverse_postorder, &[5, 4, 0, 2, 1, 3]);
        assert_eq!(first_order.back_edges.len(), 1);
        assert_eq!(
            first.edge_endpoints(first_order.back_edges[0]),
            Some((3, 1))
        );
    }

    #[test]
    fn kosaraju_canonicalizes_nested_and_disconnected_components() {
        let graph = TestGraph::new(
            9,
            &[
                (0, 1, 0),
                (1, 2, 0),
                (2, 0, 0),
                (2, 3, 0),
                (3, 4, 0),
                (4, 3, 0),
                (6, 6, 0),
                (7, 8, 0),
            ],
        );
        let cancellation = CancellationToken::default();
        let mut budget = CfgAlgorithmBudget::uniform(1_000);
        let components =
            strongly_connected_components(&graph, &mut request(&mut budget, &cancellation))
                .unwrap();
        let members = components
            .components
            .iter()
            .map(|members| members.to_vec())
            .collect::<Vec<_>>();
        assert_eq!(
            members,
            vec![
                vec![0, 1, 2],
                vec![3, 4],
                vec![5],
                vec![6],
                vec![7],
                vec![8]
            ]
        );
        assert_eq!(components.component_of(&graph, 4), Some(1));
        assert_eq!(components.component_of(&graph, 99), None);
    }

    #[test]
    fn loop_regions_preserve_self_loops_and_irreducible_entries() {
        let graph = TestGraph::new(
            7,
            &[
                (0, 2, 0),
                (1, 3, 0),
                (2, 3, 0),
                (3, 4, 0),
                (4, 2, 0),
                (5, 5, 0),
            ],
        );
        let cancellation = CancellationToken::default();
        let mut budget = CfgAlgorithmBudget::uniform(1_000);
        let loops = loop_regions(&graph, &mut request(&mut budget, &cancellation)).unwrap();
        assert_eq!(loops.regions.len(), 2);
        assert_eq!(&*loops.regions[0].members, &[2, 3, 4]);
        assert_eq!(&*loops.regions[0].entries, &[2, 3]);
        assert_eq!(
            loops.regions[0].entry_structure,
            LoopEntryStructure::Multiple
        );
        assert!(!loops.regions[0].has_self_loop);
        assert!(!loops.regions[0].back_edges.is_empty());
        assert_eq!(&*loops.regions[1].members, &[5]);
        assert!(loops.regions[1].entries.is_empty());
        assert!(loops.regions[1].has_self_loop);
        assert_eq!(loops.regions[1].entry_structure, LoopEntryStructure::None);
    }

    #[test]
    fn loop_region_back_edges_are_partitioned_linearly_across_many_cycles() {
        let cycle_count = 1_000;
        let edges = (0..cycle_count)
            .flat_map(|cycle| {
                let first = cycle * 2;
                [(first, first + 1, 0), (first + 1, first, 0)]
            })
            .collect::<Vec<_>>();
        let graph = TestGraph::new(cycle_count * 2, &edges);
        let cancellation = CancellationToken::default();
        let mut budget = CfgAlgorithmBudget::uniform(edges.len() * 8);
        let loops = loop_regions(&graph, &mut request(&mut budget, &cancellation)).unwrap();

        assert_eq!(loops.regions.len(), cycle_count);
        assert!(
            loops
                .regions
                .iter()
                .all(|region| region.back_edges.len() == 1)
        );
        assert_eq!(
            loops.work,
            CfgAlgorithmWork {
                node_visits: graph.node_count() * 3,
                edge_visits: graph.edge_count() * 3,
            }
        );
    }

    #[test]
    fn shortest_path_uses_canonical_rich_edge_tie_breaking() {
        let graph = TestGraph::new(5, &[(0, 2, 0), (2, 4, 0), (0, 1, 9), (0, 1, 1), (1, 4, 0)]);
        let cancellation = CancellationToken::default();
        let mut budget = CfgAlgorithmBudget::uniform(100);
        let path = shortest_path(&graph, 0, 4, &mut request(&mut budget, &cancellation))
            .unwrap()
            .unwrap();
        assert_eq!(&*path.nodes, &[0, 1, 4]);
        assert_eq!(graph.edge_label(path.edges[0]), Some(1));

        let mut budget = CfgAlgorithmBudget::uniform(100);
        let zero = shortest_path(&graph, 3, 3, &mut request(&mut budget, &cancellation))
            .unwrap()
            .unwrap();
        assert_eq!(&*zero.nodes, &[3]);
        assert!(zero.edges.is_empty());

        let mut budget = CfgAlgorithmBudget::uniform(100);
        assert!(
            shortest_path(&graph, 4, 0, &mut request(&mut budget, &cancellation))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn shortest_path_cancellation_during_reconstruction_returns_no_path() {
        let node_count = 100;
        let edges = (0..node_count - 1)
            .map(|source| (source, source + 1, 0))
            .collect::<Vec<_>>();
        let graph = TestGraph::new(node_count, &edges);
        let cancellation = CancellationToken::cancel_after_checks_for_test(300);
        let mut budget = CfgAlgorithmBudget::uniform(1_000);

        assert!(matches!(
            shortest_path(
                &graph,
                0,
                node_count - 1,
                &mut request(&mut budget, &cancellation)
            ),
            Err(CfgAlgorithmError::Cancelled { .. })
        ));
    }

    #[test]
    fn shortest_path_reconstruction_is_charged_to_the_visit_budget() {
        let graph = TestGraph::new(4, &[(0, 1, 0), (1, 2, 0), (2, 3, 0)]);
        let cancellation = CancellationToken::default();
        let mut budget = CfgAlgorithmBudget::new(CfgAlgorithmWork {
            node_visits: 4,
            edge_visits: 3,
        });

        assert!(matches!(
            shortest_path(&graph, 0, 3, &mut request(&mut budget, &cancellation)),
            Err(CfgAlgorithmError::ExceededBudget(
                CfgAlgorithmBudgetExceeded {
                    limit_kind: CfgAlgorithmLimit::EdgeVisits,
                    attempted: 4,
                    ..
                }
            ))
        ));
    }

    #[test]
    fn scc_and_loop_emission_observe_cancellation_after_graph_visits() {
        let node_count = 32;
        let edges = (0..node_count)
            .map(|source| (source, (source + 1) % node_count, 0))
            .collect::<Vec<_>>();
        let graph = TestGraph::new(node_count, &edges);

        let cancellation = CancellationToken::default();
        let mut budget = CfgAlgorithmBudget::uniform(1_000);
        let scc_work =
            strongly_connected_components(&graph, &mut request(&mut budget, &cancellation))
                .unwrap()
                .work;
        let mut cancelled_during_scc_emission = false;
        for checks in 1..1_000 {
            let cancellation = CancellationToken::cancel_after_checks_for_test(checks);
            let mut budget = CfgAlgorithmBudget::uniform(1_000);
            if matches!(
                strongly_connected_components(
                    &graph,
                    &mut request(&mut budget, &cancellation)
                ),
                Err(CfgAlgorithmError::Cancelled { work }) if work == scc_work
            ) {
                cancelled_during_scc_emission = true;
                break;
            }
        }
        assert!(cancelled_during_scc_emission);

        let cancellation = CancellationToken::default();
        let mut budget = CfgAlgorithmBudget::uniform(1_000);
        let loop_work = loop_regions(&graph, &mut request(&mut budget, &cancellation))
            .unwrap()
            .work;
        let mut cancelled_during_loop_emission = false;
        for checks in 1..2_000 {
            let cancellation = CancellationToken::cancel_after_checks_for_test(checks);
            let mut budget = CfgAlgorithmBudget::uniform(1_000);
            if matches!(
                loop_regions(&graph, &mut request(&mut budget, &cancellation)),
                Err(CfgAlgorithmError::Cancelled { work }) if work == loop_work
            ) {
                cancelled_during_loop_emission = true;
                break;
            }
        }
        assert!(cancelled_during_loop_emission);
    }

    #[test]
    fn invalid_nodes_budget_exhaustion_and_cancellation_are_typed() {
        let graph = TestGraph::new(3, &[(0, 1, 0), (1, 2, 0)]);
        let cancellation = CancellationToken::default();
        let mut budget = CfgAlgorithmBudget::uniform(10);
        assert_eq!(
            forward_reachability(&graph, 9, &mut request(&mut budget, &cancellation)),
            Err(CfgAlgorithmError::InvalidNode(9))
        );

        let mut budget = CfgAlgorithmBudget::new(CfgAlgorithmWork {
            node_visits: 1,
            edge_visits: 10,
        });
        let error =
            forward_reachability(&graph, 0, &mut request(&mut budget, &cancellation)).unwrap_err();
        assert!(matches!(
            error,
            CfgAlgorithmError::ExceededBudget(CfgAlgorithmBudgetExceeded {
                limit_kind: CfgAlgorithmLimit::NodeVisits,
                ..
            })
        ));

        let mut budget = CfgAlgorithmBudget::new(CfgAlgorithmWork {
            node_visits: 10,
            edge_visits: 0,
        });
        let error =
            forward_reachability(&graph, 0, &mut request(&mut budget, &cancellation)).unwrap_err();
        assert!(matches!(
            error,
            CfgAlgorithmError::ExceededBudget(CfgAlgorithmBudgetExceeded {
                limit_kind: CfgAlgorithmLimit::EdgeVisits,
                ..
            })
        ));

        let cancelled = CancellationToken::default();
        cancelled.cancel();
        let mut budget = CfgAlgorithmBudget::uniform(10);
        assert!(matches!(
            forward_reachability(&graph, 0, &mut request(&mut budget, &cancelled)),
            Err(CfgAlgorithmError::Cancelled {
                work: CfgAlgorithmWork {
                    node_visits: 0,
                    edge_visits: 0
                }
            })
        ));

        let mid_traversal = CancellationToken::cancel_after_checks_for_test(5);
        let mut budget = CfgAlgorithmBudget::uniform(10);
        assert!(matches!(
            forward_reachability(&graph, 0, &mut request(&mut budget, &mid_traversal)),
            Err(CfgAlgorithmError::Cancelled { .. })
        ));
    }

    #[test]
    fn dominance_on_a_diamond_separates_branches_from_the_join() {
        let graph = TestGraph::new(5, &[(0, 1, 0), (0, 2, 0), (1, 3, 0), (2, 3, 0)]);
        let cancellation = CancellationToken::default();
        let mut budget = CfgAlgorithmBudget::uniform(100);
        let dominance = dominators(&graph, 0, &mut request(&mut budget, &cancellation)).unwrap();

        assert_eq!(dominance.immediate_dominator(&graph, 0), None);
        assert_eq!(dominance.immediate_dominator(&graph, 1), Some(0));
        assert_eq!(dominance.immediate_dominator(&graph, 2), Some(0));
        assert_eq!(dominance.immediate_dominator(&graph, 3), Some(0));
        for node in 0..4 {
            assert!(dominance.dominates(&graph, 0, node));
            assert!(dominance.dominates(&graph, node, node));
        }
        assert!(!dominance.dominates(&graph, 1, 3));
        assert!(!dominance.dominates(&graph, 2, 3));
        assert!(!dominance.dominates(&graph, 3, 1));

        assert_eq!(dominance.immediate_dominator(&graph, 4), None);
        assert!(!dominance.dominates(&graph, 4, 4));
        assert!(!dominance.dominates(&graph, 0, 4));
        assert!(!dominance.dominates(&graph, 4, 3));
        assert_eq!(dominance.immediate_dominator(&graph, 9), None);
        assert!(!dominance.dominates(&graph, 9, 0));
        assert!(dominance.work().node_visits > 0);
    }

    #[test]
    fn dominance_on_a_loop_puts_the_header_over_the_body() {
        let graph = TestGraph::new(4, &[(0, 1, 0), (1, 2, 0), (1, 3, 0), (2, 1, 0)]);
        let cancellation = CancellationToken::default();
        let mut budget = CfgAlgorithmBudget::uniform(1_000);
        let dominance = dominators(&graph, 0, &mut request(&mut budget, &cancellation)).unwrap();

        assert_eq!(dominance.immediate_dominator(&graph, 1), Some(0));
        assert_eq!(dominance.immediate_dominator(&graph, 2), Some(1));
        assert_eq!(dominance.immediate_dominator(&graph, 3), Some(1));
        assert!(dominance.dominates(&graph, 1, 2));
        assert!(dominance.dominates(&graph, 1, 3));
        assert!(!dominance.dominates(&graph, 2, 3));
    }

    #[test]
    fn dominance_reports_invalid_entries_and_typed_budget_exhaustion() {
        let graph = TestGraph::new(4, &[(0, 1, 0), (1, 2, 0), (2, 3, 0)]);
        let cancellation = CancellationToken::default();
        let mut budget = CfgAlgorithmBudget::uniform(100);
        assert_eq!(
            dominators(&graph, 9, &mut request(&mut budget, &cancellation)),
            Err(CfgAlgorithmError::InvalidNode(9))
        );

        let mut budget = CfgAlgorithmBudget::new(CfgAlgorithmWork {
            node_visits: 2,
            edge_visits: 100,
        });
        assert!(matches!(
            dominators(&graph, 0, &mut request(&mut budget, &cancellation)),
            Err(CfgAlgorithmError::ExceededBudget(
                CfgAlgorithmBudgetExceeded {
                    limit_kind: CfgAlgorithmLimit::NodeVisits,
                    limit: 2,
                    ..
                }
            ))
        ));

        let cancelled = CancellationToken::default();
        cancelled.cancel();
        let mut budget = CfgAlgorithmBudget::uniform(100);
        assert!(matches!(
            dominators(&graph, 0, &mut request(&mut budget, &cancelled)),
            Err(CfgAlgorithmError::Cancelled { .. })
        ));
    }

    #[test]
    fn reaching_definitions_carry_a_straight_line_definition_until_a_kill() {
        let graph = TestGraph::new(4, &[(0, 1, 0), (1, 2, 0), (2, 3, 0)]);
        let mut facts = GenKillFacts::new(4, 2);
        for definition in 0..2 {
            facts.record_killed(0, definition);
            facts.record_killed(2, definition);
        }
        facts.record_generated(0, 0);
        facts.record_generated(2, 1);

        let cancellation = CancellationToken::default();
        let mut budget = CfgAlgorithmBudget::uniform(1_000);
        let reaching =
            reaching_definitions(&graph, 0, &facts, &mut request(&mut budget, &cancellation))
                .unwrap();

        assert_eq!(
            reaching.reaching_in(0).collect::<Vec<usize>>(),
            Vec::<usize>::new()
        );
        assert_eq!(reaching.reaching_in(1).collect::<Vec<_>>(), vec![0]);
        assert!(reaching.reaches_in(2, 0));
        assert_eq!(reaching.reaching_in(3).collect::<Vec<_>>(), vec![1]);
        assert!(!reaching.reaches_in(3, 0));
        assert!(reaching.work().node_visits > 0);
    }

    #[test]
    fn reaching_definitions_join_both_arms_of_a_diamond() {
        let graph = TestGraph::new(5, &[(0, 1, 0), (0, 2, 0), (1, 3, 0), (2, 3, 0)]);
        let mut facts = GenKillFacts::new(5, 3);
        for node in [0, 1, 2] {
            for definition in 0..3 {
                facts.record_killed(node, definition);
            }
            facts.record_generated(node, node);
        }

        let cancellation = CancellationToken::default();
        let mut budget = CfgAlgorithmBudget::uniform(1_000);
        let reaching =
            reaching_definitions(&graph, 0, &facts, &mut request(&mut budget, &cancellation))
                .unwrap();

        assert_eq!(reaching.reaching_in(1).collect::<Vec<_>>(), vec![0]);
        assert_eq!(reaching.reaching_in(2).collect::<Vec<_>>(), vec![0]);
        assert_eq!(reaching.reaching_in(3).collect::<Vec<_>>(), vec![1, 2]);
        assert!(!reaching.reaches_in(3, 0));
        assert_eq!(
            reaching.reaching_in(4).collect::<Vec<usize>>(),
            Vec::<usize>::new()
        );
    }

    #[test]
    fn reaching_definitions_propagate_a_loop_body_definition_back_to_the_header() {
        let graph = TestGraph::new(4, &[(0, 1, 0), (1, 2, 0), (1, 3, 0), (2, 1, 0)]);
        let mut facts = GenKillFacts::new(4, 2);
        for node in [0, 2] {
            for definition in 0..2 {
                facts.record_killed(node, definition);
            }
        }
        facts.record_generated(0, 0);
        facts.record_generated(2, 1);

        let cancellation = CancellationToken::default();
        let mut budget = CfgAlgorithmBudget::uniform(1_000);
        let reaching =
            reaching_definitions(&graph, 0, &facts, &mut request(&mut budget, &cancellation))
                .unwrap();

        assert_eq!(reaching.reaching_in(1).collect::<Vec<_>>(), vec![0, 1]);
        assert_eq!(reaching.reaching_in(2).collect::<Vec<_>>(), vec![0, 1]);
        assert_eq!(reaching.reaching_in(3).collect::<Vec<_>>(), vec![0, 1]);
    }

    #[test]
    fn reaching_definitions_report_typed_budget_exhaustion_and_cancellation() {
        let graph = TestGraph::new(4, &[(0, 1, 0), (1, 2, 0), (2, 3, 0)]);
        let facts = GenKillFacts::new(4, 1);
        let cancellation = CancellationToken::default();

        let mut budget = CfgAlgorithmBudget::new(CfgAlgorithmWork {
            node_visits: 3,
            edge_visits: 100,
        });
        assert!(matches!(
            reaching_definitions(&graph, 0, &facts, &mut request(&mut budget, &cancellation)),
            Err(CfgAlgorithmError::ExceededBudget(
                CfgAlgorithmBudgetExceeded {
                    limit_kind: CfgAlgorithmLimit::NodeVisits,
                    limit: 3,
                    ..
                }
            ))
        ));

        let mut budget = CfgAlgorithmBudget::new(CfgAlgorithmWork {
            node_visits: 100,
            edge_visits: 2,
        });
        assert!(matches!(
            reaching_definitions(&graph, 0, &facts, &mut request(&mut budget, &cancellation)),
            Err(CfgAlgorithmError::ExceededBudget(
                CfgAlgorithmBudgetExceeded {
                    limit_kind: CfgAlgorithmLimit::EdgeVisits,
                    ..
                }
            ))
        ));

        let mid_traversal = CancellationToken::cancel_after_checks_for_test(3);
        let mut budget = CfgAlgorithmBudget::uniform(1_000);
        assert!(matches!(
            reaching_definitions(&graph, 0, &facts, &mut request(&mut budget, &mid_traversal)),
            Err(CfgAlgorithmError::Cancelled { .. })
        ));
    }

    #[test]
    fn dominance_and_reaching_definitions_are_deterministic_across_runs() {
        let graph = TestGraph::new(
            7,
            &[
                (0, 1, 0),
                (0, 2, 0),
                (1, 3, 0),
                (2, 3, 0),
                (3, 4, 0),
                (4, 3, 0),
                (4, 5, 0),
            ],
        );
        let mut facts = GenKillFacts::new(7, 4);
        for (node, definition) in [(0_usize, 0_usize), (1, 1), (2, 2), (4, 3)] {
            for killed in 0..4 {
                facts.record_killed(node, killed);
            }
            facts.record_generated(node, definition);
        }
        let cancellation = CancellationToken::default();

        let run = || {
            let mut budget = CfgAlgorithmBudget::uniform(10_000);
            let mut request = CfgAlgorithmRequest::new(&mut budget, &cancellation);
            let dominance = dominators(&graph, 0, &mut request).unwrap();
            let reaching = reaching_definitions(&graph, 0, &facts, &mut request).unwrap();
            (dominance, reaching)
        };
        assert_eq!(run(), run());

        let (dominance, reaching) = run();
        assert_eq!(dominance.immediate_dominator(&graph, 3), Some(0));
        assert_eq!(dominance.immediate_dominator(&graph, 5), Some(4));
        assert!(dominance.dominates(&graph, 3, 5));
        assert_eq!(reaching.reaching_in(3).collect::<Vec<_>>(), vec![1, 2, 3]);
        assert_eq!(
            reaching.reaching_in(6).collect::<Vec<usize>>(),
            Vec::<usize>::new()
        );
    }

    #[test]
    fn hundred_thousand_node_chain_is_stack_safe() {
        let node_count = 100_000;
        let edges = (0..node_count - 1)
            .map(|source| (source, source + 1, 0))
            .collect::<Vec<_>>();
        let graph = TestGraph::new(node_count, &edges);
        let cancellation = CancellationToken::default();

        let mut budget = CfgAlgorithmBudget::uniform(node_count * 4);
        let order = depth_first_order(&graph, &mut request(&mut budget, &cancellation)).unwrap();
        assert_eq!(order.preorder.len(), node_count);
        assert_eq!(order.reverse_postorder.len(), node_count);

        let mut budget = CfgAlgorithmBudget::uniform(node_count * 4);
        let components =
            strongly_connected_components(&graph, &mut request(&mut budget, &cancellation))
                .unwrap();
        assert_eq!(components.components.len(), node_count);
    }
}
