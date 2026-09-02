## Purpose

Defines per-client-key usage attribution on forward logs: which credential each forwarded request used, how logs are filtered and summarized by key, how historical rows are backfilled to the primary key, and how the key filter list resolves the latest name for each key.

## ADDED Requirements

### Requirement: Forwarded requests record the authenticating credential

Every authenticated forwarded request MUST record the id and a name snapshot of the credential that authenticated it. The primary key MUST be recorded under a fixed hardcoded UUID identifier constant whose value is stable from release onwards and may change only through an explicit migration that updates all historical rows; its name snapshot is the fixed display name "Primary". Unauthenticated requests MUST NOT produce forward log rows.

#### Scenario: Primary key requests attribute to the fixed primary identifier

- **WHEN** a request authenticates with the primary key
- **THEN** the forward log row carries the primary key's hardcoded UUID identifier and the name "Primary"

#### Scenario: Sub key requests attribute to that sub key

- **WHEN** a request authenticates with an enabled sub key
- **THEN** the forward log row carries that sub key's id and its name at write time, and later renames do not alter the stored snapshot

#### Scenario: Unauthenticated requests leave no rows

- **WHEN** a request fails gateway authentication
- **THEN** no forward log row is written and no upstream call is made

### Requirement: Forward logs can be filtered and summarized by key

The forward log query MUST support filtering by client key id, including a special value selecting rows without attribution, and the request/token/cost summary MUST be computed over exactly the filtered scope.

#### Scenario: Filtering by a sub key returns only its rows

- **WHEN** the log view filters on a sub key's id
- **THEN** only rows authenticated by that sub key are listed and the summary totals match those rows only

#### Scenario: Unattributed filter selects pre-backfill rows

- **WHEN** the filter is set to the unattributed special value
- **THEN** rows without a client key id are returned

### Requirement: Historical logs are attributed to the primary key

Rows written before per-key attribution existed MUST be backfilled to the primary key's hardcoded UUID identifier in bounded idempotent chunks that never block startup or request forwarding, resuming from a persisted watermark after interruption. Rows lacking attribution that appear after the backfill completed — for example during a downgrade window on a single-key binary — MUST be detected on the next startup without a full-table scan and re-attributed by restarting the scan, so no row stays unattributed permanently.

#### Scenario: Backfill attributes legacy rows to the primary key

- **WHEN** the node starts with historical rows lacking a client key id
- **THEN** those rows end up attributed to the primary key's fixed identifier without duplicate accounting

#### Scenario: Downgrade-window rows are re-attributed

- **WHEN** rows without a client key id are written after the backfill completed and the node restarts
- **THEN** the backfill restarts and attributes those rows to the primary key

### Requirement: The key filter list shows the latest name per key

The key list used by log filtering MUST return one entry per distinct client key id, resolving the most recent name snapshot for that id, so renamed keys appear under their current name rather than any historical one.

#### Scenario: Renamed key shows its current name

- **WHEN** a sub key was renamed after some requests were logged under its previous name
- **THEN** the key filter list shows exactly one entry for that key id bearing the latest name

#### Scenario: Primary key absent until its first logged request

- **WHEN** the node has no forward log rows attributed to the primary key
- **THEN** the key filter list omits the primary key entry; it appears once any forward log row carries the primary key's identifier
