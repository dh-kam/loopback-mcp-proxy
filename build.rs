use std::fs;
use std::path::Path;

fn main() {
    let dist = Path::new("web/dist");
    fs::create_dir_all(dist).expect("create web/dist");
    let index = dist.join("index.html");
    if !index.exists() {
        fs::write(
            &index,
            "<!doctype html><meta charset=\"utf-8\"><title>loopback-mcp-proxy</title><p>Web UI is not built. Run <code>make web</code>.</p>",
        )
        .expect("write placeholder index");
    }
    println!("cargo:rerun-if-changed=web/dist");
}
