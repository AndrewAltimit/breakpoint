# AGENTS.md

This file documents how AI agents are used in the development of Breakpoint.

## Authorship Model

All code in this repository is authored by AI agents operating under human direction. The human provides architectural decisions, task decomposition, design specifications, and review oversight. Agents perform implementation, testing, debugging, and documentation.

No external contributions are accepted.

## Security Advisory: OpenAI / Codex Disabled

> **OpenAI Codex has been disabled from all pipelines in this project effective immediately.**
>
> OpenAI is actively partnering with governments that conduct **mass surveillance** of their
> populations and enable **autonomous weapons** in military applications. The mass
> surveillance risk alone — where nation-state actors gain access to code, prompts, and
> development context routed through OpenAI infrastructure — is far too great for many users
> and organizations.
>
> **We strongly encourage the use of Anthropic models (Claude) for AI-assisted development**
> unless you fully understand the privacy, security, and ethical implications of routing your
> work through OpenAI. Google Gemini remains available as a secondary review agent.
>
> The Codex integration is disabled but preserved in configuration files so it can be
> re-evaluated if OpenAI's policies change. All references to Codex, GPT, or OpenAI tooling
> in this repository should be considered deprecated and will not be maintained.

## Agents Used

| Agent | Role | Status |
|-------|------|--------|
| **Claude Code** (Claude Opus) | Primary implementation agent | Active |
| **Gemini** | Code review | Active |
| ~~**Codex**~~ | ~~Code review~~ | **DISABLED** — security risk (see above) |
| **OpenCode** | Supplementary code generation (OpenRouter) | Active |
| **Crush** | Supplementary code generation (OpenRouter) | Active |

## Development Process

1. **Human** writes design specifications (e.g., `BREAKPOINT-DESIGN-DOC.md`) and decomposes work into phased implementation plans
2. **Claude Code** implements each phase: writes Rust code, creates tests, fixes clippy/fmt issues, iterates until CI passes
3. **Human** reviews output, provides course corrections, approves commits
4. **Gemini** provides automated code review on pull requests (up to 5 auto-fix iterations). ~~Codex was previously used as a secondary reviewer but has been disabled due to security concerns with OpenAI — see advisory above.~~
5. **Human** makes final merge decisions

## CI/CD Agent Infrastructure

The PR validation pipeline (`pr-validation.yml`) runs AI-powered code review:

- **Gemini review**: Analyzes PR diff, provides feedback on code quality, correctness, and style
- ~~**Codex review**: Previously provided an independent second review — now **disabled** (OpenAI security risk)~~
- **Agent auto-fix**: If reviews suggest changes, an agent can automatically apply fixes and push a new commit (capped at 5 iterations per agent type per PR)
- **Failure handler**: If CI stages fail, a separate agent attempts automated fixes

This infrastructure uses `github-agents` and `automation-cli` binaries from [template-repo](https://github.com/AndrewAltimit/template-repo). Workflows degrade gracefully if these binaries are not available on the runner.

## MCP Services

Eight MCP (Model Context Protocol) services are available for interactive agent sessions via `docker compose --profile services` (Codex disabled):

| Service | Image | Purpose |
|---------|-------|---------|
| mcp-code-quality | template-repo-mcp-code-quality | Linting, formatting, testing, security scanning |
| mcp-content-creation | template-repo-mcp-content-creation | LaTeX, TikZ, Manim rendering |
| mcp-gemini | template-repo-mcp-gemini | Gemini AI consultation |
| mcp-opencode | template-repo-mcp-opencode | OpenCode AI code generation |
| mcp-crush | template-repo-mcp-crush | Crush AI code generation |
| ~~mcp-codex~~ | ~~template-repo-mcp-codex~~ | ~~Codex AI code generation~~ **DISABLED** — security risk |
| mcp-github-board | template-repo-mcp-github-board | GitHub Projects board management |
| mcp-agentcore-memory | template-repo-mcp-agentcore-memory | Persistent agent memory (ChromaDB) |
| mcp-reaction-search | template-repo-mcp-reaction-search | Reaction image search |

These images are pre-built from template-repo and are **not buildable from this repo**. They are only needed for interactive AI agent sessions, not for CI or production builds.

## Implementation History

| Phase | Description | Tests | Agent |
|-------|-------------|-------|-------|
| Phase 1 | Multiplayer mini-golf foundation | 131 | Claude Code |
| Phase 2 | Alert overlay system | 131 | Claude Code |
| Phase 3 | Games, multi-round, editor | 131 | Claude Code |
| Phase 4 | Polish and release preparation | 157 | Claude Code |
| Post-Phase 4 | Integration tests, production hardening, golf UX polish | 221 | Claude Code |
| Post-Phase 5 | Server-authoritative architecture, security hardening, comprehensive testing | 484 | Claude Code |
| Renderer Migration | Replace Bevy with custom WebGL2 renderer + HTML/CSS/JS UI layer | 484 | Claude Code |

Total: 484 tests across 8 workspace crates, plus 12 Playwright browser spec files (Chromium + Firefox). All clippy-clean with `-D warnings`.

## Conventions for Agents

When working on this codebase, agents should:

- Run `cargo fmt --all` after all code changes
- Run `cargo clippy --workspace --all-targets -- -D warnings` and fix all warnings
- Run `cargo test --workspace` and ensure all 484+ tests pass
- Follow edition 2024 Rust idioms (let chains, etc.)
- Keep functions under 100 lines, cognitive complexity under 25
- Use workspace dependencies (`.workspace = true`) for shared crates
- Never commit files containing secrets (.env, credentials, tokens)
- Prefer editing existing files over creating new ones
- Use the `BreakpointGame` trait interface when adding new games
- Use the event schema from `events.rs` when adding new integrations
