use super::state::{AckmeState, ConnectionState};
use crate::antimatter::crdt_trait::PrunableCrdt;
use crate::antimatter::messages::{Fissure, Message, Patch};
use crate::core::traits::BraidRuntime;
use crate::core::Result;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct AntimatterCrdt<T: PrunableCrdt + Clone> {
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
    pub send_cb: Arc<dyn Fn(Message) + Send + Sync>,
    pub runtime: Arc<dyn BraidRuntime>,
}

impl<T: PrunableCrdt + Clone> AntimatterCrdt<T> {
    pub fn new(
        id: Option<String>,
        crdt: T,
        send_cb: Arc<dyn Fn(Message) + Send + Sync>,
        runtime: Arc<dyn BraidRuntime>,
    ) -> Self {
        Self {
            id: id.unwrap_or_else(|| Uuid::new_v4().to_string()[..12].to_string()),
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
            ackme_current_wait_time: 2000,
            send_cb,
            runtime,
        }
    }

    pub fn receive(&mut self, msg: Message) -> Result<Vec<Patch>> {
        let mut rebased_patches = Vec::new();

        match msg {
            Message::Subscribe { conn, .. } => {
                self.proto_conns.insert(conn.clone());
                (self.send_cb)(Message::Welcome {
                    conn,
                    versions: Vec::new(), // Initial state would be a snapshot if needed
                    fissures: self.fissures.values().cloned().collect(),
                    parents: self.current_version.clone(),
                    peer: Some(self.id.clone()),
                });
            }
            Message::Welcome {
                conn,
                versions,
                fissures,
                ..
            } => {
                self.proto_conns.remove(&conn);
                let seq = self.generate_seq();
                self.conns
                    .insert(conn.clone(), ConnectionState { peer: None, seq });

                for f in fissures {
                    let key = format!("{}:{}:{}", f.a, f.b, f.conn);
                    self.fissures.insert(key, f);
                }

                for v in versions {
                    // In real antimatter, we add all these versions
                    self.add_version(v.version, v.parents, v.patches);
                }
            }
            Message::Update {
                version,
                parents,
                patches,
                ackme,
                ..
            } => {
                rebased_patches = self.add_version(version, parents.clone(), patches);

                if let Some(ackme_id) = ackme {
                    self.process_ackme(ackme_id, parents, None);
                }
            }
            Message::Ack {
                seen,
                ackme,
                versions,
                conn,
                ..
            } => {
                if let Some(ackme_id) = ackme {
                    if let Some(versions) = versions {
                        if seen == "local" {
                            self.process_ackme(ackme_id, versions, Some(conn));
                        } else if seen == "global" {
                            self.add_full_ack_leaves(&ackme_id, Some(&conn));
                        }
                    }
                }
            }
            Message::Fissure {
                fissure, fissures, ..
            } => {
                if let Some(f) = fissure {
                    let key = format!("{}:{}:{}", f.a, f.b, f.conn);
                    self.fissures.insert(key, f);
                }
                if let Some(fs) = fissures {
                    for f in fs {
                        let key = format!("{}:{}:{}", f.a, f.b, f.conn);
                        self.fissures.insert(key, f);
                    }
                }
            }
            _ => {}
        }

        Ok(rebased_patches)
    }

    // =============================================================
    // Public API Methods - Matching JS Antimatter API
    // =============================================================

    pub fn subscribe(&mut self, conn: String) {
        self.proto_conns.insert(conn.clone());
        (self.send_cb)(Message::Subscribe {
            peer: self.id.clone(),
            conn: conn.clone(),
            parents: self.current_version.clone(),
            protocol_version: Some(1),
        });
    }

    pub fn disconnect(&mut self, conn: String, create_fissure: bool) {
        if let Some(state) = self.conns.remove(&conn) {
            if create_fissure {
                if let Some(peer) = state.peer {
                    if let Some(f) = self.create_fissure(&peer, &conn) {
                        let key = format!("{}:{}:{}", self.id, peer, conn);
                        self.fissures.insert(key, f);
                    }
                }
            }
        }
    }

    pub fn update(&mut self, patches: Vec<Patch>) -> String {
        let version = self.generate_random_id();
        self.add_version(version.clone(), self.current_version.clone(), patches);

        if self.conns.len() > 0 {
            let ackme = self.ackme();
            for (c, _) in &self.conns {
                (self.send_cb)(Message::Update {
                    version: version.clone(),
                    parents: self.current_version.clone(),
                    patches: Vec::new(),
                    ackme: Some(ackme.clone()),
                    conn: c.clone(),
                });
            }
        }

        version
    }

    pub fn ackme(&mut self) -> String {
        let id = self.generate_random_id();
        let m = AckmeState {
            id: id.clone(),
            origin: None,
            count: self.conns.len(),
            versions: self.current_version.clone(),
            seq: self.generate_seq(),
            time: self.runtime.now_ms(),
            time2: None,
            orig_count: self.conns.len(),
            real_ackme: true,
            key: format!("{}:{}", self.id, id),
            cancelled: false,
        };

        self.ackmes.insert(id.clone(), m);

        for (c, _) in &self.conns {
            (self.send_cb)(Message::Update {
                version: "".to_string(),
                parents: self.current_version.clone(),
                patches: Vec::new(),
                ackme: Some(id.clone()),
                conn: c.clone(),
            });
        }

        id
    }

