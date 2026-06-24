//! Schema version tracking. The `meta` table holds app/index bookkeeping —
//! settings live in the `settings` table so a settings reset never touches
//! index bookkeeping (per spec §9.2).

/// Current schema version. Bumped on each backward-incompatible migration.
pub const SCHEMA_VERSION: i64 = 2;
