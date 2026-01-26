use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct ConnectionState {
    pub peer: Option<String>,
    pub seq: u64,
}

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug)]
pub struct ParentSet {
    pub members: HashMap<String, bool>,
    pub done: bool,
}

#[derive(Clone, Debug)]
pub struct ChildSet {
    pub members: HashMap<String, bool>,
}
