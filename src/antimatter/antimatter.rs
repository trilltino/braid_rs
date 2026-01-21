use crate::antimatter::crdt_trait::PrunableCrdt;
use crate::antimatter::messages::{Fissure, Message, Patch};
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use uuid::Uuid;

pub struct AntimatterCrdt<T: PrunableCrdt> {
    pub id: String,

    // Core state
    pub crdt: T,

    // Networking
    pub conns: HashMap<String, ConnectionState>,
    pub proto_conns: HashSet<String>,
    pub conn_count: u64,

    // Algorithm State
    /// The DAG: Version ID -> Set of Parent IDs
    pub t: HashMap<String, HashSet<String>>,

    /// The current frontier versions (leaves of the DAG)
    pub current_version: HashMap<String, bool>,

    pub fissures: HashMap<String, Fissure>,
    pub acked_boundary: HashSet<String>,
    pub ackmes: HashMap<String, AckmeState>,
    pub version_groups: HashMap<String, Vec<String>>,

    // Ackme logic
    pub ackme_map: HashMap<String, HashMap<String, bool>>, // key -> ackme_id -> true
    pub ackme_time_est_1: u64,
    pub ackme_time_est_2: u64,
    pub ackme_current_wait_time: u64,

    // Hooks
    pub send_cb: Box<dyn Fn(Message) + Send + Sync>,
}

pub struct ConnectionState {
    pub peer: Option<String>,
    pub seq: u64,
}

pub struct AckmeState {
    pub id: String,
    pub origin: Option<String>, // Connection ID
    pub count: usize,
    pub versions: HashMap<String, bool>,
    pub seq: u64,
    pub time: u64,
    pub time2: Option<u64>,
    pub orig_count: usize,
    pub real_ackme: bool,
    pub key: String,
    pub cancelled: bool,
}

impl<T: PrunableCrdt> AntimatterCrdt<T> {
    pub fn new(id: Option<String>, crdt: T, send_cb: Box<dyn Fn(Message) + Send + Sync>) -> Self {
        Self {
            id: id.unwrap_or_else(|| Uuid::new_v4().to_string()),
            crdt,
            conns: HashMap::new(),
            proto_conns: HashSet::new(),
            conn_count: 0,

            t: HashMap::new(),
            current_version: HashMap::new(),
            fissures: HashMap::new(),
            acked_boundary: HashSet::new(),
            ackmes: HashMap::new(),
            version_groups: HashMap::new(),
            ackme_map: HashMap::new(),
            ackme_time_est_1: 1000,
            ackme_time_est_2: 1000,
            ackme_current_wait_time: 1000,
            send_cb,
        }
    }

