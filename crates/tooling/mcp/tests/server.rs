//! In-process integration tests: a real MCP client talks to the server over an in-memory
//! duplex pipe and every tool's result is compared with the CLI library's own envelope.

use std::path::PathBuf;

use rmcp::model::{CallToolRequestParams, CallToolResult, ClientConfig, ReadResourceRequestParams};
use rmcp::service::RunningService;
use rmcp::{RoleClient, ServiceExt};
use serde_json::{Value, json};
use syrp_cli::cli::{Cmd, DocsCmd};
#[cfg(feature = "internal")]
use syrp_cli::cli::{ReviewCmd, WpCmd};
use syrp_mcp::SyrpServer;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("repository root")
}

fn server() -> SyrpServer {
    SyrpServer::new(Some(repo_root()))
}

async fn connect() -> RunningService<RoleClient, ClientConfig> {
    let (server_io, client_io) = tokio::io::duplex(1 << 16);
    tokio::spawn(async move {
        let running = server().serve(server_io).await.expect("server starts");
        let _ = running.waiting().await;
    });
    ClientConfig::default()
        .serve(client_io)
        .await
        .expect("client connects")
}

async fn call(
    client: &RunningService<RoleClient, ClientConfig>,
    name: &str,
    args: Value,
) -> CallToolResult {
    let mut params = CallToolRequestParams::new(name.to_string());
    if let Value::Object(map) = args {
        params = params.with_arguments(map);
    }
    client
        .call_tool(params)
        .await
        .expect("tool call succeeds at the protocol level")
}

/// The envelope minus its timing, which is the only field allowed to differ between runs.
fn untimed(mut v: Value) -> Value {
    v.as_object_mut()
        .expect("envelope object")
        .remove("duration_ms");
    v
}

/// The CLI library's own envelope for `cmd`, as `cargo syrp <cmd> --json` would print it.
fn cli_envelope(cmd: Cmd) -> Value {
    untimed(serde_json::to_value(server().envelope(cmd)).unwrap())
}

fn structured(r: &CallToolResult) -> Value {
    let v = r.structured_content.clone().expect("structured content");
    assert_eq!(v["schema"], "syrp.envelope/v1");
    // A failed command is flagged as a tool error, and only then.
    assert_eq!(r.is_error == Some(true), v["ok"] == json!(false), "{v}");
    untimed(v)
}

#[tokio::test]
async fn b1_server_speaks_mcp_over_a_byte_stream_and_identifies_itself() {
    let client = connect().await;
    let info = client.peer_info().expect("server info");
    assert_eq!(
        info.server_info.as_ref().expect("server info").name,
        "syrp-mcp"
    );
    assert!(info.capabilities.tools.is_some());
    assert!(info.capabilities.resources.is_some());
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn b1_binary_is_named_syrp_mcp_and_answers_help() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_syrp-mcp"))
        .arg("--help")
        .output()
        .expect("binary runs");
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("syrp-mcp"));
}

