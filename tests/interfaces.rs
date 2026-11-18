mod common;
use common::*;
use raad::{Error, mcp::Server};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
};

#[test]
fn cli_zero_config_reads_and_committed_only_behavior() {
    let f = Fixture::new();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_raad"))
            .current_dir(f.repo.path())
            .env("RAAD_CACHE_DIR", f.cache.path())
            .args(args)
            .output()
            .unwrap()
    };
    let output = run(&["list", "sops/", "--json"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let list: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(list["paths"].as_array().unwrap().len(), 2);
    let output = run(&["read", "sops/procedures/refunds.md"]);
    assert_eq!(output.stdout, b"Refund within 30 days.\n");
    assert!(
        run(&["history", "sops/procedures/refunds.md", "--json"])
            .status
            .success()
    );
    assert!(run(&["validate"]).status.success());
    let output = run(&["read", "../secret", "--json"]);
    assert!(!output.status.success());
    let subdir = Command::new(env!("CARGO_BIN_EXE_raad"))
        .current_dir(f.repo.path().join("sops"))
        .env("RAAD_CACHE_DIR", f.cache.path())
        .args(["read", "config.json"])
        .output()
        .unwrap();
    assert!(subdir.status.success());
    assert!(run(&["status", "--json"]).status.success());
    assert!(run(&["clear-cache"]).status.success());
}
#[test]
fn mcp_allowlist_readonly_and_no_direct_commits() {
    let f = Fixture::new();
    f.mount.sync(None).unwrap();
    let server = Server::new(vec![f.mount.clone()]).unwrap();
    assert_eq!(
        server.call("raad_list_mounts", json!({})).unwrap()["mounts"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(
        !server
            .tools()
            .iter()
            .any(|tool| tool.name == "raad_propose")
    );
    assert!(
        server
            .call("raad_read", json!({"mount":"default","path":"config.json"}))
            .is_ok()
    );
    assert!(matches!(
        server.call("raad_read", json!({"mount":"private-other","path":"file"})),
        Err(Error::Permission(_))
    ));
    assert!(
        server
            .call(
                "raad_read",
                json!({"mount":"default","path":"../secret.txt"})
            )
            .is_err()
    );
    assert!(
        server
            .call(
                "raad_read",
                json!({"mount":"default","path":"config.json","repo":"unapproved/repo"})
            )
            .is_err()
    );
    let base = f.mount.status().unwrap().active_sha.unwrap();
    let req = json!({"mount":"default","request":{"expected_base":base,"title":"change","changes":[{"operation":"delete","path":"config.json"}]}});
    assert!(matches!(
        server.call("raad_propose", req.clone()),
        Err(Error::Permission(_))
    ));
    let mut direct = req;
    direct["request"]["mode"] = json!("direct");
    assert!(matches!(
        server.call("raad_propose", direct),
        Err(Error::Permission(_))
    ));
    let remote = f.remote();
    remote.sync(None).unwrap();
    let server = Server::new(vec![remote]).unwrap();
    assert!(
        server
            .tools()
            .iter()
            .any(|tool| tool.name == "raad_propose")
    );
}
#[test]
fn stdio_mcp_initialize_discover_read_and_reject_arbitrary_mount() {
    let f = Fixture::new();
    let mut child = Command::new(env!("CARGO_BIN_EXE_raad"))
        .current_dir(f.repo.path())
        .env("RAAD_CACHE_DIR", f.cache.path())
        .args(["mcp", "--root", "sops"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    fn exchange(input: &mut impl Write, output: &mut impl BufRead, request: Value) -> Value {
        writeln!(input, "{request}").unwrap();
        input.flush().unwrap();
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    }
    let init = exchange(
        &mut stdin,
        &mut stdout,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"raad-test","version":"1"}}}),
    );
    assert!(init.get("result").is_some(), "{init}");
    writeln!(
        stdin,
        "{}",
        json!({"jsonrpc":"2.0","method":"notifications/initialized"})
    )
    .unwrap();
    let tools = exchange(
        &mut stdin,
        &mut stdout,
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
    );
    assert_eq!(tools["result"]["tools"].as_array().unwrap().len(), 6);
    let read = exchange(
        &mut stdin,
        &mut stdout,
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"raad_read","arguments":{"mount":"default","path":"config.json"}}}),
    );
    assert_eq!(read["result"]["isError"], false, "{read}");
    let text: Value =
        serde_json::from_str(read["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert!(text["provenance"]["commit_sha"].as_str().unwrap().len() == 40);
    let denied = exchange(
        &mut stdin,
        &mut stdout,
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"raad_read","arguments":{"mount":"other","path":"file"}}}),
    );
    assert_eq!(denied["result"]["isError"], true);
    drop(stdin);
    let _ = child.kill();
    child.wait().unwrap();
}
