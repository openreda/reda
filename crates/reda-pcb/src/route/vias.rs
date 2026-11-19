//! Ranked Via choices for a route changing physical Layer. A 32-Layer board can
//! offer over a billion distinct stacks, so only counts and individual choices
//! are stored. Table order breaks ties within the same amount of excess span.
use reda_model::{PcbDocument, Uuid};

#[derive(Clone, Debug)]
struct Edge {
    id: Uuid,
    start: usize,
    end: usize,
}

/// Choices with the same first Via and the same final physical Layer.
#[derive(Clone, Debug)]
struct Group {
    first: usize,
    end: usize,
    count: usize,
}

#[derive(Clone, Debug, Default)]
pub struct ViaCandidates {
    edges: Vec<Edge>,
    /// Outgoing edges in Via types table order, indexed by physical Layer.
    outgoing: Vec<Vec<usize>>,
    /// Number of paths from an inner join to each possible final Layer.
    paths: Vec<Vec<usize>>,
    groups: Vec<Group>,
    len: usize,
}

impl ViaCandidates {
    /// Uses the Net's resolved permissions and normalizes the requested Layer pair.
    /// A saved choice has the same type IDs and rank in either routing direction.
    pub fn between(pcb: &PcbDocument, net: Option<Uuid>, from: Uuid, to: Uuid) -> Self {
        let position = |id| pcb.copper_number(id).map(|n| n - 1);
        let (Some(a), Some(b)) = (position(from), position(to)) else {
            return Self::default();
        };
        if a == b {
            return Self::default();
        }
        let edges = pcb
            .stackup()
            .via_types
            .iter()
            .filter(|t| crate::rules::via_type_allowed(pcb, net, t.uuid))
            .filter_map(|t| {
                let (a, b) = (position(t.from)?, position(t.to)?);
                Some(Edge {
                    id: t.uuid,
                    start: a.min(b),
                    end: a.max(b),
                })
            })
            .collect();
        Self::build(edges, pcb.copper_layers().count(), a.min(b), a.max(b))
    }

    fn build(edges: Vec<Edge>, layers: usize, low: usize, high: usize) -> Self {
        let mut outgoing = vec![Vec::new(); layers];
        for (index, edge) in edges.iter().enumerate() {
            outgoing[edge.start].push(index);
        }
        // End at the first Via that reaches the destination. Each inner join advances
        // the route and has already been counted when visiting shallower positions.
        let mut paths = vec![vec![0; layers]; layers];
        for (end, row) in paths.iter_mut().enumerate().skip(high) {
            row[end] = 1;
        }
        for start in (low + 1..high).rev() {
            let (prefix, following) = paths.split_at_mut(start + 1);
            let row = &mut prefix[start];
            for &edge in &outgoing[start] {
                let next = &following[edges[edge].end - start - 1];
                for (total, ways) in row[high..].iter_mut().zip(&next[high..]) {
                    *total += ways;
                }
            }
        }
        let mut groups = Vec::new();
        for (first, edge) in edges.iter().enumerate() {
            if edge.start > low || edge.end <= low {
                continue;
            }
            for (end, &count) in paths[edge.end].iter().enumerate().skip(high) {
                if count != 0 {
                    groups.push(Group { first, end, count });
                }
            }
        }
        groups.sort_by_key(|g| {
            let first = &edges[g.first];
            let excess = (low - first.start) + (g.end - high);
            (excess, first.end != g.end, g.first)
        });
        let len = groups.iter().map(|g| g.count).sum();
        Self {
            edges,
            outgoing,
            paths,
            groups,
            len,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The `index`th single Via or stack, in canonical top-to-bottom order. Each
    /// type keeps its own drilling direction independently of this ordering.
    pub fn get(&self, mut index: usize) -> Option<Vec<Uuid>> {
        let group = self.groups.iter().find(|g| {
            if index < g.count {
                true
            } else {
                index -= g.count;
                false
            }
        })?;
        let mut edge = &self.edges[group.first];
        let mut ids = vec![edge.id];
        while edge.end != group.end {
            edge = self.outgoing[edge.end].iter().find_map(|&i| {
                let next = &self.edges[i];
                let count = self.paths[next.end][group.end];
                if index < count {
                    Some(next)
                } else {
                    index -= count;
                    None
                }
            })?;
            ids.push(edge.id);
        }
        Some(ids)
    }

    /// Restore an editor-state choice after type reorder, edits or permission changes.
    pub fn index_of(&self, ids: &[Uuid]) -> Option<usize> {
        let first = *ids.first()?;
        let last = *ids.last()?;
        let end = self.edges.iter().find(|e| e.id == last)?.end;
        let mut index = 0;
        let group = self.groups.iter().find(|g| {
            if self.edges[g.first].id == first && g.end == end {
                true
            } else {
                index += g.count;
                false
            }
        })?;
        let mut at = self.edges[group.first].end;
        for &id in &ids[1..] {
            let next = self.outgoing[at].iter().find_map(|&i| {
                let edge = &self.edges[i];
                let count = self.paths[edge.end][end];
                if edge.id == id && count != 0 {
                    Some(edge)
                } else {
                    index += count;
                    None
                }
            })?;
            at = next.end;
        }
        (at == end).then_some(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fully_connected_32_layer_stack_is_counted_and_random_accessible() {
        let mut edges = Vec::new();
        for start in 0..32 {
            for end in start + 1..32 {
                edges.push(Edge {
                    id: Uuid::from_u128((start * 32 + end) as u128),
                    start,
                    end,
                });
            }
        }
        let choices = ViaCandidates::build(edges, 32, 0, 31);
        assert_eq!(choices.len(), 1 << 30);
        assert_eq!(choices.get(0), Some(vec![Uuid::from_u128(31)]));
        for index in [1, 147, choices.len() / 2, choices.len() - 1] {
            assert_eq!(choices.index_of(&choices.get(index).unwrap()), Some(index));
        }
    }
}
