## Purpose

Defines the multi-architecture delivery contract for the published container images: both `linux/amd64` and `linux/arm64` variants of the manager and browser-sidecar images are built natively, smoke-tested on their own architecture, and published as one merged manifest per tag without splitting the paired release channels.

## ADDED Requirements

### Requirement: Published image tags resolve for both amd64 and arm64

Every container tag published through the release channel (the immutable `version` and `sha-<short>` tags, and the moving `minor` / `latest` channels when they advance) SHALL be a single multi-architecture manifest index containing exactly the `linux/amd64` and `linux/arm64` variants. A plain `docker pull <tag>` on either architecture MUST select and run the matching variant without emulation.

#### Scenario: Pull on an arm64 host selects the native variant

- **WHEN** the version tag has been published and a host with `linux/arm64` architecture pulls the manager or browser image by tag
- **THEN** the pulled image architecture reports `arm64` and the container starts without QEMU/binfmt emulation

#### Scenario: Pull on an amd64 host is unchanged

- **WHEN** an amd64 host pulls any published tag after this change
- **THEN** it receives the `linux/amd64` variant of the same build, as before

### Requirement: Each architecture is built and smoke-tested natively

The release pipeline SHALL build every image variant on a runner matching its target architecture (no cross-compilation and no QEMU-emulated builds for release artifacts), and the existing smoke suite — manager health/dashboard/auth checks and the browser container's live Chromium verification — MUST pass on each architecture before its digest enters the published manifest.

#### Scenario: An arm64 smoke failure blocks the release

- **WHEN** the arm64 leg of the build matrix fails its smoke test (for example Chromium does not start)
- **THEN** no tag for that release is published or advanced on either architecture

#### Scenario: Browser smoke verifies real Chromium on arm64

- **WHEN** the browser sidecar smoke runs on the arm64 build leg
- **THEN** it launches the arm64 Chromium from the image and asserts the same hardening flags and profile isolation as the amd64 leg

#### Scenario: Build legs run on architecture-matched runners

- **WHEN** the build job's matrix dispatches its legs
- **THEN** the amd64 leg's runner label resolves to `ubuntu-24.04` (not the arm variant) and the arm64 leg's runner label resolves to `ubuntu-24.04-arm` — pins native arm64 builds and rules out a silent QEMU-emulated release.

### Requirement: Paired channels stay atomic across architectures and images

The existing pairing guarantee — a moving tag on the manager image and on the browser sidecar image MUST resolve to the same release version, with the sidecar published first — SHALL be preserved for the merged multi-architecture manifests. A failed or partial publish MUST NOT leave one architecture or one image advanced without the other.

#### Scenario: Moving channel advance is all-or-nothing

- **WHEN** a stable release advances the minor or latest channel
- **THEN** both images' channel tags point at multi-architecture indexes of the same version, and no intermediate state exposes one image or one architecture from a different version

### Requirement: Immutable tag policy applies to the merged manifest digest

The existing immutability and anti-rollback policy SHALL operate on the merged manifest index digest: an already-published version or `sha-` tag MUST NOT be recreated or overwritten, and a re-run for the same release MUST verify that the tag resolves to the same index digest.

#### Scenario: Re-publishing the same version tag is a no-op

- **WHEN** the workflow runs again for a version whose tags already exist
- **THEN** the existing tags are left unchanged and the run verifies their digest matches the candidate index digest

### Requirement: Published platform claims match reality

User-facing documentation and shipping examples (user guide in both languages, README, compose example, maintainer notes) SHALL state that the container images provide `linux/amd64` and `linux/arm64`, and MUST NOT imply native ARM64 availability for the desktop installers, which remain out of scope.

#### Scenario: Compose example no longer pins amd64

- **WHEN** a user deploys the example compose file on an arm64 host
- **THEN** no `platform:` pin forces amd64 emulation; the deployment uses the native arm64 variant
