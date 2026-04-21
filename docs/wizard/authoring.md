# Wizard Flow Authoring

This is the preferred flow authoring path for both humans and coding agents.

Use the wizard when you want to:

- create or edit flows inside a pack
- add or update component-backed steps
- keep `pack.yaml`, flow files, and resolve sidecars in sync
- automate edits without hand-editing `*.ygtc`

The preferred commands are:

```bash
greentic-flow wizard <pack>
greentic-flow wizard --schema
greentic-flow wizard <pack> --answers <plan.json>
greentic-flow component-schema <component> --mode <mode>
```

## Pack layout

The wizard runs against a pack root, not a single loose flow file.

At minimum, the pack should contain:

```text
<pack>/
  pack.yaml
  flows/
    main.ygtc
```

Wizard plan actions refer to flows by pack-relative path such as `flows/main.ygtc`.

## Human Workflow

### Interactive path

Use this when you want the CLI to guide you.

```bash
greentic-flow wizard /path/to/pack
```

The wizard lets you:

- add flows
- edit flow summaries
- add, update, or delete steps
- save the resulting action plan for replay

When you save from the interactive wizard, `greentic-flow` can write a reusable plan JSON file. That file can later be replayed with `--answers`.

### Non-interactive replay path

If you already have a wizard plan JSON file:

```bash
greentic-flow wizard /path/to/pack --answers plan.json
```

This applies the plan without prompting.

After applying a plan, validate the resulting flows:

```bash
greentic-flow doctor /path/to/pack/flows
```

## Coding Agent Workflow

Coding agents should prefer schema-guided plan authoring instead of:

- editing `*.ygtc` directly
- calling low-level `add-step` / `update-step` first
- inventing component answer shapes from memory

The correct sequence is:

1. Fetch the generic wizard plan schema.
2. Fetch the exact component answer schema for each component action.
3. Author a wizard plan JSON file.
4. Apply the plan with `wizard --answers`.
5. Run `doctor` on the resulting flows.

### 1. Fetch the generic wizard plan schema

```bash
greentic-flow wizard --schema > /tmp/greentic-flow.wizard.plan.schema.json
```

This returns the generic action-plan schema for:

- `add-flow`
- `edit-flow-summary`
- `generate-translations`
- `delete-flow`
- `add-step`
- `update-step`
- `delete-step`

Use this first to understand the top-level plan shape.

### 2. Fetch exact component answer schemas

For every `add-step` or `update-step` action, fetch the exact answer contract before writing `answers`.

```bash
greentic-flow component-schema <component> --mode default
greentic-flow component-schema <component> --mode setup
greentic-flow component-schema <component> --mode update
greentic-flow component-schema <component> --mode remove
```

Examples:

```bash
greentic-flow component-schema \
  oci://ghcr.io/greenticai/component/component-llm-openai:latest \
  --mode setup

greentic-flow component-schema \
  components/component_llm_openai.wasm \
  --mode default
```

Important:

- `answers` in a wizard plan must match the schema returned by `component-schema`.
- `answers` are component answers only.
- Flow authoring fields such as `routing`, `in_map`, `out_map`, and `err_map` are not part of component `answers`.

### 3. Author the wizard plan JSON

Minimum envelope:

```json
{
  "schema_id": "greentic-flow.wizard.plan",
  "schema_version": "2.0.0",
  "actions": []
}
```

### 4. Apply the plan

```bash
greentic-flow wizard /path/to/pack --answers /tmp/plan.json
```

### 5. Validate the result

```bash
greentic-flow doctor /path/to/pack/flows
```

## Rules For Agent-Authored Plans

### Flow paths

- `flow` must be pack-relative.
- `flow` must stay under `flows/`.
- `flow` must end in `.ygtc`.
- Do not use `../` to escape the pack root.

Good:

```json
{ "flow": "flows/main.ygtc" }
```

Bad:

```json
{ "flow": "../outside.ygtc" }
```

### Component source

The `component` field accepts either:

- an OCI or other remote reference such as `oci://...`
- a local wasm path

For buildable packs, prefer one of these:

- a pinned OCI ref
- a wasm path under `components/`

Examples:

```json
{ "component": "oci://ghcr.io/greenticai/component/component-llm-openai:latest" }
```

```json
{ "component": "components/component_llm_openai.wasm" }
```

Using a pack-local wasm path is the safest option when you want the pack to stay self-contained.

### `operation`

`operation` is optional at the schema level for backward compatibility.

In practice, coding agents should treat it as required whenever:

- the component exposes more than one runnable operation
- the target step would otherwise be ambiguous
- the component contract does not provide a safe default

If you know which operation should be authored, include it explicitly.

### `routing`

Wizard plans express real routing intent, not internal placeholders.

