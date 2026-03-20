# Legacy Compatibility and Deprecation Signals

This page tracks legacy surfaces that are intentionally not part of canonical
v0.6 runtime behavior.

## Legacy surfaces

1. Any runtime use of `component-wizard` worlds
   - Status: disallowed in `src/**` and `tests/**` via CI/local guard.
   - Canonical replacement: canonical node world setup flow.
2. Local bindgen for wizard contracts
   - Status: disallowed for flow runtime.
   - Canonical replacement: `greentic-interfaces` bindings.
3. Legacy setup apply contract (`apply-answers(...)` export)
   - Status: deprecated and not used in runtime path.
   - Canonical replacement: invoke op `setup.apply_answers`.
4. Legacy wizard setup discovery exports (`describe/qa-spec` on wizard world)
   - Status: deprecated and not used in runtime path.
   - Canonical replacement: `node.describe()` and `descriptor.setup`.
5. Unconfirmed remove operations
   - Status: disallowed.
   - Canonical replacement: mandatory `Type REMOVE to confirm`.
6. Direct usage of non-canonical WIT type modules for setup flow
   - Status: deprecated in flow runtime.
   - Canonical replacement: `greentic_interfaces::canonical::*`.
7. Reintroduction of `component-wizard` strings in runtime/tests
    - Status: blocked by `ci/check_no_component_wizard_usage.sh`.
    - Canonical replacement: canonical setup terminology.

## Usage rule

- New implementation work must target canonical v0.6 only.
- Keep legacy notes in docs only.