    pub fn generate_seq(&mut self) -> u64 {
        self.conn_count += 1;
        self.conn_count
    }

    pub fn generate_random_id(&mut self) -> String {
        let seq = self.generate_seq();
        format!("{}-{}", self.id, seq)
    }

    pub fn create_fissure(&self, peer: &str, conn: &str) -> Option<Fissure> {
        Some(Fissure {
            a: self.id.clone(),
            b: peer.to_string(),
            conn: conn.to_string(),
            versions: self.current_version.clone(),
            time: self.runtime.now_ms(),
            t: Some(self.conn_count),
        })
    }

    pub fn process_ackme(
        &mut self,
        ackme: String,
        versions: HashMap<String, bool>,
        conn: Option<String>,
    ) {
        if let Some(m) = self.ackmes.get_mut(&ackme) {
            if m.count > 0 {
                m.count -= 1;
                self.check_ackme_count(&ackme);
            }
        } else {
            // New ackme request
            let m = AckmeState {
                id: ackme.clone(),
                origin: conn,
                count: self.conns.len().saturating_sub(1),
                versions,
                seq: self.generate_seq(),
                time: self.runtime.now_ms(),
                time2: None,
                orig_count: self.conns.len(),
                real_ackme: false,
                key: ackme.clone(),
                cancelled: false,
            };
            self.ackmes.insert(ackme.clone(), m);
            self.check_ackme_count(&ackme);
        }
    }

    pub fn add_version(
        &mut self,
        version: String,
        parents: HashMap<String, bool>,
        patches: Vec<Patch>,
    ) -> Vec<Patch> {
        if self.t.contains_key(&version) {
            return Vec::new();
        }

        let parent_set: HashSet<String> = parents.keys().cloned().collect();
        self.t.insert(version.clone(), parent_set);

        for p in parents.keys() {
            self.current_version.remove(p);
        }
        self.current_version.insert(version.clone(), true);

        let ps: Vec<String> = parents.keys().cloned().collect();
        if ps.len() == 1 && self.version_groups.contains_key(&ps[0]) {
            let mut group = self.version_groups.get(&ps[0]).unwrap().clone();
            group.push(version.clone());
            self.version_groups.insert(version.clone(), group);
        } else {
            self.version_groups
                .insert(version.clone(), vec![version.clone()]);
        }

        for patch in &patches {
            self.crdt.apply_patch(patch.clone());
        }

        patches
    }

    // =============================================================
    // Delegated Algorithm Methods
    // =============================================================

    pub fn prune(&mut self, just_checking: bool) -> bool {
        super::algorithm::prune::prune(self, just_checking)
    }

    pub fn prune_with_time(&mut self, just_checking: bool, t: u64) -> bool {
        super::algorithm::prune::prune_with_time(self, just_checking, t)
    }

    pub fn ancestors(
        &self,
        versions: &HashMap<String, bool>,
        ignore_nonexistent: bool,
    ) -> Result<HashMap<String, bool>> {
        super::algorithm::bubble::ancestors(self, versions, ignore_nonexistent)
    }

    pub fn descendants(
        &self,
        versions: &HashMap<String, bool>,
        ignore_nonexistent: bool,
    ) -> Result<HashMap<String, bool>> {
        super::algorithm::bubble::descendants(self, versions, ignore_nonexistent)
    }

    pub fn get_child_map(&self) -> HashMap<String, HashSet<String>> {
        super::algorithm::bubble::get_child_map(self)
    }

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

    pub fn check_ackme_count(&mut self, ackme: &str) {
        let m = if let Some(m) = self.ackmes.get(ackme) {
            m
        } else {
            return;
        };

        if m.count == 0 && !m.cancelled {
            let orig_count = m.orig_count;
            let time = m.time;
            let origin = m.origin.clone();
            let versions = m.versions.clone();

            let m_mut = self.ackmes.get_mut(ackme).unwrap();
            m_mut.time2 = Some(self.runtime.now_ms());

            if orig_count > 0 {
                let t = m_mut.time2.unwrap() - time;
                let weight = 0.1;
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
                        unsubscribe: false,
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

        for v in versions.keys() {
            if !self.t.contains_key(v) {
                continue;
            }

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

    pub fn ackme_timeout(&mut self, ackme_id: &str) {
        if let Some(m) = self.ackmes.get_mut(ackme_id) {
            if m.cancelled {
                return;
            }

            let now = self.runtime.now_ms();
            if m.count > 0 && (now - m.time) > self.ackme_current_wait_time {
                tracing::debug!("Ackme {} timed out, count={}", ackme_id, m.count);
                m.cancelled = true;
                self.prune(false);
            }
        }
    }
}
