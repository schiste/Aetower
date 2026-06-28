# Agent Contract Schemas v1

This directory defines the first version of Aetower's machine-checkable agent
contracts. Repositories can expose these YAML files:

- `.agents/manifest.yaml`
- `.agents/tasks.yaml`
- `.agents/repo-map.yaml`
- `.agents/contracts.yaml`
- `.agents/commands.yaml`
- `.agents/validation.yaml`
- `.agents/boundaries.yaml`
- `.agents/risks.yaml`
- `.agents/references.yaml`

The schemas are intentionally split by responsibility:

- `manifest.schema.json` lists expected contracts, schemas, generator/check
  commands, and cross-file integrity policy.
- `tasks.schema.json` maps operator intent to task classes, required reads,
  likely paths, validation rules, and completion reporting.
- `repo-map.schema.json` describes repository topology and generated/ignored roots.
- `contracts.schema.json` declares invariants agents must not break, such as API,
  auth, data, release, storage, performance, and security contracts.
- `commands.schema.json` describes exact command metadata and safety properties.
- `validation.schema.json` maps touched paths to validation commands.
- `boundaries.schema.json` describes architecture and dependency rules.
- `risks.schema.json` declares high-risk surfaces and required safeguards.
- `references.schema.json` points agents to deeper context loaded only when needed.

JSON Schema validates local file shape. Cross-file guarantees such as unique IDs,
command references, source-file existence, and generated-file freshness belong in
the contract checker declared by `.agents/manifest.yaml`.

Hooks should validate these contracts, not regenerate them. Generation belongs in
an explicit command so agents and humans can review diffs before committing.