Valid forms are:

```json
{ "routing": "out" }
```

```json
{ "routing": "reply" }
```

```json
{ "routing": [{ "to": "next" }] }
```

Do not author `NEXT_NODE_PLACEHOLDER`. That is an internal implementation detail.

### `in_map`, `out_map`, `err_map`

These are top-level wizard plan fields on `add-step` and `update-step`.

They are intentionally separate from component `answers`.

Example:

```json
{
  "action": "add-step",
  "flow": "flows/main.ygtc",
  "step_id": "llm",
  "component": "components/component_llm_openai.wasm",
  "mode": "setup",
  "operation": "handle_message",
  "answers": {
    "provider": "ollama",
    "base_url": "http://127.0.0.1:11434/v1",
    "default_model": "llama3.2",
    "endpoint_requires_api_key": false
  },
  "in_map": {
    "messages": "$.input.messages"
  },
  "routing": "out"
}
```

Notes:

- the schema intentionally allows arbitrary JSON for these mappings
- validity is checked when the rendered node is applied against the actual flow/component contract

### Setup mode vs runtime input

For many components, `mode: "setup"` only answers persisted component config questions.

If the selected operation also requires per-invocation input, the agent must provide that separately, typically via `in_map`.

Preferred pattern:

- `mode: "setup"` for persisted component config
- `in_map` for runtime input

For complex runnable components, especially `component-llm-openai.handle_message`, coding agents should prefer a two-phase authoring pattern:

1. `add-step` to create the component-backed step with setup/config answers only.
2. `update-step` on the same step to attach the runnable `operation`, `in_map`, and `routing`.

This keeps persisted config and per-invocation payload mapping separate and mirrors how real flows are typically composed.

If setup answers alone do not satisfy the operation contract, `greentic-flow` rejects the step instead of writing a runnable-looking but invalid node.

## Recommended Patterns

### Add a new flow, then add a component step

```json
{
  "schema_id": "greentic-flow.wizard.plan",
  "schema_version": "2.0.0",
  "actions": [
    {
      "action": "add-flow",
      "flow": "flows/main.ygtc",
      "flow_id": "main",
      "flow_type": "messaging"
    },
    {
      "action": "add-step",
      "flow": "flows/main.ygtc",
      "step_id": "card",
      "component": "components/component_adaptive_card__0_6_0.wasm",
      "mode": "default",
      "operation": "run",
      "answers": {
        "card_source": "inline",
        "default_card_inline": "{\"type\":\"AdaptiveCard\",\"version\":\"1.6\",\"body\":[{\"type\":\"TextBlock\",\"text\":\"Hello\"}]}",
        "multilingual": false
      },
      "routing": "out"
    }
  ]
}
```

### Add a setup-backed LLM step, then attach runnable input

This is the recommended pattern for `component-llm-openai` and similar components whose runnable operation needs both persisted config and invocation input.

```json
{
  "schema_id": "greentic-flow.wizard.plan",
  "schema_version": "2.0.0",
  "actions": [
    {
      "action": "add-step",
      "flow": "flows/main.ygtc",
      "step_id": "llm",
      "component": "components/component_llm_openai.wasm",
      "mode": "setup",
      "answers": {
        "provider": "ollama",
        "base_url": "http://127.0.0.1:11434/v1",
        "default_model": "llama3.2",
        "endpoint_requires_api_key": false
      }
    },
    {
      "action": "update-step",
      "flow": "flows/main.ygtc",
      "step_id": "llm",
      "operation": "handle_message",
      "in_map": {
        "config": "$.config.llm",
        "input": {
          "messages": "$.input.messages"
        }
      },
      "routing": "out"
    }
  ]
}
```

Notes:

- Use setup answers to persist component config such as `provider: "ollama"` and `base_url`.
- Use `in_map.config` when the runtime payload should forward previously persisted config into the operation input shape.
- Use `in_map.input.messages` to satisfy the `handle_message` invocation contract.
- Do not place `in_map`, `routing`, or other flow-shaping fields inside component `answers`.

## Strict Schema For An Existing Plan

If you already have a plan file and want a schema constrained to that exact action list:

```bash
greentic-flow wizard /path/to/pack --schema --answers /tmp/plan.json > /tmp/plan.strict.schema.json
```

This is useful when a coding agent wants:

- the generic schema first
- a concrete plan draft second
- a strict schema for validating edits to that exact draft

## When To Use Low-Level Commands

Use `add-step`, `update-step`, `delete-step`, and `bind-component` directly only when you intentionally need a low-level escape hatch, for example:

- debugging a single node mutation
- repairing a sidecar binding
- working outside the pack-level wizard flow

For normal authoring and automation, prefer `wizard --schema` + `component-schema` + `wizard --answers`.
