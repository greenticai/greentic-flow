Nightly frequent-component wizard fixtures live under this directory.

Layout:

- `tests/e2e/frequent-components/<component-id>/default.answers.json`
- `tests/e2e/frequent-components/<component-id>/personalised.answers.json`

These files are consumed by `ci/nightly_wizard_e2e.sh` for both:

- `greentic-flow add-step --wizard-mode default`
- `greentic-flow add-step --wizard-mode setup`

The nightly job runs each mode twice:

- with `--interactive --answers-file ...`
- with `--answers-file ...`

Guidance:

- Keep fixtures deterministic and offline-safe.
- Check in the minimal answers needed for the component to complete setup.
- Check in a fixture for every component/mode pair, even when the content is just `{}`.
- Use `__ROOT_DIR__` when an answer needs to point at a checked-in file in this repo.
- Use the component id from `frequent-components.json` as the directory name.

Example:

`tests/e2e/frequent-components/http/default.answers.json`

```json
{}
```
