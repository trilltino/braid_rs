use braid_rs::antimatter::json_crdt::JsonCrdt;
use braid_rs::antimatter::{messages::Message, AntimatterCrdt};
use std::sync::Mutex;

#[cxx::bridge]
mod ffi {
    extern "Rust" {
        type BraidClient;

        fn new_client(name: &str) -> Box<BraidClient>;
        fn get_peer_name(&self) -> String;
        fn add_version(&self, version: &str, parents: Vec<String>, patches: Vec<String>) -> String;
    }
}

pub struct BraidClient {
    // We wrap in Mutex because C++ might call from multiple threads or we need interior mutability
    // for methods that take &self but modify state (cxx allows &mut self, but Mutex is safer for shared access)
    inner: Mutex<AntimatterCrdt<JsonCrdt>>,
}

fn new_client(name: &str) -> Box<BraidClient> {
    // Define a simple callback that prints messages (mock network)
    let send_cb = Box::new(|msg: Message| {
        println!("C++ Client sending message: {:?}", msg);
    });

    let crdt = JsonCrdt::new(name);
    let antimatter = AntimatterCrdt::new(Some(name.to_string()), crdt, send_cb);

    Box::new(BraidClient {
        inner: Mutex::new(antimatter),
    })
}

impl BraidClient {
    fn get_peer_name(&self) -> String {
        let lock = self.inner.lock().unwrap();
        lock.id.clone()
    }

    fn add_version(&self, version: &str, parents: Vec<String>, patches: Vec<String>) -> String {
        // Mock implementation to verify connectivity
        // In reality, we'd parse patches and call antimatter.receive or add_version
        // For now, let's just return what we got to prove the link works
        format!("Added version {} with {} parents", version, parents.len())
    }
}
