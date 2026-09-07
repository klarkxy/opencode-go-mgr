# Open Console Gateway for Pi

This is the embedded source template for Open Console Gateway's global Pi package. Its
only provider is `ocg-manager`, which sends OpenAI Chat Completions requests to
the local Open Console Gateway at `http://127.0.0.1:9042/v1`.

The desktop app materializes `models.generated.json` before installation. That
file is the package's complete model catalog; the package does not fetch a
catalog or start a background service.

## Lifecycle boundary

- Install the generated package globally with Pi (`pi install <package-path>`).
- Use Pi's native `/login ocg-manager` flow to enter an Open Console Gateway Key. Pi owns
  that stored credential; it is never written into this package.
- Remove it with `pi remove <package-source>`. Removal unregisters the provider
  on the next Pi startup and leaves Pi's other providers unchanged.

This template is not a standalone public package. Open Console Gateway owns generation,
installation status, and updates.
