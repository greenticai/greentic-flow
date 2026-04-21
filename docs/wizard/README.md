# Wizard Docs

The preferred authoring path in `greentic-flow` is the pack-level wizard:

```bash
greentic-flow wizard <pack>
greentic-flow wizard <pack> --answers <plan.json>
greentic-flow wizard --schema
```

Use the dedicated guide for the full workflow:

- Wizard authoring guide: [`docs/wizard/authoring.md`](authoring.md)

That guide covers:

- the human workflow for creating and editing flows in a pack
- the preferred automation path using `wizard --schema` and `wizard --answers`
- how coding agents should fetch per-component answer schemas with `component-schema`
- how to author `add-step` / `update-step` actions correctly, including `operation`, `routing`, and flow-level mappings

Low-level commands such as `add-step`, `update-step`, and `bind-component` still exist, but they are escape hatches. For new flow authoring and automation, prefer the wizard plan flow.