#[tokio::test]
async fn b2_tool_list_matches_the_spec() {
    let client = connect().await;
    let mut names: Vec<String> = client
        .list_all_tools()
        .await
        .unwrap()
        .into_iter()
        .map(|t| {
            assert!(t.output_schema.is_some(), "{} has no output schema", t.name);
            t.name.to_string()
        })
        .collect();
    names.sort();
    let mut expected = vec!["check", "docs_search", "doctor", "explain", "test"];
    if cfg!(feature = "internal") {
        expected.extend(["review_checklist", "wp_ready", "wp_show"]);
    }
    expected.sort();
    assert_eq!(names, expected);
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn b2_tools_return_the_cli_envelope_unchanged() {
    let client = connect().await;
    let cases: Vec<(&str, Value, Cmd)> = vec![
        ("doctor", json!({}), Cmd::Doctor),
        (
            "check",
            json!({"lints_only": true}),
            Cmd::Check { lints_only: true },
        ),
        (
            "explain",
            json!({"code": "SYRP-CLI-0001"}),
            Cmd::Explain {
                code: Some("SYRP-CLI-0001".into()),
                list: false,
            },
        ),
        (
            "explain",
            json!({"list": true}),
            Cmd::Explain {
                code: None,
                list: true,
            },
        ),
        (
            "explain",
            json!({"code": "SYRP-NOPE-9999"}),
            Cmd::Explain {
                code: Some("SYRP-NOPE-9999".into()),
                list: false,
            },
        ),
        (
            "docs_search",
            json!({"query": "error catalogue", "limit": 3}),
            Cmd::Docs(DocsCmd::Search {
                query: vec!["error".into(), "catalogue".into()],
                limit: 3,
            }),
        ),
    ];
    for (name, args, cmd) in cases {
        let got = structured(&call(&client, name, args.clone()).await);
        assert_eq!(got, cli_envelope(cmd), "{name} {args}");
    }
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn b2_test_tool_runs_cargo_test_through_the_cli() {
    let client = connect().await;
    // A package that does not exist keeps this fast and avoids re-entering this test run;
    // the CLI reports it as a failed `test` envelope, which is what the tool must pass on.
    let r = call(&client, "test", json!({"crate": "syrp-no-such-package"})).await;
    let got = structured(&r);
    assert_eq!(
        got,
        cli_envelope(Cmd::Test {
            krate: Some("syrp-no-such-package".into())
        })
    );
    assert_eq!(got["command"], "test");
    assert_eq!(r.is_error, Some(true));
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn b3_error_resource_is_the_explain_envelope() {
    let client = connect().await;
    let templates: Vec<String> = client
        .list_all_resource_templates()
        .await
        .unwrap()
        .into_iter()
        .map(|t| t.uri_template)
        .collect();
    assert!(templates.contains(&"syrp://errors/{code}".to_string()));
    assert_eq!(
        templates.contains(&"syrp://wp/{id}".to_string()),
        cfg!(feature = "internal")
    );

    let uri = "syrp://errors/SYRP-CLI-0001";
    let r = client
        .read_resource(ReadResourceRequestParams::new(uri))
        .await
        .unwrap();
    let rmcp::model::ResourceContents::TextResourceContents {
        text,
        mime_type,
        uri: got_uri,
        ..
    } = &r.contents[0]
    else {
        panic!("expected text contents");
    };
    assert_eq!(got_uri, uri);
    assert_eq!(mime_type.as_deref(), Some("application/json"));
    let v: Value = serde_json::from_str(text).unwrap();
    assert_eq!(
        untimed(v),
        cli_envelope(Cmd::Explain {
            code: Some("SYRP-CLI-0001".into()),
            list: false
        })
    );
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn b3_unknown_resources_are_not_found() {
    let client = connect().await;
    for uri in [
        "syrp://errors/SYRP-NOPE-9999",
        "syrp://nothing/here",
        "syrp://errors/",
    ] {
        let err = client
            .read_resource(ReadResourceRequestParams::new(uri))
            .await;
        assert!(err.is_err(), "{uri} should not resolve");
    }
    client.cancel().await.unwrap();
}

#[cfg(feature = "internal")]
#[tokio::test]
async fn b3_wp_resource_is_the_wp_show_envelope() {
    let client = connect().await;
    let uri = "syrp://wp/WP-0.2-10";
    let r = client
        .read_resource(ReadResourceRequestParams::new(uri))
        .await
        .unwrap();
    let rmcp::model::ResourceContents::TextResourceContents { text, .. } = &r.contents[0] else {
        panic!("expected text contents");
    };
    let v: Value = serde_json::from_str(text).unwrap();
    assert_eq!(
        untimed(v),
        cli_envelope(Cmd::Wp(WpCmd::Show {
            id: "WP-0.2-10".into()
        }))
    );
    client.cancel().await.unwrap();
}

#[test]
fn b4_internal_tools_follow_the_feature_flag() {
    let names = server().tool_names();
    for t in ["wp_show", "wp_ready", "review_checklist"] {
        assert_eq!(
            names.contains(&t.to_string()),
            cfg!(feature = "internal"),
            "{t}"
        );
    }
    assert_eq!(
        syrp_mcp::resource_command("syrp://wp/WP-0.2-10").is_some(),
        cfg!(feature = "internal")
    );
    // The feature is on by default, and this repository builds with defaults.
    let manifest = include_str!("../Cargo.toml");
    assert!(manifest.contains("default = [\"internal\"]"));
}

#[cfg(feature = "internal")]
#[tokio::test]
async fn b4_internal_tools_return_the_cli_envelope_unchanged() {
    let client = connect().await;
    let cases: Vec<(&str, Value, Cmd)> = vec![
        (
            "wp_show",
            json!({"id": "WP-0.2-10"}),
            Cmd::Wp(WpCmd::Show {
                id: "WP-0.2-10".into(),
            }),
        ),
        (
            "wp_show",
            json!({"id": "WP-9.9-99"}),
            Cmd::Wp(WpCmd::Show {
                id: "WP-9.9-99".into(),
            }),
        ),
        (
            "wp_ready",
            json!({}),
            Cmd::Wp(WpCmd::Ready { done: vec![] }),
        ),
        (
            "wp_ready",
            json!({"done": ["WP-0.2-10"]}),
            Cmd::Wp(WpCmd::Ready {
                done: vec!["WP-0.2-10".into()],
            }),
        ),
        (
            "review_checklist",
            json!({}),
            Cmd::Review(ReviewCmd::Checklist),
        ),
    ];
    for (name, args, cmd) in cases {
        let got = structured(&call(&client, name, args.clone()).await);
        assert_eq!(got, cli_envelope(cmd), "{name} {args}");
    }
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn b5_every_listed_tool_is_callable_in_process() {
    let client = connect().await;
    let tools = client.list_all_tools().await.unwrap();
    for t in tools {
        let args = match t.name.as_ref() {
            "check" => json!({"lints_only": true}),
            "test" => json!({"crate": "syrp-no-such-package"}),
            "explain" => json!({"list": true}),
            "docs_search" => json!({"query": "envelope"}),
            "wp_show" => json!({"id": "WP-0.2-10"}),
            _ => json!({}),
        };
        let r = call(&client, &t.name, args).await;
        let v = structured(&r);
        assert!(v["command"].is_string(), "{}: {v}", t.name);
    }
    client.cancel().await.unwrap();
}
