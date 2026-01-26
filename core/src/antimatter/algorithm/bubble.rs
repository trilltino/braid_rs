use crate::antimatter::crdt_trait::PrunableCrdt;
use crate::antimatter::state::{ChildSet, ParentSet};
use crate::antimatter::AntimatterCrdt;
use crate::core::Result;
use std::collections::{HashMap, HashSet};

pub fn get_child_map<T: PrunableCrdt + Clone>(
    crdt_state: &AntimatterCrdt<T>,
) -> HashMap<String, HashSet<String>> {
    let mut children = HashMap::new();
    for (v, parents) in &crdt_state.t {
        for parent in parents {
            children
                .entry(parent.clone())
                .or_insert_with(HashSet::new)
                .insert(v.clone());
        }
    }
    children
}

pub fn get_parent_and_child_sets<T: PrunableCrdt + Clone>(
    crdt_state: &AntimatterCrdt<T>,
    children: &HashMap<String, HashSet<String>>,
) -> (HashMap<String, ParentSet>, HashMap<String, ChildSet>) {
    let mut parent_sets: HashMap<String, ParentSet> = HashMap::new();
    let mut child_sets: HashMap<String, ChildSet> = HashMap::new();
    let mut done: HashSet<String> = HashSet::new();

    // Add current_version as a parent set
    if crdt_state.current_version.len() >= 2 {
        let members: HashMap<String, bool> = crdt_state.current_version.clone();
        let parent_set = ParentSet {
            members: members.clone(),
            done: false,
        };
        for v in crdt_state.current_version.keys() {
            parent_sets.insert(v.clone(), parent_set.clone());
            done.insert(v.clone());
        }
    }

    // Find other parent/child sets
    for v in crdt_state.t.keys() {
        if done.contains(v) {
            continue;
        }
        done.insert(v.clone());

        if let Some(child_set) = children.get(v) {
            if child_set.len() >= 2 {
                // Check if all children have same parents
                let first_child = child_set.iter().next();
                if let Some(first_child) = first_child {
                    if let Some(first_parent_set) = crdt_state.t.get(first_child) {
                        let all_same = child_set.iter().all(|c| {
                            crdt_state.t.get(c).map_or(false, |ps| {
                                ps.len() == first_parent_set.len()
                                    && ps.iter().all(|p| first_parent_set.contains(p))
                            })
                        });

                        if all_same {
                            let members: HashMap<String, bool> =
                                child_set.iter().map(|c| (c.clone(), true)).collect();
                            let cs = ChildSet {
                                members: members.clone(),
                            };
                            for c in child_set {
                                child_sets.insert(c.clone(), cs.clone());
                            }
                        }
                    }
                }
            }
        }
    }

    (parent_sets, child_sets)
}

pub fn find_one_bubble<T: PrunableCrdt + Clone>(
    crdt_state: &AntimatterCrdt<T>,
    bottom: &HashMap<String, bool>,
    children: &HashMap<String, HashSet<String>>,
    child_sets: &HashMap<String, ChildSet>,
    restricted: Option<&HashMap<String, bool>>,
) -> Option<HashMap<String, bool>> {
    let mut expecting: HashMap<String, bool> = bottom.clone();
    let mut seen: HashSet<String> = HashSet::new();

    // Mark children of bottom as seen
    for v in bottom.keys() {
        if let Some(kids) = children.get(v) {
            for k in kids {
                seen.insert(k.clone());
            }
        }
    }

    let mut queue: Vec<String> = expecting.keys().cloned().collect();
    let mut last_top: Option<HashMap<String, bool>> = None;

    while let Some(cur) = queue.pop() {
        if !crdt_state.t.contains_key(&cur) {
            if restricted.is_none() {
                return None;
            }
            return last_top;
        }

        if let Some(r) = restricted {
            if r.contains_key(&cur) {
                return last_top;
            }
        }

        if seen.contains(&cur) {
            continue;
        }

        if let Some(kids) = children.get(&cur) {
            if !kids.iter().all(|c| seen.contains(c)) {
                continue;
            }
        }

        seen.insert(cur.clone());
        expecting.remove(&cur);

        if expecting.is_empty() {
            last_top = Some([(cur.clone(), true)].into_iter().collect());
            if restricted.is_none() {
                return last_top;
            }
        }

        if let Some(parents) = crdt_state.t.get(&cur) {
            for p in parents {
                expecting.insert(p.clone(), true);
                queue.push(p.clone());
            }
        }

        if let Some(cs) = child_sets.get(&cur) {
            if cs.members.keys().all(|v| seen.contains(v)) {
                let expecting_keys: HashSet<_> = expecting.keys().cloned().collect();
                if let Some(parents) = crdt_state.t.get(&cur) {
                    let parents_set: HashSet<_> = parents.iter().cloned().collect();
                    if expecting_keys.len() == parents_set.len()
                        && expecting_keys.iter().all(|e| parents_set.contains(e))
                    {
                        last_top = Some(cs.members.clone());
                        if restricted.is_none() {
                            return last_top;
                        }
                    }
                }
            }
        }
    }

    last_top
}

