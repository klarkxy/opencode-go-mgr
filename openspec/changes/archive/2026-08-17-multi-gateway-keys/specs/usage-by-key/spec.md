## Purpose

Records which gateway key served each forwarded request and lets users query and aggregate usage per key, including safe backfill of historical logs.

## ADDED Requirements

### Requirement: Forwarded requests record the client key
Every authenticated, forwarded request SHALL be recorded with the identifier of the gateway key that authenticated it. Unauthenticated requests SHALL be rejected with HTTP 401 and SHALL NOT create forward usage rows.

#### Scenario: Request attributed to its key
- **WHEN** a client authenticates with key A and the gateway forwards a request
- **THEN** the corresponding forward log records key A as the client key

#### Scenario: Unauthenticated request not logged
- **WHEN** a request presents no valid key
- **THEN** the gateway returns HTTP 401 and writes no forward usage row for it

### Requirement: Forward logs can be filtered by key
The forward log query SHALL accept an optional key filter. When provided, the returned rows and their summary totals (request count, token counts, cost) SHALL include only logs recorded for that key.

#### Scenario: Filter by one key
- **WHEN** a user queries forward logs filtered by key A
- **THEN** the result contains only logs attributed to key A and the summary reflects only those logs

#### Scenario: No filter returns all
- **WHEN** a user queries forward logs without a key filter
- **THEN** the result includes logs for every key (and unattributed logs), with a combined summary

### Requirement: Historical logs are attributed to the primary key
Forward logs written before multi-key support SHALL be attributed to the primary key once the upgrade completes. Until attributed, such logs SHALL remain visible and queryable under a distinct "unattributed" option and SHALL NOT be lost. Attribution SHALL be idempotent and resumable, and the gateway SHALL remain available while attribution runs.

#### Scenario: Upgrade with historical logs
- **WHEN** the app is upgraded and historical logs exist
- **THEN** once backfill completes, those logs are attributed to the primary key and appear under the primary key in key-filtered queries

#### Scenario: Attribution interrupted and resumed
- **WHEN** backfill of a large log table is interrupted and the app restarts
- **THEN** attribution resumes from the remaining unattributed logs without double-counting already attributed rows

#### Scenario: Gateway available during backfill
- **WHEN** backfill is running on a large log table
- **THEN** the gateway continues to accept and forward requests, and unattributed rows remain visible under the "unattributed" filter
