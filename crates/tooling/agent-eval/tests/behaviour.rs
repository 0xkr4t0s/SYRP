//! Spec behaviours B1..B4 of WP-0.2-13. Everything runs offline: the agent is either nobody,
//! the tasks' reference solutions, or a shell command; no model API is called.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use syrp_agent_eval::{Agent, RunConfig, Task, load_tasks, run};
use syrp_error::Diagnostic;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn work(name: &str) -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("agent-eval")
        .join(name)
}

/// The `syrp` function runs this crate's binary, which embeds the Syrp CLI.
fn config(name: &str) -> RunConfig {
    let bin = env!("CARGO_BIN_EXE_syrp-agent-eval").replace('\\', "/");
    RunConfig::new(root(), work(name)).syrp(format!("\"{bin}\" syrp"))
}

/// Writes synthetic tasks (id, verify, timeout) into a fresh directory.
fn synthetic(name: &str, tasks: &[(&str, &str, u64)]) -> PathBuf {
    let dir = work(name).join("tasks");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (id, verify, secs) in tasks {
        let src = format!(
            "id = '{id}'\ntitle = 'T'\nprompt = 'magic prompt'\nsetup = 'true'\n\
             verify = '''{verify}'''\nreference = 'true'\ntimeout_secs = {secs}\n"
        );
        std::fs::write(dir.join(format!("{id}.toml")), src).unwrap();
    }
    dir
}

#[test]
fn b1_ten_tasks_each_have_prompt_setup_and_verifier() {
    // Loading rejects a task with a missing or empty prompt, setup, verify or reference.
    let tasks = load_tasks(&root().join("evals/tasks")).unwrap();
    assert_eq!(tasks.len(), 10);
    assert!(tasks.iter().all(|t| !t.prompt().trim().is_empty()));
    let ids: Vec<&str> = tasks.iter().map(Task::id).collect();
    for want in ["explain-code", "wp-owner-of-crate", "create-crate"] {
        assert!(ids.contains(&want), "{want} missing from {ids:?}");
    }
}

#[test]
fn b1_task_without_verifier_is_rejected() {
    let dir = synthetic("b1-bad", &[]);
    let bad = "id = 'x'\ntitle = 'T'\nprompt = 'p'\nsetup = 'true'\nreference = 'true'\n";
    std::fs::write(dir.join("x.toml"), bad).unwrap();
    let err = load_tasks(&dir).unwrap_err();
    assert_eq!(err.code().as_str(), "SYRP-EVAL-0001");
    assert!(err.to_string().contains("x.toml") && err.to_string().contains("verify"));
}

#[test]
fn b2_reference_solutions_pass_every_task_end_to_end() {
    let tasks = load_tasks(&root().join("evals/tasks")).unwrap();
    let report = run(&tasks, &config("b2-reference").agent(Agent::Reference)).unwrap();
    let json = report.to_json();
    assert_eq!(json["summary"]["passed"], 10, "{json:#}");
    assert_eq!(json["schema"], "syrp.agent-eval/v1");
    assert_eq!(json["agent"], "reference");
    for t in json["tasks"].as_array().unwrap() {
        assert_eq!(t["passed"], true);
        assert!(t["duration_ms"].is_u64());
        let names: Vec<&str> = t["phases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["setup", "agent", "verify"]);
    }
}

#[test]
fn b2_every_verifier_fails_without_an_agent() {
    let tasks = load_tasks(&root().join("evals/tasks")).unwrap();
    let json = run(&tasks, &config("b2-none")).unwrap().to_json();
    assert_eq!(json["summary"]["total"], 10);
    for t in json["tasks"].as_array().unwrap() {
        assert_eq!(t["passed"], false, "{t:#}");
        assert_eq!(t["failed_phase"], "verify", "{t:#}");
        assert_eq!(t["phases"][0]["exit_code"], 0, "{t:#}");
    }
}

#[test]
fn b3_command_template_runs_the_agent_with_substituted_paths() {
    let verify = "grep -q 'magic prompt' seen.md && test \"$(cat task.txt)\" = synth";
    let dir = synthetic("b3-command", &[("synth", verify, 60)]);
    let tasks = load_tasks(&dir).unwrap();
    let agent = Agent::Command(
        "cp {prompt_file} {workspace}/seen.md && echo {task} > {workspace}/task.txt && test -d {root}"
            .into(),
    );
    let report = run(&tasks, &config("b3-command").agent(agent)).unwrap();
    assert_eq!(report.success_rate(), 1.0, "{:#}", report.to_json());
    assert_eq!(report.to_json()["agent"], "command");
}

#[test]
fn b3_agent_past_the_time_limit_fails() {
    let dir = synthetic("b3-timeout", &[("slow", "true", 1)]);
    let tasks = load_tasks(&dir).unwrap();
    let report = run(
        &tasks,
        &config("b3-timeout").agent(Agent::Command("sleep 30".into())),
    )
    .unwrap();
    let t = &report.to_json()["tasks"][0];
    assert_eq!(t["passed"], false);
    assert_eq!(t["failed_phase"], "agent");
    assert_eq!(t["phases"][1]["timed_out"], true);
    assert!(t["duration_ms"].as_u64().unwrap() < 20_000);
}

#[test]
fn b4_summary_reports_success_rate() {
    let dir = synthetic(
        "b4-summary",
        &[
            ("a", "true", 60),
            ("b", "false", 60),
            ("c", "true", 60),
            ("d", "exit 3", 60),
        ],
    );
    let report = run(&load_tasks(&dir).unwrap(), &config("b4-summary")).unwrap();
    assert_eq!(report.to_json()["summary"]["total"], 4);
    assert_eq!(report.success_rate(), 0.5);
    assert_eq!(report.to_json()["summary"]["failed"], 2);
}

#[test]
fn b4_cli_writes_the_kpi_and_gates_on_min_success_rate() {
    let dir = synthetic("b4-cli", &[("a", "true", 60), ("b", "false", 60)]);
    let out = work("b4-cli").join("report.json");
    let eval = |min: &str| {
        Command::new(env!("CARGO_BIN_EXE_syrp-agent-eval"))
            .args(["run", "--agent", "none", "--min-success-rate", min, "--out"])
            .arg(&out)
            .arg("--root")
            .arg(root())
            .arg("--tasks")
            .arg(&dir)
            .arg("--work-dir")
            .arg(work("b4-cli"))
            .output()
            .unwrap()
    };
    let pass = eval("0.5");
    assert_eq!(pass.status.code(), Some(0));
    let printed: Value = serde_json::from_slice(&pass.stdout).unwrap();
    let written: Value = serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
    assert_eq!(printed, written);
    assert_eq!(written["summary"]["success_rate"], 0.5);
    assert_eq!(eval("0.6").status.code(), Some(1));
}