pub fn apply_bubbles<T: PrunableCrdt + Clone>(
    crdt_state: &mut AntimatterCrdt<T>,
    to_bubble: &HashMap<String, (String, String)>,
) {
    if to_bubble.is_empty() {
        return;
    }

    let old_t = std::mem::take(&mut crdt_state.t);
    for (v, parents) in old_t {
        let new_v = if let Some((bottom, _)) = to_bubble.get(&v) {
            bottom.clone()
        } else {
            v.clone()
        };

        let new_parents: HashSet<String> = parents
            .iter()
            .map(|p| {
                if let Some((_, top)) = to_bubble.get(p) {
                    top.clone()
                } else {
                    p.clone()
                }
            })
            .collect();

        crdt_state
            .t
            .entry(new_v)
            .or_insert_with(HashSet::new)
            .extend(new_parents);
    }

    let old_cv = std::mem::take(&mut crdt_state.current_version);
    for (v, _) in old_cv {
        let new_v = if let Some((bottom, _)) = to_bubble.get(&v) {
            bottom.clone()
        } else {
            v
        };
        crdt_state.current_version.insert(new_v, true);
    }

    let old_ab: Vec<String> = crdt_state.acked_boundary.iter().cloned().collect();
    crdt_state.acked_boundary.clear();
    for v in old_ab {
        let new_v = if let Some((bottom, _)) = to_bubble.get(&v) {
            bottom.clone()
        } else {
            v
        };
        crdt_state.acked_boundary.insert(new_v);
    }

    for (v, (bottom, _)) in to_bubble {
        if v != bottom {
            crdt_state
                .version_groups
                .entry(bottom.clone())
                .or_insert_with(Vec::new)
                .push(v.clone());
        }
    }

    for (v, (bottom, _)) in to_bubble {
        if v != bottom {
            crdt_state.crdt.prune(v);
        }
    }
}

pub fn ancestors<T: PrunableCrdt + Clone>(
    crdt_state: &AntimatterCrdt<T>,
    versions: &HashMap<String, bool>,
    ignore_nonexistent: bool,
) -> Result<HashMap<String, bool>> {
    let mut result = HashMap::new();

    fn recurse(
        v: &str,
        t: &HashMap<String, HashSet<String>>,
        result: &mut HashMap<String, bool>,
        ignore_nonexistent: bool,
    ) -> Result<()> {
        if result.contains_key(v) {
            return Ok(());
        }
        if !t.contains_key(v) {
            if ignore_nonexistent {
                return Ok(());
            }
            return Err(crate::core::BraidError::Internal(format!(
                "The version {} does not exist",
                v
            )));
        }

        result.insert(v.to_string(), true);

        if let Some(parents) = t.get(v) {
            for p in parents {
                recurse(p, t, result, ignore_nonexistent)?;
            }
        }
        Ok(())
    }

    for v in versions.keys() {
        recurse(v, &crdt_state.t, &mut result, ignore_nonexistent)?;
    }

    Ok(result)
}

pub fn descendants<T: PrunableCrdt + Clone>(
    crdt_state: &AntimatterCrdt<T>,
    versions: &HashMap<String, bool>,
    ignore_nonexistent: bool,
) -> Result<HashMap<String, bool>> {
    let children = get_child_map(crdt_state);
    let mut result = HashMap::new();

    fn recurse(
        v: &str,
        children: &HashMap<String, HashSet<String>>,
        t: &HashMap<String, HashSet<String>>,
        result: &mut HashMap<String, bool>,
        ignore_nonexistent: bool,
    ) -> Result<()> {
        if result.contains_key(v) {
            return Ok(());
        }
        if !t.contains_key(v) {
            if ignore_nonexistent {
                return Ok(());
            }
            return Err(crate::core::BraidError::Internal(format!(
                "The version {} does not exist",
                v
            )));
        }

        result.insert(v.to_string(), true);

        if let Some(kids) = children.get(v) {
            for k in kids {
                recurse(k, children, t, result, ignore_nonexistent)?;
            }
        }
        Ok(())
    }

    for v in versions.keys() {
        recurse(v, &children, &crdt_state.t, &mut result, ignore_nonexistent)?;
    }

    Ok(result)
}
