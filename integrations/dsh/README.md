# Open Console Gateway for DSH

This is the embedded source template for Open Console Gateway's `ocg-manager-dsh` bundle.
It is installed only into DSH's `web` profile and adds an OCG-owned
companion plugin with the `ocg-manager` provider. The companion reuses the
public `PiAiAdapter` transport and registers only its fixed provider route, so
it does not replace the base `llm-pi-ai` row or mask providers supplied by
another bundle. It does not modify another DSH profile.

The desktop app replaces the generated-model placeholder before installation.
The route uses OpenAI Chat Completions at `http://127.0.0.1:9042/v1` and refers
to `OCG_MANAGER_API_KEY`; the Key remains outside this package.

## Lifecycle boundary

- Open Console Gateway installs or removes this bundle from the fixed `web` profile.
- The DSH process resolves `OCG_MANAGER_API_KEY` at request time. The Desktop
  connector field-manages only that one assignment in DSH's `.env`; the bundle
  has no login screen and does not persist a credential.
- Removing the bundle removes only the OCG-owned companion row on the next DSH
  start. Base-profile rows, other bundles, and other profiles stay untouched.

This template is not a standalone public package. Open Console Gateway owns generation,
installation status, and updates.
