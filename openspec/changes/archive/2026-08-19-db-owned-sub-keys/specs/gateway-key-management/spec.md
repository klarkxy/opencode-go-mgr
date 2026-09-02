## Purpose

Defines the two-tier gateway credential model: one legacy primary key that always authenticates and follows the original settings path, plus database-owned sub keys managed through a dedicated lifecycle API, together with the multi-header authentication contract and upgrade/downgrade safety.

## ADDED Requirements

### Requirement: Primary key always authenticates and follows the legacy path

The system SHALL keep exactly one primary gateway key stored in the legacy settings field. It MUST always be active: no API may disable or delete it. Its value MAY be customized through the generic settings update (trimmed, non-empty) and rotated through the dedicated regeneration endpoint, matching pre-multi-key behavior.

#### Scenario: Settings update sets a custom primary key

- **WHEN** a client submits a settings update carrying a non-empty `gateway_key` value after trimming that differs from every active sub key value
- **THEN** the primary key adopts that value and subsequent requests presenting it authenticate

#### Scenario: Blank primary key is rejected

- **WHEN** a settings update or any configuration write path carries a `gateway_key` that is empty after trimming
- **THEN** the write is rejected with a clear error and the previous primary key is retained

#### Scenario: Primary key colliding with a sub key is rejected

- **WHEN** a settings update carries a `gateway_key` value equal to an active sub key's value
- **THEN** the write is rejected and neither credential changes

#### Scenario: Primary key rotation

- **WHEN** the primary key regeneration endpoint is invoked
- **THEN** a fresh value is issued, the previous value stops authenticating immediately, and no other credential is affected

#### Scenario: Primary key cannot be disabled or deleted

- **WHEN** any key-management operation would disable or delete the primary key
- **THEN** the operation is rejected (or the interface does not expose it) and the primary key remains active

### Requirement: Sub keys live in a dedicated database store with a full lifecycle

Sub keys SHALL be persisted in their own database table owned exclusively by the key lifecycle API. Each sub key MUST carry a stable id, a name (non-empty, length-bounded), a unique value, an enabled flag, and a soft-delete marker. Creating a sub key MUST return the full value exactly once. Soft-deleting a sub key MUST clear its plaintext while retaining id, name, and deletion timestamp for log attribution. The number of active (non-deleted) sub keys MUST NOT exceed 64; soft-deleted entries do not count against the ceiling. The generic settings update MUST NOT create, modify, or remove sub keys.

#### Scenario: Create returns the value exactly once

- **WHEN** a sub key is created with a valid name
- **THEN** the response includes the full key value once, and later listings show only masked or identifier information

#### Scenario: Soft delete preserves attribution

- **WHEN** a sub key is deleted
- **THEN** its value stops authenticating immediately, its plaintext is cleared, and its id and name remain available for historical log attribution

#### Scenario: Active sub key ceiling

- **WHEN** a create request arrives while 64 active sub keys exist
- **THEN** the request is rejected with a clear error and no key is created

#### Scenario: Sub key values never collide with the primary key

- **WHEN** a sub key is created or regenerated
- **THEN** its generated value differs from the current primary key value

#### Scenario: Operations on an unknown sub key identifier are rejected

- **WHEN** a key lifecycle operation addresses an identifier that matches no sub key row — including the primary key's identifier, which never exists as a sub key
- **THEN** the operation fails with a clear error and no key state changes

#### Scenario: Disabled sub key value cannot become the primary key

- **WHEN** a settings update carries a `gateway_key` value equal to the value of any non-deleted sub key, whether enabled or disabled
- **THEN** the write is rejected and neither credential changes

#### Scenario: Re-enabling a colliding sub key is rejected

- **WHEN** a disabled sub key whose value equals the current primary key value is re-enabled
- **THEN** the operation is rejected and the sub key stays disabled

#### Scenario: Settings updates cannot touch sub keys

- **WHEN** a settings update payload contains sub key material
- **THEN** sub keys are unaffected by the save

### Requirement: Authentication accepts any matching candidate header

The gateway SHALL treat every non-empty authentication candidate the request presents — `Authorization: Bearer` value, `x-api-key`, and `x-goog-api-key` — as acceptable presentations. Authentication MUST succeed when ANY candidate matches ANY currently valid credential (the primary key value or any enabled, non-deleted sub key value), and MUST fail with 401 when none matches. A disabled or soft-deleted sub key value MUST NOT authenticate.

#### Scenario: Wrong x-api-key alongside correct x-goog-api-key

- **WHEN** a request carries an `x-api-key` that matches no credential and an `x-goog-api-key` that matches an active credential
- **THEN** the request is authenticated

#### Scenario: No candidate matches

- **WHEN** every presented candidate header differs from every valid credential, or all headers are absent/empty
- **THEN** the gateway responds 401 without forwarding upstream

#### Scenario: Disabled sub key is rejected

- **WHEN** a request presents the value of a sub key that was disabled or soft-deleted
- **THEN** the gateway responds 401

### Requirement: Credential state survives version downgrades safely

The persisted settings MUST NOT embed a list of gateway keys. Downgrading to a single-key binary MUST NOT resurrect any credential that was revoked before the downgrade (the primary key is never revocable by disabling, so its legacy mirror is always intentionally active). Sub keys MUST survive a downgrade and re-upgrade round trip unchanged, because single-key binaries never read or rewrite the sub key store.

#### Scenario: Downgrade does not resurrect revoked sub keys

- **WHEN** a sub key was disabled or soft-deleted, the node runs an older single-key binary, and the current binary is restored
- **THEN** the sub key remains disabled or deleted and never authenticates on either binary

#### Scenario: Sub keys survive a downgrade round trip

- **WHEN** the node runs an older single-key binary that saves settings, then returns to the current binary
- **THEN** all sub keys and their states are intact

### Requirement: Key mutations are concurrency-safe and audited

Every key lifecycle mutation MUST serialize under the settings update lock, honor an optional expected-revision check that rejects stale writers with 409 before any change applies, and write an audit log entry identifying the affected key (including the previous name on rename).

#### Scenario: Stale revision is rejected

- **WHEN** a key mutation carries an expected revision that differs from the current settings revision
- **THEN** the gateway responds 409 and the key state is unchanged

#### Scenario: Rename records old and new names

- **WHEN** a sub key is renamed
- **THEN** the audit entry names both the previous and the new name
