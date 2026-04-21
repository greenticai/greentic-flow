# PR: Extend `greentic-flow` wizard plan schema for declarative multi-node orchestration

## Summary

This PR adds the minimum missing wizard-plan features needed for coding agents to author complex flows without hand-editing `main.ygtc` for every routing or orchestration change.

The current wizard plan model already supports:

- flow creation and deletion
- step add/update/delete
- component binding
- component-mode answers
- flow-level `in_map`, `out_map`, and `err_map`

What it still lacks is a first-class way to express the remaining two pieces of orchestration:

1. the **operation** a step should invoke
2. the **routing/edges** the step should use after it is inserted or updated

This PR keeps the change set intentionally small:

- add `operation` to wizard `add-step` / `update-step` actions
- add `routing` to wizard `add-step` / `update-step` actions
- relax `in_map` / `out_map` / `err_map` from object-only to arbitrary JSON values
- document the pattern for coding agents authoring multi-step flows

## Why this matters

For a complex flow, a coding agent needs to declare:

- the step/node
- the component + operation
- the payload mapping
- the next route(s)

Today the wizard schema covers most of that, but not all of it. In practice that means:

- coding agents can add/update steps
- but they still need to hand-author raw YGTc for exact routing behavior
- and they cannot fully express a runnable `component.exec` orchestration step in the plan schema alone

This is the minimum gap blocking “declarative multi-node flow orchestration”.

---

## Proposed schema changes

### 1. Extend `WizardPlanAction`

Add two optional fields:

```rust
#[serde(skip_serializing_if = "Option::is_none")]
operation: Option<String>,

#[serde(skip_serializing_if = "Option::is_none")]
routing: Option<serde_json::Value>,
```

These only apply to `add-step` and `update-step`.

`operation` should remain optional at the schema level for backward compatibility.

Behaviorally, it should be treated as effectively required when the target step would otherwise be ambiguous, or runnable, and the component contract cannot supply a safe default.

So the intended rule is:

- schema: optional
- execution/validation: require it when needed

### 2. Extend generic action schemas

In `generic_add_step_action_schema()` and `generic_update_step_action_schema()`:

- add:

```json
"operation": { "type": "string" }
```

- add:

```json
"routing": {
  "description": "Optional declarative routing override for the authored step. Use \"out\", \"reply\", or an explicit route array.",
  "oneOf": [
    { "const": "out" },
    { "const": "reply" },
    {
      "type": "array",
      "items": {
        "type": "object",
        "additionalProperties": false,
        "properties": {
          "to": { "type": "string" },
          "condition": { "type": "string" },
          "out": { "type": "boolean" },
          "reply": { "type": "boolean" }
        }
      }
    }
  ]
}
```

- relax these from `"type": "object"` to any JSON value:

```json
"in_map": { "description": "..." }
"out_map": { "description": "..." }
"err_map": { "description": "..." }
```

Reason:

- the runtime treats mappings as JSON values, not only objects
- object-only is unnecessarily restrictive for plan authoring
- relaxing them to arbitrary JSON is intentional
- guardrails should remain at execution/runtime validation layers
- the plan schema should allow any JSON value
- invalid mappings should be rejected only when applied to the actual node/contract

### 3. Extend `schema_for_wizard_plan_action(...)`

When an action already includes `operation` and/or `routing`, emit them back into the per-action schema so replay remains strict and deterministic.

---

## Proposed execution changes

The plan executor should forward the new fields into the existing CLI machinery.

### `execute_add_step_plan_action(...)`

Current behavior already forwards:

- `after`
- `step_id`
- `answers`
- `in_map`
- `out_map`
- `err_map`

Add support for:

- `operation`
- `routing`

Suggested behavior:

1. If `action.operation` is present, pass it through to `AddStepArgs.operation`.
2. If `action.routing` is present:
   - `"out"` -> set `routing_out = true`
   - `"reply"` -> set `routing_reply = true`
   - array -> translate it through a shared helper into the existing routing representation/path
3. If `action.routing` is absent, preserve current defaults.

Implementation note:

- reuse existing routing parsing/helpers if possible
- do not introduce ad hoc temp-file logic in multiple places if it can be avoided
- best path is to factor a helper that converts wizard-plan routing JSON into the existing routing representation, then feed that into the same add/update routing path already used by CLI flags

### `execute_update_step_plan_action(...)`

Same behavior:

1. Pass `action.operation` through to `UpdateStepArgs.operation`
2. Translate `action.routing` into:
   - `routing_out`
   - `routing_reply`
   - or `routing_json`
3. If absent, preserve current update-step behavior

This keeps the implementation small by reusing the existing routing parsing and validation path.

`NEXT_NODE_PLACEHOLDER` should remain an internal implementation detail only.

The wizard plan schema should express real routing intent, not compiler placeholders. Agents should author:

- `routing: "out"`
- `routing: "reply"`
- `routing: [{ "to": "next" }]`

and should never need to see placeholder mechanics.

---

## Tests to add

### 1. Schema coverage test

Extend the existing wizard schema test so it also asserts:

- `operation` is present on `add-step` and `update-step`
- `routing` is present on `add-step` and `update-step`
- `in_map`, `out_map`, `err_map` no longer force `"type": "object"`

Suggested test target:

- existing area around `wizard_step_schema_includes_optional_mapping_aliases`

### 2. Plan roundtrip test

Extend the existing roundtrip test so it preserves:

- `operation`
- `routing`
- `in_map`
- `out_map`
- `err_map`

Example action:

```json
{
  "action": "add-step",
  "flow": "flows/global/messaging/main.ygtc",
  "component": "components/widget.wasm",
  "operation": "handle_message",
  "mode": "default",
  "answers": { "answer": "value" },
  "routing": [{ "to": "next-step" }],
  "in_map": { "config": { "provider": "ollama" } }
}
```

### 3. Add-step execution test

Add a wizard-plan execution test that proves `operation` and `routing` are actually applied, not just accepted by the schema.

Minimum assertion:

- create a temp pack with one flow
- run a wizard plan containing `add-step`
- verify the resulting flow contains:
  - the chosen op key / operation
  - the chosen routing

This can be done with an existing local fixture component or a minimal local manifest fixture.

### 4. Update-step execution test

Add a wizard-plan execution test that:

- seeds a flow with a step
- runs `update-step` plan action with:
  - `operation`
  - `routing`
  - mappings
- verifies the flow changed accordingly

### 5. Routing shorthand coverage

Add unit coverage for wizard plan routing translation:

- `"out"`
- `"reply"`
- array form

This is especially useful if the implementation uses a temp file bridge into `routing_json`.
This is still useful even if the implementation factors a shared routing-conversion helper, because shorthand translation is one of the main new behavioral surfaces in this PR.

---

## Docs to add/update

### 1. Update `docs/cli.md`

In the wizard plan sections, document that `add-step` / `update-step` actions now support:

- `operation`
- `routing`
- `in_map`
- `out_map`
- `err_map`

Document the routing forms:

```json
"routing": "out"
```

```json
"routing": "reply"
```

```json
"routing": [
  { "to": "show_research_plan" },
  { "condition": "response.action == \"retry\"", "to": "planner" }
]
```

### 2. Add a coding-agent-oriented doc

Add a new doc, for example:

- `docs/wizard/complex-flow-authoring.md`

Purpose:

- explain how a coding agent should author a complex flow declaratively using the wizard plan schema
- explain the relationship between:
  - step = node
  - routing = edge
  - `in_map/out_map/err_map` = dataflow

Recommended structure:

#### a. Mental model

- Steps are nodes
- Routing defines edges
- Mappings define payload/dataflow

#### b. Authoring workflow

1. Create flow
2. Add steps
3. Set component + operation
4. Set `routing`
5. Set `in_map`
6. Validate

#### c. Complex-flow example

Example plan for:

- card input
- planner LLM
- card output
- analyst LLM
- final report card

That example should show a coding agent exactly how to author:

- multiple `add-step` actions
- explicit `operation`
- explicit `routing`
- explicit `in_map`

#### d. Guidance

- use `component-schema` first to inspect operation requirements
- use `answers` / `mode: "setup"` for persisted component config
- use `in_map` for per-invocation runtime payload shaping
- use `routing` for graph structure

This is the endorsed pattern:

- `answers` / setup mode for persisted component config
- `in_map` for per-invocation runtime payload

### 3. Update `docs/wizard/README.md`

Add a short note that the wizard plan schema now supports not just step CRUD, but also:

- explicit operation selection
- explicit routing declarations
- flow-level mapping declarations

---

## Example target plan after this PR

This should be valid and understandable to a coding agent:

```json
{
  "schema_id": "greentic-flow.wizard.plan",
  "schema_version": "2.0.0",
  "actions": [
    {
      "action": "add-step",
      "flow": "flows/main.ygtc",
      "step_id": "research_planner",
      "after": "main_menu",
      "component": "oci://ghcr.io/greenticai/component/component-llm-openai:latest",
      "operation": "handle_message",
      "mode": "setup",
      "answers": {},
      "in_map": {
        "config": {
          "provider": "ollama",
          "base_url": "http://127.0.0.1:11434/v1",
          "default_model": "llama3:8b"
        },
        "input": {
          "messages": [
            {
              "role": "system",
              "content": "You are a Research Planner Agent."
            },
            {
              "role": "user",
              "content": "{{entry.input.metadata.user_question}}"
            }
          ]
        }
      },
      "routing": [
        { "to": "show_research_plan" }
      ]
    }
  ]
}
```

---

## Non-goals for this PR

This PR should **not** try to solve everything:

- no higher-level automatic flow synthesis
- no auto-generated business logic
- no automatic `in_map` design for every component
- no parity work for `delete-step` or other unrelated wizard actions in this PR

The goal is only to make the wizard plan schema expressive enough for a coding agent to declare:

- node
- operation
- edge
- mapping

That is the real minimum needed for declarative multi-node flow orchestration.

This PR should stay scoped to `add-step` and `update-step` only. `delete-step` and other wizard actions can gain parity later if needed, but they are not necessary to close the core orchestration gap.
