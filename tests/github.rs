mod common;
use common::*;
use raad::{Change, Credentials, Error, FileData, Mount, WriteMode, WriteRequest};
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};
use tiny_http::{Header, Response, Server};

struct Mock {
    url: String,
    requests: Arc<Mutex<Vec<(String, String, Value)>>>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Mock {
    fn new(replies: Vec<(u16, Value)>) -> Self {
        let server = Server::http("127.0.0.1:0").unwrap();
        let url = format!("http://{}/", server.server_addr());
        let requests = Arc::new(Mutex::new(vec![]));
        let received = requests.clone();
        let worker = thread::spawn(move || {
            for (code, body) in replies {
                let Some(mut req) = server.recv_timeout(Duration::from_secs(3)).unwrap() else {
                    break;
                };
                assert_eq!(
                    req.headers()
                        .iter()
                        .find(|h| h.field.equiv("Authorization"))
                        .unwrap()
                        .value
                        .as_str(),
                    "Bearer test-token-never-print"
                );
                let mut bytes = String::new();
                req.as_reader().read_to_string(&mut bytes).unwrap();
                received.lock().unwrap().push((
                    req.method().to_string(),
                    req.url().to_owned(),
                    serde_json::from_str(&bytes).unwrap_or(Value::Null),
                ));
                let mut response = Response::from_string(body.to_string())
                    .with_status_code(code)
                    .with_header(Header::from_bytes("Content-Type", "application/json").unwrap());
                if code == 429 {
                    response.add_header(Header::from_bytes("Retry-After", "60").unwrap());
                }
                req.respond(response).unwrap();
            }
        });
        Self {
            url,
            requests,
            worker: Some(worker),
        }
    }
    fn finish(mut self) -> Vec<(String, String, Value)> {
        self.worker.take().unwrap().join().unwrap();
        Arc::try_unwrap(self.requests)
            .unwrap()
            .into_inner()
            .unwrap()
    }
}
fn request(base: &str) -> WriteRequest {
    WriteRequest {
        expected_base: base.into(),
        title: "Update refunds".into(),
        body: "Review required".into(),
        mode: WriteMode::Proposal,
        changes: vec![
            Change::Put {
                path: "procedures/refunds.md".into(),
                data: FileData::new(b"Refund within 60 days.\n"),
            },
            Change::Delete {
                path: "config.json".into(),
            },
        ],
    }
}
fn pr_replies(base: &str) -> Vec<(u16, Value)> {
    vec![
        (200, json!({"object":{"sha":base}})),
        (200, json!({"tree":{"sha":"base-tree"}})),
        (201, json!({"sha":"new-blob"})),
        (201, json!({"sha":"new-tree"})),
        (
            201,
            json!({"sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}),
        ),
        (200, json!({"object":{"sha":base}})),
        (201, json!({"ref":"refs/heads/raad/aaaaaaaaaaaaaaaaaaaa"})),
        (
            201,
            json!({"html_url":"https://github.com/fixture/private/pull/1"}),
        ),
    ]
}
#[test]
fn private_multi_file_pr_is_one_commit_and_never_activates() {
    let f = Fixture::new();
    let mount = f.remote();
    let base = mount.sync(None).unwrap().active_sha.unwrap();
    let mock = Mock::new(pr_replies(&base));
    let proposal = mount.propose_with_api(request(&base), &mock.url).unwrap();
    assert!(proposal.pull_request_url.unwrap().ends_with("/pull/1"));
    assert_eq!(
        mount.status().unwrap().active_sha.as_deref(),
        Some(base.as_str())
    );
    let calls = mock.finish();
    assert_eq!(calls.len(), 8);
    assert!(calls[0].1.ends_with("/git/ref/heads%2Fmain"));
    assert_eq!(calls[3].2["base_tree"], "base-tree");
    let entries = calls[3].2["tree"].as_array().unwrap();
    assert!(
        entries
            .iter()
            .all(|e| e["path"].as_str().unwrap().starts_with("sops/"))
    );
    assert_eq!(entries[1]["sha"], Value::Null);
    assert_eq!(calls[4].2["parents"], json!([base]));
    assert_eq!(calls[7].2["base"], "main");
    let cache = std::fs::read_to_string(mount.cache_directory().join("state.json")).unwrap();
    assert!(!cache.contains("test-token"));
    assert!(!format!("{:?}", mount).contains("test-token"));
}
#[test]
fn stale_base_rate_limits_and_redaction() {
    let f = Fixture::new();
    let mount = f.remote();
    let base = mount.sync(None).unwrap().active_sha.unwrap();
    let mock = Mock::new(vec![(
        200,
        json!({"object":{"sha":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}}),
    )]);
    assert!(matches!(
        mount.propose_with_api(request(&base), &mock.url),
        Err(Error::Conflict(_))
    ));
    assert_eq!(mock.finish().len(), 1);
    let mock = Mock::new(vec![(429, json!({"message":"test-token-never-print"}))]);
    let error = mount
        .propose_with_api(request(&base), &mock.url)
        .unwrap_err();
    assert!(matches!(error, Error::RateLimit(_)));
    assert!(!error.to_string().contains("test-token"));
    mock.finish();
    let mock = Mock::new(vec![(403, json!({"message":"test-token-never-print"}))]);
    let error = mount
        .propose_with_api(request(&base), &mock.url)
        .unwrap_err();
    assert!(matches!(error, Error::Permission(_)));
    assert!(!error.to_string().contains("test-token"));
    mock.finish();
}
#[test]
fn direct_writes_are_explicit_and_atomic() {
    let f = Fixture::new();
    let mount = f.remote();
    let base = mount.sync(None).unwrap().active_sha.unwrap();
    let mut req = request(&base);
    req.mode = WriteMode::Direct;
    assert!(matches!(
        mount.propose_with_api(req.clone(), "http://127.0.0.1:1/"),
        Err(Error::Permission(_))
    ));
    let mut config = mount.config().clone();
    config.allow_direct = true;
    let mount = Mount::open(
        config,
        f.cache.path(),
        Credentials::token("test-token-never-print"),
    )
    .unwrap();
    let mock = Mock::new(vec![
        (200, json!({"object":{"sha":base}})),
        (
            200,
            json!({"data":{"createCommitOnBranch":{"commit":{"oid":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}}}),
        ),
    ]);
    let result = mount.propose_with_api(req.clone(), &mock.url).unwrap();
    assert!(result.pull_request_url.is_none());
    let calls = mock.finish();
    assert_eq!(calls[1].1, "/graphql");
    assert_eq!(calls[1].2["variables"]["input"]["expectedHeadOid"], base);
    assert_eq!(
        calls[1].2["variables"]["input"]["fileChanges"]["additions"][0]["path"],
        "sops/procedures/refunds.md"
    );
    let mock = Mock::new(vec![
        (200, json!({"object":{"sha":base}})),
        (200, json!({"errors":[{"message":"head changed"}]})),
    ]);
    assert!(matches!(
        mount.propose_with_api(req, &mock.url),
        Err(Error::Conflict(_))
    ));
    mock.finish();
}
#[test]
fn write_validation_and_mount_bounds_precede_mutations() {
    let f = Fixture::new();
    let mount = f.remote();
    let base = mount.sync(None).unwrap().active_sha.unwrap();
    for path in ["../secret.txt", ".git/config", "/tmp/file"] {
        let mock = Mock::new(vec![(200, json!({"object":{"sha":base}}))]);
        let mut req = request(&base);
        req.changes = vec![Change::Put {
            path: path.into(),
            data: FileData::new(b"bad"),
        }];
        assert!(matches!(
            mount.propose_with_api(req, &mock.url),
            Err(Error::InvalidPath(_))
        ));
        assert_eq!(mock.finish().len(), 1);
    }
    let mock = Mock::new(vec![(200, json!({"object":{"sha":base}}))]);
    let mut req = request(&base);
    req.changes = vec![Change::Put {
        path: "config.json".into(),
        data: FileData::new(b"not JSON"),
    }];
    assert!(matches!(
        mount.propose_with_api(req, &mock.url),
        Err(Error::Validation(_))
    ));
    assert_eq!(mock.finish().len(), 1);
}
#[test]
fn app_token_provider_refreshes_without_persisting_tokens() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let f = Fixture::new();
    let remote = f.remote();
    let base = remote.sync(None).unwrap().active_sha.unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let used = count.clone();
    let credentials = Credentials::Provider(Arc::new(move |repository: &str| {
        assert_eq!(repository, "fixture/private");
        used.fetch_add(1, Ordering::SeqCst);
        Ok(Some("test-token-never-print".into()))
    }));
    let mount = Mount::open(remote.config().clone(), f.cache.path(), credentials).unwrap();
    for _ in 0..2 {
        let mock = Mock::new(vec![(
            200,
            json!({"object":{"sha":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}}),
        )]);
        assert!(mount.propose_with_api(request(&base), &mock.url).is_err());
        mock.finish();
    }
    assert_eq!(count.load(Ordering::SeqCst), 2);
}

#[test]
fn authorized_mcp_proposal_uses_same_validated_github_workflow() {
    let f = Fixture::new();
    let mount = f.remote();
    let base = mount.sync(None).unwrap().active_sha.unwrap();
    let mock = Mock::new(pr_replies(&base));
    let mount = mount.with_github_api(&mock.url).unwrap();
    let server = raad::mcp::Server::new(vec![mount]).unwrap();
    let response = server
        .call(
            "raad_propose",
            json!({"mount":"default","request":request(&base)}),
        )
        .unwrap();
    assert_eq!(
        response["pull_request_url"],
        "https://github.com/fixture/private/pull/1"
    );
    assert_eq!(mock.finish().len(), 8);
}
