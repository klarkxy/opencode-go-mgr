## Purpose

Manages multiple client-facing gateway keys with independent naming, lifecycle, and authentication, replacing the single-key model without breaking existing clients.

## ADDED Requirements

### Requirement: Multiple gateway keys can be configured
The system SHALL support more than one concurrent gateway key. Each key SHALL have a unique identifier, a display name, a key value, and an enabled state. Key values among non-deleted keys SHALL be unique.

#### Scenario: Add a second key
- **WHEN** a user creates a second gateway key with a display name
- **THEN** the system persists the new key alongside the existing one, and both keys appear in the key list with their names

#### Scenario: Duplicate key value rejected
- **WHEN** a user creates or regenerates a key whose value equals another non-deleted key's value
- **THEN** the system rejects the operation and reports a duplicate-key error

#### Scenario: Settings update cannot clear the key list
- **WHEN** a client submits a settings update whose JSON lacks the gateway key list
- **THEN** the system preserves the existing key configuration unchanged

### Requirement: Legacy single key migrates seamlessly
On upgrade from a version with a single gateway key, the system SHALL convert the existing key into the primary entry of the key list. Clients using the old key SHALL continue to authenticate without reconfiguration.

#### Scenario: Upgrade with an existing key
- **WHEN** the app starts on a database whose config holds a single legacy gateway key
- **THEN** that key becomes the primary key in the new list, remains accepted for authentication, and is returned by management APIs

#### Scenario: Upgrade with no key
- **WHEN** the app starts on a database with no legacy gateway key
- **THEN** the system generates one primary key so the gateway never runs without a usable credential

### Requirement: Authentication accepts any enabled key
The gateway SHALL accept a request when its presented credential matches any enabled, non-deleted key, and SHALL reject it with HTTP 401 when it matches none. Matching SHALL apply to the standard bearer token and to the API-key headers accepted today.

#### Scenario: Request with any enabled key
- **WHEN** a client authenticates with the value of any enabled key
- **THEN** the gateway processes the request normally

#### Scenario: Disabled key rejected
- **WHEN** a client authenticates with a key that has been disabled
- **THEN** the gateway rejects the request with HTTP 401 and does not forward it

#### Scenario: Deleted key rejected
- **WHEN** a client authenticates with a key that has been deleted
- **THEN** the gateway rejects the request with HTTP 401

#### Scenario: Unknown key rejected
- **WHEN** a client authenticates with a value matching no key
- **THEN** the gateway rejects the request with HTTP 401

### Requirement: Key lifecycle management
The system SHALL provide dashboard operations to create, rename, enable, disable, regenerate, and delete a gateway key, and SHALL record an audit entry in the gateway log for each key change. Deleting a key SHALL be a soft delete: the record's identifier and name are retained for attribution, the key value is cleared, and the key stops authenticating immediately. The last enabled key SHALL NOT be disabled or deleted; at least one enabled key MUST remain. Regenerating a key SHALL assign a new unique value and invalidate the previous value immediately.

#### Scenario: Rename a key
- **WHEN** a user renames a key
- **THEN** the new name is persisted and shown in all key lists

#### Scenario: Disable a key
- **WHEN** a user disables a key
- **THEN** the key stops authenticating immediately while remaining visible in the management list

#### Scenario: Delete a key
- **WHEN** a user deletes a key
- **THEN** the key stops authenticating immediately, disappears from the active management list, and its record is retained for log attribution

#### Scenario: Regenerate a key
- **WHEN** a user regenerates a key
- **THEN** the key receives a new unique value, the old value stops authenticating immediately, and the new value is returned to the user

#### Scenario: Last enabled key protected
- **WHEN** a user attempts to disable or delete the only remaining enabled key
- **THEN** the system rejects the operation with an explanatory error

#### Scenario: Primary key deleted promotes a successor
- **WHEN** a user deletes the primary key while other enabled keys exist
- **THEN** the earliest remaining enabled key becomes the new primary, the primary key's value mirror updates, and historical logs still resolve to the deleted key's record

### Requirement: Deleted keys retain attribution
The system SHALL keep soft-deleted key records so that usage logs written while the key was active remain resolvable to that key's name.

#### Scenario: Logs stay attributable after deletion
- **WHEN** a user views usage logs for a key that was later deleted
- **THEN** the logs still display the deleted key's name instead of an unknown placeholder
