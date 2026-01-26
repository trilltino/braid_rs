use napi_derive::napi;

#[napi]
pub fn hello_braid() -> String {
    "Hello from Braid Rust!".to_string()
}

// We will expose BraidClient and other structs here
