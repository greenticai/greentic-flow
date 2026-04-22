# PR-02 — greentic-flow — authoring alias support and wizard plan surface

## Audit update (2026-04-19)

The goal is solid, but the original draft needs one important correction: in this repo, `answers` is component-QA input, not flow-authoring metadata.

### Audit verdict

- `greentic-types` already supports canonical alias decode on the canonical `Node` model, so this PR should align with that existing contract instead of inventing a parallel one.
- `greentic-flow` currently forwards `actions[].answers` directly into component QA / `apply_answers`, so putting `in_map` / `out_map` / `err_map` inside `answers` is the wrong layer and risks strict-schema failures.
- `greentic-flow`'s editable YAML/IR path still treats unknown flattened node keys as operation keys, so raw alias-field support is not safe unless those keys are explicitly reserved and normalized.

### Recommended surgical scope

- Add optional top-level `in_map`, `out_map`, and `err_map` fields to wizard plan `add-step` / `update-step` actions.
- Keep `answers` unchanged and pass it to component QA exactly as today.
- After QA answers are applied and the node is built/updated, normalize the optional alias fields into the flow authoring shape.
- Preserve existing emitted YAML unless one of the new alias fields was explicitly supplied.
- Do not redesign routing, runtime execution, or component ABI.

### Temporary local dependency rule

Until the updated crates are published, validate this work against local paths:

```toml
greentic-types = { path = "../greentic-types", features = ["telemetry-autoinit", "schema"] }
greentic-interfaces-host = { path = "../greentic-interfaces/crates/greentic-interfaces-host" }
greentic-interfaces-wasmtime = { path = "../greentic-interfaces/crates/greentic-interfaces-wasmtime" }
```

## Goal

Teach `greentic-flow` to **accept and emit** the canonical mapping terms at the authoring layer:

- `in_map`
- `out_map`
- `err_map`

while preserving all existing flow documents and existing CLI behavior.

This repo should not force users to rewrite older flows.

---

## Current issue

`greentic-flow` currently operates on its own YAML authoring shapes and legacy shorthand routing/forms.  
That is fine for backward compatibility, but it means the newer declarative mapping language is not yet expressed cleanly at the CLI/wizard layer.

---

## Required behavior

### Read path
The loader/parser must accept both:
- legacy/old flow shapes
- canonical alias mapping fields

### Write path
Prefer one of these two strategies:

#### Safe strategy (recommended)
Continue writing the current stable shape by default, but allow wizard plans and docs to refer to `in_map`, `out_map`, and `err_map`.

#### Slightly more progressive strategy
When creating **new** steps from wizard plan answers, write canonical alias fields if the underlying types crate already supports them, while still preserving legacy flow parsing.

The safer choice is the first one.

---

## Suggested implementation

### 1. Extend the wizard plan action shape, not the component answers schema

Update the wizard plan schema used by `greentic-flow wizard --schema` so `add-step` and `update-step` actions can include top-level optional fields:

```json
{
  "answers": { "...": "..." },
  "in_map": { "...": "..." },
  "out_map": { "...": "..." },
  "err_map": { "...": "..." }
}
```

These should be optional.

Important:

- `answers` remains the component QA answer payload.
- `in_map`, `out_map`, and `err_map` are flow authoring metadata and must live alongside `answers`, not inside it.

### 2. Map aliases into the existing internal write model

Internally normalize:

- `in_map` -> node input mapping / component input shape
- `out_map` -> node success output mapping
- `err_map` -> optional node error mapping

Safe execution order:

1. Run QA / `apply_answers` using `answers` only.
2. Build or update the node exactly as today.
3. If alias fields were supplied, patch the resulting node authoring shape with those mappings.

### 3. Do not require these fields

Most existing steps should still work with only one mapping or none.

### 4. Preserve current routing flags

Keep:
- `--routing-out`
- `--routing-next`
- `--routing-reply`

Do not mix this PR with routing redesign.

---

## Wizard UX behavior

When the user runs `add-step` or `update-step`, the tool should be able to accept mapping answers without forcing all three maps.

### Recommended rules
- `in_map` is optional
- `out_map` is optional
- `err_map` is optional
- absence means legacy/default behavior remains in place

This fits your intended rule that a step often only needs one side mapped.

---

## CLI / schema changes

Update the action-plan schema descriptions so Codex and similar tools understand:

- `in_map` maps from flow payload/state/config into the component call shape
- `out_map` normalizes successful component results for the next step
- `err_map` normalizes error results for the next step

Also make sure descriptions explicitly mention that:
- `config.<key>` should be addressable in `in_map`
- mapping is for **authoring-time flow wiring**, not a component ABI change
- these fields are separate from component `answers`

---

## Files likely to update

- wizard plan schema generation code
- `WizardPlanAction` serialization/deserialization
- plan execution for `add-step` / `update-step`
- node/IR normalization so alias keys are not mistaken for operation keys
- `tests/flow_cli_ops.rs`
- CLI docs in `docs/cli.md`
- internal step add/update plan handlers

---

## Tests to add

Code audit notes driving these tests:

- the generic wizard plan schema currently only exposes `answers` and disallows other top-level fields
- plan execution currently serializes `action.answers` straight into `--answers`
- the flow IR parser currently treats unknown flattened node keys as operation keys
- the canonical types crate already supports alias decode for `in_map` / `out_map` / `err_map`

### 1. Wizard schema contains mapping fields
Verify `wizard --schema` for `add-step` and `update-step` includes `in_map`, `out_map`, `err_map`.

### 2. Add-step accepts top-level `in_map` only
A step with `answers` plus top-level `in_map` should succeed.

### 3. Add-step accepts top-level `out_map` only
A step with `answers` plus top-level `out_map` should succeed.

### 4. Add-step accepts top-level `err_map`
Optional error normalization should be stored.

### 5. Legacy answers continue to work
No mapping aliases present should still pass.

### 6. Update-step remains unchanged when alias fields are omitted
This guards backward compatibility for existing plans.

### 7. `config.<key>` examples appear in docs/schema descriptions
This is important because the user specifically wants config-driven input shaping.

### 8. Raw flow parsing reserves alias keys
If alias fields appear in a node, they must not be misclassified as operation keys.

---

## Acceptance criteria

- No existing flows need rewriting.
- Wizard/action-plan schema supports canonical mapping fields.
- Existing CLI flags remain valid.
- Mapping aliases are optional and additive.
- No component update is required.

---

## Non-goals

- Do not redesign flow execution runtime in this repo.
- Do not enforce a full expression language.
- Do not make `in_map`, `out_map`, `err_map` mandatory.
- Do not remove legacy shorthand YAML.
- Do not push alias fields through component QA schemas.

---

## Suggested PR title

`feat(wizard): add optional in_map/out_map/err_map fields for step plans`

---

## Suggested PR body

This PR adds optional canonical mapping fields to the `greentic-flow` authoring layer and wizard plan schema.

It does not break existing flows, keeps current CLI behavior intact, and allows new plans to express `in_map`, `out_map`, and `err_map` without forcing any existing components or flow documents to be updated.
