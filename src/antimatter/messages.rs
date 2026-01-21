use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fissure {
    pub a: String,
    pub b: String,
    pub conn: String,
    pub versions: HashMap<String, bool>,
    pub time: u64,
    #[serde(default)]
    pub t: Option<u64>, // Connection count/logical time
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Patch {
    pub range: String, // Simplified for now, might need structured path
    pub content: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Version {
    pub version: String,
    pub parents: HashMap<String, bool>,
    #[serde(default)]
    pub patches: Vec<Patch>,
    #[serde(default)]
    pub sort_keys: Option<HashMap<String, String>>, // Depending on implementation
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "camelCase")]
pub enum Message {
    Subscribe {
        peer: String,
        conn: String,
        #[serde(default)]
        parents: HashMap<String, bool>,
        #[serde(default)]
        protocol_version: Option<u32>,
    },
    Update {
        version: String,
        parents: HashMap<String, bool>,
        patches: Vec<Patch>,
        conn: String,
        #[serde(default)]
        ackme: Option<String>,
    },
    Ack {
        seen: String, // "local" or "global"
        #[serde(default)]
        version: Option<String>,
        #[serde(default)]
        ackme: Option<String>,
        #[serde(default)]
        versions: Option<HashMap<String, bool>>, // For ackme acks
        conn: String,
        #[serde(default)]
        unsubscribe: bool,
    },
    Fissure {
        fissure: Option<Fissure>,       // Single fissure
        fissures: Option<Vec<Fissure>>, // Multiple fissures
        conn: String,
    },
    Welcome {
        versions: Vec<Version>,
        fissures: Vec<Fissure>,
        #[serde(default)]
        parents: HashMap<String, bool>,
        #[serde(default)]
        peer: Option<String>,
        conn: String,
    },
    Ackme {
        ackme: String,                   // ID of the ackme
        versions: HashMap<String, bool>, // Versions needing ack
        conn: String,
    },
    Unsubscribe {
        conn: String,
    },
}
