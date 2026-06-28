# Prompt: Generate `.agents/risks.yaml`

```text
You are generating or updating `.agents/risks.yaml`.

Target file:
`.agents/risks.yaml`

Scope:
Edit only `.agents/risks.yaml`.

Schema:
`.agents/schema-v1/risks.schema.json`

Purpose:
Declare high-risk repository surfaces and the safeguards agents must apply
before changing them.

Required behavior:
- Run `git status --short` first.
- Preserve unrelated user changes.
- Inspect path names, security docs, privacy docs, release docs, deploy scripts,
  auth/permission code, data export code, migrations, observability code,
  dependency manifests, `.agents/contracts.yaml`, and `.agents/commands.yaml`.
- Do not exaggerate or invent business risk. If a surface looks risky but is not
  locally proven, mark it for review.
- Do not mark `reviewed_by` or `reviewed_at` unless a human reviewed the risk
  file.

Recommended content:
- Risk surfaces for auth, permissions, billing, audit, tenant isolation, data
  export, PII, migrations, webhooks, deploy, secrets, dependencies, CI, hooks,
  infrastructure, observability, generated code, and LLM/agent behavior when
  applicable.
- Approval type: user, maintainer, security, or ops.
- Forbidden operations without approval.
- Safe read-only commands.
- Required validation command IDs when reliable local commands exist.
- Manual review requirements when local commands are insufficient.
- Incident/reporting notes if the repository has them.

Validation:
- Validate YAML against `.agents/schema-v1/risks.schema.json` if available.
- Check every command ID exists in `.agents/commands.yaml`.
- Check every path exists or is clearly a glob/pattern.
- Report changed files, validation performed, and residual risk.
```