    pub fn receive(&mut self, msg: Message) -> Result<Vec<Patch>, String> {
        let mut rebased_patches = Vec::new();

        match msg {
            Message::Subscribe {
                peer,
                conn,
                parents: _,
                protocol_version,
            } => {
                if let Some(v) = protocol_version {
                    if v != 2 {
                        return Err("wrong protocol version".to_string());
                    }
                }

                if self.conns.contains_key(&conn) {
                    return Err("bad".to_string());
                }

                self.conn_count += 1;
                self.conns.insert(
                    conn.clone(),
                    ConnectionState {
                        peer: Some(peer.clone()),
                        seq: self.conn_count,
                    },
                );

                // Construct welcome message (simplified for now)
                let welcome = Message::Welcome {
                    versions: Vec::new(), // TODO: generate braid
                    fissures: self.fissures.values().cloned().collect(),
                    parents: HashMap::new(), // TODO: get leaves
                    peer: Some(self.id.clone()),
                    conn: conn.clone(),
                };
                (self.send_cb)(welcome);
            }

            Message::Fissure {
                fissure,
                fissures,
                conn: _,
            } => {
                let mut incoming_fissures = Vec::new();
                if let Some(f) = fissure {
                    incoming_fissures.push(f);
                }
                if let Some(fs) = fissures {
                    incoming_fissures.extend(fs);
                }

                for mut f in incoming_fissures {
                    // Update fissure time/conn count
                    f.t = Some(self.conn_count);

                    let key = format!("{}:{}:{}", f.a, f.b, f.conn);
                    if !self.fissures.contains_key(&key) {
                        self.fissures.insert(key.clone(), f.clone());
                        // Propagate logic would go here (fissures_forward)
                    }
                }
            }

            Message::Update {
                version,
                parents,
                patches,
                conn,
                ackme,
            } => {
                if parents.keys().any(|p| !self.t.contains_key(p)) {
                    return Ok(Vec::new());
                }

                // Add version logic
                let _ = self.add_version(version.clone(), parents.clone(), patches.clone());
                // rebased_patches.extend(...)

                // Propagate
                for (c, _) in &self.conns {
                    if *c != conn {
                        (self.send_cb)(Message::Update {
                            version: version.clone(),
                            parents: parents.clone(),
                            patches: patches.clone(),
                            conn: c.clone(),
                            ackme: ackme.clone(),
                        });
                    }
                }

                // Handle atomic ackme logic for updates
                if let Some(ackme_id) = ackme {
                    let mut ackme_versions = HashMap::new();
                    ackme_versions.insert(version.clone(), true);
                    self.process_ackme(ackme_id, ackme_versions, Some(conn));
                }
            }

            Message::Ack {
                seen,
                ackme,
                versions: _,
                conn,
                ..
            } => {
                if seen == "local" {
                    if let Some(ackme_id) = ackme {
                        if let Some(m) = self.ackmes.get_mut(&ackme_id) {
                            if !m.cancelled {
                                if m.count > 0 {
                                    m.count -= 1;
                                }
                            }
                        }
                        self.check_ackme_count(&ackme_id);
                    }
                } else if seen == "global" {
                    if let Some(ackme_id) = ackme {
                        if let Some(m) = self.ackmes.get(&ackme_id) {
                            if !m.cancelled {
                                let t = get_time() - m.time2.unwrap_or(0);
                                let weight = 0.1;
                                self.ackme_time_est_2 = (weight * t as f64
                                    + (1.0 - weight) * self.ackme_time_est_2 as f64)
                                    as u64;

                                self.add_full_ack_leaves(&ackme_id, Some(&conn));
                            }
                        }
                    }
                }
            }

            Message::Ackme {
                ackme,
                versions,
                conn,
            } => {
                self.process_ackme(ackme, versions, Some(conn));
            }

            Message::Welcome { .. } => {
                // TODO: Welcome handler
            }

            Message::Unsubscribe { conn } => {
                if !self.conns.contains_key(&conn) {
                    return Err("bad".to_string());
                }
                (self.send_cb)(Message::Ack {
                    seen: "local".to_string(),
                    conn: conn.clone(),
                    unsubscribe: true,
                    version: None,
                    ackme: None,
                    versions: None,
                });
                self.conns.remove(&conn);
                self.proto_conns.remove(&conn);
            }
        }

        Ok(rebased_patches)
    }

    // =============================================================
    // Public API Methods - Matching JS Antimatter API
    // =============================================================

    /// Register a new connection with id `conn`.
    /// Triggers this antimatter_crdt object to send a `subscribe` message over the given connection.
    ///
    /// Equivalent to JS: `alice_antimatter_crdt.subscribe('connection_to_bob')`
    pub fn subscribe(&mut self, conn: String) {
        self.proto_conns.insert(conn.clone());
        (self.send_cb)(Message::Subscribe {
            peer: self.id.clone(),
            conn,
            parents: self.current_version.clone(),
            protocol_version: Some(2),
        });
    }

