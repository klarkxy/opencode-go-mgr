## Purpose

Defines how the dashboard connection center and settings page present the two-tier key model: a lightweight key switcher fed by connection data instead of the full settings payload, per-key copy and regeneration actions, and the settings key management section.

## ADDED Requirements

### Requirement: Connection center key switcher uses a lightweight payload

The connection center SHALL obtain the primary key value, sub key list, settings revision, and connection fields (gateway port, client root URL, upstream base URL) through a dedicated lightweight response rather than the full settings object, and MUST NOT hold the complete settings shape. With more than one valid credential the center MUST show a switcher listing the primary key (pinned first, labeled) and every enabled sub key with a masked value preview, defaulting to the primary key. With a single credential the layout MUST match the pre-multi-key single-key presentation.

#### Scenario: Default selection is the primary key

- **WHEN** the dashboard loads with one primary key and at least one sub key
- **THEN** the switcher shows the primary key selected and its masked value displayed

#### Scenario: Switching keys retargets value and actions

- **WHEN** the user selects a different key in the switcher
- **THEN** the masked value, copy action, and regeneration action all target the selected key

#### Scenario: Single credential keeps the original layout

- **WHEN** only the primary key exists
- **THEN** no switcher is rendered and the connection row matches the single-key layout

### Requirement: Copy and regenerate act on the selected key only

Copying MUST place the selected key's full value on the clipboard. Regenerating a sub key MUST require confirmation stating that only the selected key's previous value stops working, MUST invalidate only that key, and MUST refresh the panel to the new value while keeping the selection. Regenerating the primary key MUST use the legacy regeneration semantics.

#### Scenario: Regeneration scope is the selected key

- **WHEN** the user confirms regeneration for a selected sub key while other keys exist
- **THEN** only the selected key's previous value stops authenticating and every other credential keeps working

#### Scenario: New value is reflected immediately

- **WHEN** regeneration succeeds but the follow-up settings refresh fails
- **THEN** the panel still shows the new value for the selected key instead of the invalidated one

### Requirement: Settings page manages the two tiers separately

The settings key section SHALL present the primary key as a fixed, always-active entry exposing only rotation, and each sub key with rename, enable/disable, regenerate, and delete actions. Enabling or disabling a sub key MUST NOT require touching any other credential, and delete MUST be confirmed with a message stating the key stops working immediately while historical usage stays attributed by name.

#### Scenario: Primary key row exposes rotation only

- **WHEN** the settings key section renders
- **THEN** the primary key row offers regeneration and no disable or delete action

#### Scenario: Sub key lifecycle from settings

- **WHEN** the user renames, toggles, regenerates, or deletes a sub key
- **THEN** the change applies immediately, the list reflects the new state, and the settings revision advances

### Requirement: Key plaintext is exposed only behind the dashboard session layer

Any response that carries a key's plaintext value MUST be served behind the dashboard session protection layer (an authenticated session cookie, or loopback local-mode access without forwarding headers). No endpoint or UI surface outside that layer MAY return key plaintext. This holds for every existing exposure — the settings response, the legacy gateway status response, and the connection info response — and for any future one.

#### Scenario: Plaintext endpoints require the dashboard session

- **WHEN** a request without a valid dashboard session (and not from loopback local-mode) targets any endpoint whose response includes key plaintext
- **THEN** the request is rejected and no key plaintext is returned
