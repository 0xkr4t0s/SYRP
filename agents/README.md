# agents/

Prompts for the pipeline's LLM roles. Workflows in `.github/workflows/` fill the `{{...}}`
placeholders and pass the result to Claude Code. The deterministic parts (scheduling, merge
gates, review validation) are `cargo auricle conductor`, `pr gates` and `review validate`.

| Role | Prompt | Started by |
|---|---|---|
| Implementer | `roles/implementer.md` | `conductor.yml` |
| Spec room | `roles/spec-room.md` | `conductor.yml` |
| Reviewer | `roles/reviewer.md` | `reviewer.yml` |
| Adversarial verifier | `roles/adversarial.md` | `reviewer.yml` |
| PR manager | `roles/pr-manager.md` | `pr-manager.yml` |

`render.sh` fills a template: `agents/render.sh roles/implementer.md WP=WP-0.1-03 ISSUE=4 ...`.