    /// Disconnect the given connection, optionally creating a fissure.
    ///
    /// If `create_fissure` is true (default), creates a fissure allowing reconnection.
    /// If false, just removes the connection without fissure.
    ///
    /// Equivalent to JS: `alice_antimatter_crdt.disconnect('connection_to_bob')`
    pub fn disconnect(&mut self, conn: String, create_fissure: bool) {
        if !self.conns.contains_key(&conn) && !self.proto_conns.contains(&conn) {
            return;
        }
        self.proto_conns.remove(&conn);

        let fissure_to_process = if let Some(conn_state) = self.conns.remove(&conn) {
            if create_fissure {
                if let Some(peer) = conn_state.peer {
                    // Build fissure inline instead of calling method
                    if !self.current_version.is_empty() {
                        Some(Fissure {
                            a: self.id.clone(),
                            b: peer,
                            conn: conn.clone(),
                            versions: self.current_version.clone(),
                            time: get_time(),
                            t: Some(self.conn_count),
                        })
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        if let Some(f) = fissure_to_process {
            let _ = self.receive(Message::Fissure {
                fissure: Some(f),
                fissures: None,
                conn: conn.clone(),
            });
        }
    }

    /// Modify this antimatter_crdt object by applying the given patches.
    /// Returns the version ID created.
    ///
    /// Equivalent to JS: `antimatter_crdt.update({range: '.life.meaning', content: 42})`
    pub fn update(&mut self, patches: Vec<Patch>) -> String {
        // Extract all values before calling receive to avoid borrow conflicts
        let seq = self.generate_seq();
        let version = format!("{}@{}", seq, self.id);
        let parents = self.current_version.clone();
        let conn = format!("local-{}", self.id);
        let ackme_id = self.generate_random_id();

        let _ = self.receive(Message::Update {
            version: version.clone(),
            parents,
            patches,
            conn,
            ackme: Some(ackme_id),
        });

        version
    }

    /// Initiate sending an ackme message to try and establish whether certain versions can be pruned.
    ///
    /// Equivalent to JS: `antimatter_crdt.ackme()`
    pub fn ackme(&mut self) -> String {
        // Extract values first to avoid borrow conflicts
        let mut versions = self.current_version.clone();

        // Expand version groups
        let version_keys: Vec<String> = self.current_version.keys().cloned().collect();
        for v in version_keys {
            if let Some(group) = self.version_groups.get(&v) {
                for gv in group {
                    versions.insert(gv.clone(), true);
                }
            }
        }

        let ackme_id = self.generate_random_id();
        let conn = format!("local-{}", self.id);

        let _ = self.receive(Message::Ackme {
            ackme: ackme_id.clone(),
            versions,
            conn,
        });

        ackme_id
    }

    /// Generate next sequence number.
    fn generate_seq(&mut self) -> u64 {
        let seq = self.conn_count;
        self.conn_count += 1;
        seq
    }

    /// Generate a random ID (simplified - uses conn_count + id).
    fn generate_random_id(&mut self) -> String {
        let id = format!("{}:{}", self.conn_count, self.id);
        self.conn_count += 1;
        id
    }

    /// Create a fissure for a disconnected connection.
    fn create_fissure(&self, peer: &str, conn: &str) -> Option<Fissure> {
        if self.current_version.is_empty() {
            return None;
        }

        Some(Fissure {
            a: self.id.clone(),
            b: peer.to_string(),
            conn: conn.to_string(),
            versions: self.current_version.clone(),
            time: get_time(),
            t: Some(self.conn_count),
        })
    }

    // Helper to process ackme logic shared by Update and Ackme messages
    fn process_ackme(
        &mut self,
        ackme: String,
        versions: HashMap<String, bool>,
        conn: Option<String>,
    ) {
        if !versions.keys().all(|v| self.t.contains_key(v)) {
            return;
        }

        if !self.ackmes.contains_key(&ackme) {
            let count = self.conns.len() - if conn.is_some() { 1 } else { 0 };
            let m = AckmeState {
                id: ackme.clone(),
                origin: conn.clone(),
                count,
                versions: versions.clone(),
                seq: self.conn_count,
                time: get_time(),
                time2: None,
                orig_count: count,
                real_ackme: true, // simplified
                key: format!("{:?}", versions.keys().collect::<Vec<_>>()), // Simplified key
                cancelled: false,
            };
            self.ackmes.insert(ackme.clone(), m);

            // Propagate
            for (c, _) in &self.conns {
                if Some(c.clone()) != conn {
                    (self.send_cb)(Message::Ackme {
                        ackme: ackme.clone(),
                        versions: versions.clone(),
                        conn: c.clone(),
                    });
                }
            }
        }

        // Check seq logic
        let mut send_local_ack = false;
        if let Some(conn_id) = &conn {
            if let Some(conn_state) = self.conns.get(conn_id) {
                if let Some(m) = self.ackmes.get(&ackme) {
                    if m.seq < conn_state.seq {
                        send_local_ack = true;
                    }
                }
            }
        }

        if send_local_ack {
            (self.send_cb)(Message::Ack {
                seen: "local".to_string(),
                ackme: Some(ackme.clone()),
                versions: Some(versions),
                conn: conn.unwrap(),
                version: None,
                unsubscribe: false,
            });
            return;
        }

        if let Some(m) = self.ackmes.get_mut(&ackme) {
            if m.count > 0 {
                m.count -= 1;
            }
        }
        self.check_ackme_count(&ackme);
    }

    pub fn add_version(
        &mut self,
        version: String,
        parents: HashMap<String, bool>,
        patches: Vec<Patch>,
    ) -> Vec<Patch> {
        if self.t.contains_key(&version) {
            return Vec::new(); // Already seen
        }

        // Add to DAG
        let parent_set: HashSet<String> = parents.keys().cloned().collect();
        self.t.insert(version.clone(), parent_set);

        // Update frontier (current_version)
        // Remove ancestors of the new version from the frontier
        for p in parents.keys() {
            self.current_version.remove(p);
        }
        // Add new version to frontier
        self.current_version.insert(version.clone(), true);

        // Add to version group
        let ps: Vec<String> = parents.keys().cloned().collect();
        if ps.len() == 1 && self.version_groups.contains_key(&ps[0]) {
            let mut group = self.version_groups.get(&ps[0]).unwrap().clone();
            group.push(version.clone());
            self.version_groups.insert(version.clone(), group);
            // Optional: update all members of the group to point to the new group vector
            // But JS just shares the object. In Rust we'd need Arc<RwLock<...>>.
            // For 1:1 parity with current struct, we'll just store the list.
        } else {
            self.version_groups
                .insert(version.clone(), vec![version.clone()]);
        }

        // Apply patches to underlying CRDT
        // In the JS version, there's complex "rebasing" logic here because
        // it supports generic JSON trees. For now, we delegate to the trait.
        // The trait is expected to handle the "rebasing" or causal ordering if needed,
        // or we assume patches are already in order for this simple port.
        for patch in &patches {
            self.crdt.apply_patch(patch.clone());
        }

        patches
    }

    pub fn prune(&mut self, just_checking: bool) -> bool {
        self.prune_with_time(just_checking, u64::MAX)
    }

    /// Full prune with bubble logic matching JS implementation.
    pub fn prune_with_time(&mut self, just_checking: bool, t: u64) -> bool {
        // 1. Prune fissures that match (both sides received)
        let mut keys_to_delete = Vec::new();
        let fissures_snapshot = self.fissures.clone();

        for (key, f) in &fissures_snapshot {
            let other_key = format!("{}:{}:{}", f.b, f.a, f.conn);
            if let Some(_other) = fissures_snapshot.get(&other_key) {
                // Check time condition (simplified - using conn_count as proxy)
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
                self.fissures.remove(&k);
            }
        }

        // 2. Calculate restricted versions
        let mut restricted: HashMap<String, bool> = HashMap::new();
        for f in self.fissures.values() {
            for v in f.versions.keys() {
                restricted.insert(v.clone(), true);
            }
        }

        // Add unacked versions to restricted
        if !just_checking {
            if let Ok(acked) = self.ancestors(
                &self
                    .acked_boundary
                    .iter()
                    .map(|v| (v.clone(), true))
                    .collect(),
                true,
            ) {
                for v in self.t.keys() {
                    if !acked.contains_key(v) {
                        restricted.insert(v.clone(), true);
                    }
                }
            }
        }

        // 3. Bubble identification
        let children = self.get_child_map();
        let (parent_sets, child_sets) = self.get_parent_and_child_sets(&children);

        let mut to_bubble: HashMap<String, (String, String)> = HashMap::new();
        let mut visited: HashSet<String> = HashSet::new();

        // Mark bubble recursively
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

        // Find bubbles starting from current_version
        for v in self.current_version.keys() {
            if visited.contains(v) {
                continue;
            }

            // Check if this version is part of a parent set
            if let Some(parent_set) = parent_sets.get(v) {
                if !parent_set.done {
                    let bottom = parent_set.members.clone();
                    if let Some(top) =
                        self.find_one_bubble(&bottom, &children, &child_sets, Some(&restricted))
                    {
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
                            mark_bubble(v, bubble.clone(), &mut to_bubble, &self.t);
                        }
                    }
                }
            } else {
                // Try finding a bubble from this single version
                let bottom: HashMap<String, bool> = [(v.clone(), true)].into_iter().collect();
                if let Some(top) =
                    self.find_one_bubble(&bottom, &children, &child_sets, Some(&restricted))
                {
                    if !top.contains_key(v) {
                        if just_checking {
                            return true;
                        }
                        let bubble = (v.clone(), top.keys().next().cloned().unwrap_or_default());
                        for vv in top.keys() {
                            to_bubble.insert(vv.clone(), bubble.clone());
                        }
                        mark_bubble(v, bubble, &mut to_bubble, &self.t);
                    }
                }
            }
            visited.insert(v.clone());
        }

        if just_checking {
            return false;
        }

        // Apply bubbles to version graph
        self.apply_bubbles(&to_bubble);

        false
    }

    /// Get parent and child sets for bubble detection.
    fn get_parent_and_child_sets(
        &self,
        children: &HashMap<String, HashSet<String>>,
    ) -> (HashMap<String, ParentSet>, HashMap<String, ChildSet>) {
        let mut parent_sets: HashMap<String, ParentSet> = HashMap::new();
        let mut child_sets: HashMap<String, ChildSet> = HashMap::new();
        let mut done: HashSet<String> = HashSet::new();

        // Add current_version as a parent set
        if self.current_version.len() >= 2 {
            let members: HashMap<String, bool> = self.current_version.clone();
            let parent_set = ParentSet {
                members: members.clone(),
                done: false,
            };
            for v in self.current_version.keys() {
                parent_sets.insert(v.clone(), parent_set.clone());
                done.insert(v.clone());
            }
        }

        // Find other parent/child sets
        for v in self.t.keys() {
            if done.contains(v) {
                continue;
            }
            done.insert(v.clone());

            if let Some(child_set) = children.get(v) {
                if child_set.len() >= 2 {
                    // Check if all children have same parents
                    let first_child = child_set.iter().next();
                    if let Some(first_child) = first_child {
                        if let Some(first_parent_set) = self.t.get(first_child) {
                            let all_same = child_set.iter().all(|c| {
                                self.t.get(c).map_or(false, |ps| {
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

    /// Find a single bubble in the DAG.
    fn find_one_bubble(
        &self,
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
            if !self.t.contains_key(&cur) {
                if restricted.is_none() {
                    return None; // Error case
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

            // Check if all children are seen
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

            // Add parents to expecting
            if let Some(parents) = self.t.get(&cur) {
                for p in parents {
                    expecting.insert(p.clone(), true);
                    queue.push(p.clone());
                }
            }

            // Check child_sets
            if let Some(cs) = child_sets.get(&cur) {
                if cs.members.keys().all(|v| seen.contains(v)) {
                    let expecting_keys: HashSet<_> = expecting.keys().cloned().collect();
                    if let Some(parents) = self.t.get(&cur) {
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

    /// Apply bubbles to version graph (compress history).
    fn apply_bubbles(&mut self, to_bubble: &HashMap<String, (String, String)>) {
        if to_bubble.is_empty() {
            return;
        }

        // Rename versions in T
        let old_t = std::mem::take(&mut self.t);
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
                        // Parent becomes the top of the bubble
                        top.clone()
                    } else {
                        p.clone()
                    }
                })
                .collect();

            self.t
                .entry(new_v)
                .or_insert_with(HashSet::new)
                .extend(new_parents);
        }

        // Update current_version
        let old_cv = std::mem::take(&mut self.current_version);
        for (v, _) in old_cv {
            let new_v = if let Some((bottom, _)) = to_bubble.get(&v) {
                bottom.clone()
            } else {
                v
            };
            self.current_version.insert(new_v, true);
        }

        // Update acked_boundary
        let old_ab: Vec<String> = self.acked_boundary.iter().cloned().collect();
        self.acked_boundary.clear();
        for v in old_ab {
            let new_v = if let Some((bottom, _)) = to_bubble.get(&v) {
                bottom.clone()
            } else {
                v
            };
            self.acked_boundary.insert(new_v);
        }

        // Update version_groups
        for (v, (bottom, _)) in to_bubble {
            if v != bottom {
                self.version_groups
                    .entry(bottom.clone())
                    .or_insert_with(Vec::new)
                    .push(v.clone());
            }
        }

        // Prune CRDT metadata
        for (v, (bottom, _)) in to_bubble {
            if v != bottom {
                self.crdt.prune(v);
            }
        }
    }

    pub fn check_ackme_count(&mut self, ackme: &str) {
        let m = if let Some(m) = self.ackmes.get(ackme) {
            m
        } else {
            return;
        };

        if m.count == 0 && !m.cancelled {
            // Need to update state, so we get mutable access again, logic slightly split
            // To avoid lifetime issues with holding 'm', we copy what we need
            let orig_count = m.orig_count;
            let time = m.time;
            let origin = m.origin.clone();
            let versions = m.versions.clone(); // potentially expensive clone

            // Re-borrow mutably
            let m_mut = self.ackmes.get_mut(ackme).unwrap();
            m_mut.time2 = Some(get_time());

            if orig_count > 0 {
                let t = m_mut.time2.unwrap() - time;
                let weight = 0.1;
                // Casting to f64 for calculation roughly
                self.ackme_time_est_1 =
                    (weight * t as f64 + (1.0 - weight) * self.ackme_time_est_1 as f64) as u64;
            }

            if let Some(origin_conn) = origin {
                if self.conns.contains_key(&origin_conn) {
                    (self.send_cb)(Message::Ack {
                        seen: "local".to_string(),
                        ackme: Some(ackme.to_string()),
                        versions: Some(versions),
                        conn: origin_conn,
                        version: None,
                        unsubscribe: false, // Defaults
                    });
                }
            } else {
                self.add_full_ack_leaves(ackme, None);
            }
        }
    }

    pub fn add_full_ack_leaves(&mut self, ackme: &str, ignoring_conn: Option<&str>) {
        if let Some(m) = self.ackmes.get_mut(ackme) {
            if m.cancelled {
                return;
            }
            m.cancelled = true;
        } else {
            return;
        }

        // Copy needed data to avoid immutable borrow during send/mutation
        let m = self.ackmes.get(ackme).unwrap();
        let m_seq = m.seq;
        let versions = m.versions.clone();
        let ackme_str = ackme.to_string();

        for (c, cc) in &self.conns {
            if Some(c.as_str()) != ignoring_conn && cc.seq <= m_seq {
                (self.send_cb)(Message::Ack {
                    seen: "global".to_string(),
                    ackme: Some(ackme_str.clone()),
                    versions: Some(versions.clone()),
                    conn: c.clone(),
                    version: None,
                    unsubscribe: false,
                });
            }
        }

        // Update acked_boundary
        for v in versions.keys() {
            if !self.t.contains_key(v) {
                continue;
            }

            // Remove ancestors from boundary
            let mut visited = HashSet::new();
            let mut stack = vec![v.clone()];

            while let Some(curr) = stack.pop() {
                if visited.contains(&curr) {
                    continue;
                }
                visited.insert(curr.clone());

                self.acked_boundary.remove(&curr);

                if let Some(parents) = self.t.get(&curr) {
                    for p in parents {
                        stack.push(p.clone());
                    }
                }
            }

            self.acked_boundary.insert(v.clone());
        }

        self.prune(false);
    }

    pub fn ancestors(
        &self,
        versions: &HashMap<String, bool>,
        ignore_nonexistent: bool,
    ) -> Result<HashMap<String, bool>, String> {
        let mut result = HashMap::new();

        fn recurse(
            v: &str,
            t: &HashMap<String, HashSet<String>>,
            result: &mut HashMap<String, bool>,
            ignore_nonexistent: bool,
        ) -> Result<(), String> {
            if result.contains_key(v) {
                return Ok(());
            }
            if !t.contains_key(v) {
                if ignore_nonexistent {
                    return Ok(());
                }
                return Err(format!("The version {} does not exist", v));
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
            recurse(v, &self.t, &mut result, ignore_nonexistent)?;
        }

        Ok(result)
    }

    /// Helper to get descendants of a set of versions
    pub fn descendants(
        &self,
        versions: &HashMap<String, bool>,
        ignore_nonexistent: bool,
    ) -> Result<HashMap<String, bool>, String> {
        let children = self.get_child_map();
        let mut result = HashMap::new();

        fn recurse(
            v: &str,
            children: &HashMap<String, HashSet<String>>,
            t: &HashMap<String, HashSet<String>>,
            result: &mut HashMap<String, bool>,
            ignore_nonexistent: bool,
        ) -> Result<(), String> {
            if result.contains_key(v) {
                return Ok(());
            }
            if !t.contains_key(v) {
                if ignore_nonexistent {
                    return Ok(());
                }
                return Err(format!("The version {} does not exist", v));
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
            recurse(v, &children, &self.t, &mut result, ignore_nonexistent)?;
        }

        Ok(result)
    }

    /// Helper to get the child map (inverse of T)
    pub fn get_child_map(&self) -> HashMap<String, HashSet<String>> {
        let mut children = HashMap::new();
        for (v, parents) in &self.t {
            for parent in parents {
                children
                    .entry(parent.clone())
                    .or_insert_with(HashSet::new)
                    .insert(v.clone());
            }
        }
        children
    }

    /// Helper to get leaves of a version set
    pub fn get_leaves(&self, versions: &HashMap<String, bool>) -> HashMap<String, bool> {
        let mut leaves = versions.clone();
        for v in versions.keys() {
            if let Some(parents) = self.t.get(v) {
                for p in parents {
                    leaves.remove(p);
                }
            }
        }
        leaves
    }

    /// Timeout handler for ackme requests.
    /// Matches JS `ackme_timeout()` from antimatter.js.
    pub fn ackme_timeout(&mut self, ackme_id: &str) {
        if let Some(m) = self.ackmes.get_mut(ackme_id) {
            if m.cancelled {
                return;
            }

            let now = get_time();
            if m.count > 0 && (now - m.time) > self.ackme_current_wait_time {
                tracing::debug!("Ackme {} timed out, count={}", ackme_id, m.count);

                // If it's a real ackme, we might want to fissure or re-ackme.
                // JS: if (ackme.real_ackme) self.fissure(...)

                // For now, mark as cancelled to stop further checks
                m.cancelled = true;

                // Prune anyway to see if we can make progress
                self.prune(false);
            }
        }
    }
}

fn get_time() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

/// Helper struct for parent sets in bubble detection.
#[derive(Clone)]
struct ParentSet {
    members: HashMap<String, bool>,
    done: bool,
}

/// Helper struct for child sets in bubble detection.
#[derive(Clone)]
struct ChildSet {
    members: HashMap<String, bool>,
}
