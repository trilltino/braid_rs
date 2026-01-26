use super::bubble::{
    ancestors, apply_bubbles, find_one_bubble, get_child_map, get_parent_and_child_sets,
};
use crate::antimatter::crdt_trait::PrunableCrdt;
use crate::antimatter::AntimatterCrdt;
use std::collections::{HashMap, HashSet};

pub fn prune<T: PrunableCrdt + Clone>(
    crdt_state: &mut AntimatterCrdt<T>,
    just_checking: bool,
) -> bool {
    prune_with_time(crdt_state, just_checking, u64::MAX)
}

pub fn prune_with_time<T: PrunableCrdt + Clone>(
    crdt_state: &mut AntimatterCrdt<T>,
    just_checking: bool,
    _t: u64,
) -> bool {
    // 1. Prune fissures that match (both sides received)
    let mut keys_to_delete = Vec::new();
    let fissures_snapshot = crdt_state.fissures.clone();

    for (key, f) in &fissures_snapshot {
        let other_key = format!("{}:{}:{}", f.b, f.a, f.conn);
        if let Some(_other) = fissures_snapshot.get(&other_key) {
            keys_to_delete.push(key.clone());
            keys_to_delete.push(other_key);
        }
    }

    keys_to_delete.sort();
    keys_to_delete.dedup();

    if just_checking && !keys_to_delete.is_empty() {
        return true;
    }

    if !just_checking {
        for k in keys_to_delete {
            crdt_state.fissures.remove(&k);
        }
    }

    // 2. Calculate restricted versions
    let mut restricted: HashMap<String, bool> = HashMap::new();
    for f in crdt_state.fissures.values() {
        for v in f.versions.keys() {
            restricted.insert(v.clone(), true);
        }
    }

    // Add unacked versions to restricted
    if !just_checking {
        if let Ok(acked) = ancestors(
            crdt_state,
            &crdt_state
                .acked_boundary
                .iter()
                .map(|v| (v.clone(), true))
                .collect(),
            true,
        ) {
            for v in crdt_state.t.keys() {
                if !acked.contains_key(v) {
                    restricted.insert(v.clone(), true);
                }
            }
        }
    }

    // 3. Bubble identification
    let children = get_child_map(crdt_state);
    let (parent_sets, child_sets) = get_parent_and_child_sets(crdt_state, &children);

    let mut to_bubble: HashMap<String, (String, String)> = HashMap::new();
    let mut visited: HashSet<String> = HashSet::new();

    // Find bubbles starting from current_version
    for v in crdt_state
        .current_version
        .keys()
        .cloned()
        .collect::<Vec<_>>()
    {
        if visited.contains(&v) {
            continue;
        }

        if let Some(parent_set) = parent_sets.get(&v) {
            if !parent_set.done {
                let bottom = parent_set.members.clone();
                if let Some(top) = find_one_bubble(
                    crdt_state,
                    &bottom,
                    &children,
                    &child_sets,
                    Some(&restricted),
                ) {
                    if just_checking {
                        return true;
                    }
                    let bottom_sorted: Vec<_> = bottom.keys().cloned().collect();
                    let bottom_key = bottom_sorted.first().cloned().unwrap_or_default();
                    let top_key = top.keys().next().cloned().unwrap_or_default();
                    let bubble = (bottom_key, top_key.clone());

                    for v in top.keys() {
                        to_bubble.insert(v.clone(), bubble.clone());
                    }
                    for v in bottom.keys() {
                        mark_bubble(&v, bubble.clone(), &mut to_bubble, &crdt_state.t);
                    }
                }
            }
        } else {
            let bottom: HashMap<String, bool> = [(v.clone(), true)].into_iter().collect();
            if let Some(top) = find_one_bubble(
                crdt_state,
                &bottom,
                &children,
                &child_sets,
                Some(&restricted),
            ) {
                if !top.contains_key(&v) {
                    if just_checking {
                        return true;
                    }
                    let bubble = (v.clone(), top.keys().next().cloned().unwrap_or_default());
                    for vv in top.keys() {
                        to_bubble.insert(vv.clone(), bubble.clone());
                    }
                    mark_bubble(&v, bubble, &mut to_bubble, &crdt_state.t);
                }
            }
        }
        visited.insert(v.clone());
    }

    if just_checking {
        return false;
    }

    // Apply bubbles to version graph
    apply_bubbles(crdt_state, &to_bubble);

    false
}

fn mark_bubble(
    v: &str,
    bubble: (String, String),
    to_bubble: &mut HashMap<String, (String, String)>,
    t: &HashMap<String, HashSet<String>>,
) {
    if to_bubble.contains_key(v) {
        return;
    }
    to_bubble.insert(v.to_string(), bubble.clone());
    if let Some(parents) = t.get(v) {
        for p in parents {
            mark_bubble(p, bubble.clone(), to_bubble, t);
        }
    }
}
