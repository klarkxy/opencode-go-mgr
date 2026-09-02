## Purpose

Lets a deployment route gateway chat forwarding (and all other outbound HTTP traffic) per model instead of per process: only region-restricted models traverse the manual proxy while every other model connects directly, removing the forced detour of a process-wide manual proxy.

## ADDED Requirements

### Requirement: List proxy mode routes chat forwarding per model

The system SHALL provide a fourth outbound proxy mode ("list mode") for gateway chat forwarding, configured with a direction and a list of known model ids:

- **Whitelist direction**: listed models connect through the configured manual proxy URL; every unlisted model connects directly.
- **Blacklist direction**: listed models connect directly; every unlisted model connects through the configured manual proxy URL.

"Direct" MUST mean ignoring platform/environment proxy configuration, identical to the existing force-direct mode. Both streaming and non-streaming forwards for the same model MUST resolve to the same route.

#### Scenario: Whitelist sends a listed model through the proxy

- **WHEN** list mode is active with whitelist direction, the manual proxy URL is set, and `gpt-5.6-luna` is listed
- **THEN** an authorized chat request for `gpt-5.6-luna` opens its upstream connection through the proxy URL, streaming or not

#### Scenario: Whitelist sends an unlisted model directly

- **WHEN** list mode is active with whitelist direction and a chat request arrives for an unlisted model
- **THEN** its upstream connection bypasses both the proxy URL and any system/environment proxy

#### Scenario: Blacklist inverts the route

- **WHEN** list mode is active with blacklist direction and model X is listed
- **THEN** requests for X connect directly and requests for any unlisted model connect through the proxy URL

### Requirement: Route resolution follows the resolved model on every attempt

The route SHALL be derived from the model actually being forwarded on each forwarding attempt — after alias rewrites, prefer-mapping, and channel fallback — and MUST be re-derived whenever the active plan's model changes within one downstream request. The route MUST NOT be pinned by conversation stickiness across requests for different models. Requests for unknown or custom models (which are forwarded as-is) resolve through the direction's default leg.

#### Scenario: Free-channel fallback re-routes mid-request

- **WHEN** a prefer-mode request starts on a listed free model and the free channel is exhausted, falling back to an unlisted Go model
- **THEN** the fallback attempt connects through the direction's default leg instead of the leg used by the free model

#### Scenario: Claude Desktop alias routes by the mapped model

- **WHEN** a Claude Desktop alias such as `sonnet` is rewritten to a listed model
- **THEN** the forward is routed according to the mapped target model, not the alias

#### Scenario: A settings change mid-request does not reroute in-flight attempts

- **WHEN** a downstream request is mid-flight under list mode and the administrator switches the proxy mode (or edits the list) before one of its later attempts
- **THEN** the in-flight request keeps using the routing snapshot captured at its entry until it completes, and only requests starting afterwards observe the new routing

### Requirement: Non-model-scoped outbound traffic follows the direction's default leg

Outbound requests that are not model-scoped — pricing refresh, official usage sync, account test ping, update check and signed update downloads, upstream model listing, and connectivity/proxy tests — SHALL use the direction's default leg: direct for whitelist, the manual proxy URL for blacklist. They MUST NOT vary with any model list membership. Loopback and in-process control channels (for example the local browser worker connection) are exempt from proxy routing entirely.

#### Scenario: Usage sync stays on the default leg

- **WHEN** list mode is active with whitelist direction and the hourly usage sync runs
- **THEN** its request to the official usage endpoint connects directly, regardless of which models are listed

#### Scenario: Signed update download follows the default leg under blacklist

- **WHEN** list mode is active with blacklist direction and an installed desktop build downloads a signed update
- **THEN** the download goes through the proxy URL, matching what the process-wide manual mode would do

### Requirement: List mode configuration is validated at the settings write gate

Saving settings SHALL be rejected when list mode is active and the manual proxy URL is empty or invalid, the model list is empty, or any listed id is not a known model; the list MUST contain exact known model ids — patterns or wildcards MUST NOT be accepted. Loading a persisted configuration MUST tolerate list entries that are no longer known and an empty list under list mode: startup MUST succeed and such entries match nothing (whitelist with no valid entries behaves as all-direct; blacklist as all-proxy through the configured proxy URL). A configuration lacking the new fields entirely MUST load and behave exactly as before.

#### Scenario: Missing proxy URL is rejected at save

- **WHEN** settings are saved with list mode active and an empty proxy URL
- **THEN** the save fails with a validation error and the previous configuration stays active

#### Scenario: Unknown model id is rejected at save

- **WHEN** settings are saved with a list containing an id that is not a known model
- **THEN** the save fails with a validation error

#### Scenario: Registry shrink does not brick startup

- **WHEN** a persisted list contains an id that a newer release removed from the known model registry
- **THEN** startup succeeds, the stale id never matches any request, and the next settings save enforces the known-id rule again

#### Scenario: Legacy config loads unchanged

- **WHEN** a configuration persisted before this change is loaded
- **THEN** routing behavior is identical to the previous release with no migration

#### Scenario: A valid list-mode save takes effect for subsequent requests

- **WHEN** list mode settings are saved successfully
- **THEN** the settings revision advances and the next forwarded request uses the new routing without a process restart

### Requirement: Every forward log row records its route

Each forward log row — success rows included, not only failures — SHALL record which route leg its attempt used, from the closed label set `auto` (process-wide auto mode), `proxy` (manual mode or a list-mode proxy leg), and `direct` (force-direct mode or a list-mode direct leg). Rows written before this change keep an empty marker meaning "not recorded". The record MUST NOT include the proxy URL, account keys, or any credential material.

#### Scenario: A successful proxied attempt is labeled

- **WHEN** a forward attempt succeeds while connecting through the proxy URL under list mode
- **THEN** its forward log row carries the `proxy` route label

#### Scenario: Legacy modes are labeled with their process-wide leg

- **WHEN** a request is forwarded under the existing auto, manual, or direct mode
- **THEN** its forward log row carries `auto`, `proxy`, or `direct` respectively

#### Scenario: Route labels carry no secrets

- **WHEN** any route label is written to forward logs
- **THEN** it contains only the closed-set label, not the proxy URL or credentials

### Requirement: Settings UI selects listed models from the known registry by checkbox

The settings page SHALL offer list mode with a direction control and a checkbox list whose entries come from the known model registry (the same registry backing the model listing), each showing a protocol hint; free-channel models are selectable and MUST carry a hint that their IP-shared free quota follows the route's egress IP. The UI MUST NOT offer free-text model entry or wildcard patterns; an empty selection or an empty proxy URL while list mode is active MUST be blocked before submission with the same errors as the API.

#### Scenario: Checkbox list matches the known registry

- **WHEN** the settings page renders list mode options
- **THEN** every known model appears exactly once with its preferred-protocol hint, and selecting models then saving persists the exact id set

#### Scenario: Free models warn about egress-based quota

- **WHEN** the checkbox list renders a free-channel model
- **THEN** it shows the shared-egress hint alongside the protocol hint

#### Scenario: Empty selection cannot be saved

- **WHEN** the user unchecks every model and saves while list mode is active
- **THEN** the UI blocks the save with the same validation error as the API
