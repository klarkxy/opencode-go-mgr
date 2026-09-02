## Purpose

Dashboard connection panel lets users switch, copy, and rotate among multiple gateway keys while keeping the single-key experience unchanged.

## ADDED Requirements

### Requirement: Connection panel shows a key selector
When more than one enabled key exists, the dashboard connection panel SHALL provide a selector listing the enabled keys. Selecting a key SHALL update the displayed (masked) key and the copy target to that key. When only one enabled key exists, the panel SHALL render exactly as it does today.

#### Scenario: Multiple keys present
- **WHEN** two or more enabled keys exist and the user opens the connection panel
- **THEN** the panel shows a key selector and the currently selected key's masked value

#### Scenario: Switch selected key
- **WHEN** the user selects a different key in the panel
- **THEN** the displayed masked key and the copy target switch to the selected key

#### Scenario: Single key unchanged
- **WHEN** only one key exists
- **THEN** the panel shows that key's masked value without a selector, matching the current single-key layout

### Requirement: Copy copies the selected key
The copy action in the connection panel SHALL copy the full value of the currently selected key.

#### Scenario: Copy selected key
- **WHEN** the user clicks copy while key B is selected
- **THEN** the clipboard receives key B's full value

### Requirement: Regenerate targets the selected key only
The regenerate action in the connection panel SHALL apply to the currently selected key only. The confirmation dialog SHALL state that only that key becomes invalid. Other keys SHALL continue to authenticate after the operation.

#### Scenario: Regenerate one of two keys
- **WHEN** the user confirms regeneration of key A while key B also exists
- **THEN** key A receives a new value, key B remains unchanged and continues to authenticate

### Requirement: Per-client guides reference the primary key
The per-client configuration guides SHALL embed the primary key's value, and SHALL reflect its latest value after a regeneration.

#### Scenario: Guides after primary regeneration
- **WHEN** the primary key is regenerated
- **THEN** the copied snippets in the client guides use the new primary key value
