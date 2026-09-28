# Legado core provenance

Derived from hadc188/Reader 1.4.6, local reference snapshot imported on 2026-09-28.
Upstream: https://github.com/hadc188/Reader
License: MIT; see LICENSE. The vendored PDF extractor retains its own license.

Imported modules: app, crawler, error, export, model, parser, service, storage, util.
Desktop IPC and frontend are not included. AppState is independent of Tauri.
The upstream parser, request semantics, migrations and PDF patches are retained.
Two upstream test files have three call-site corrections for the current interfaces.
The Tlegado workspace owns dependency patches and builds this crate without legado-example/.

Tlegado-specific change: BookSourceRepo::upsert_many and BookSourceService::save_many
save a validated import batch in one SQLite transaction. A trigger-based regression
test verifies rollback when an insert fails after an earlier successful insert.

The HTTP fetcher uses tracing debug events instead of raw terminal prints. These
events omit request URLs and bodies. A subprocess regression test runs GET, POST
and retried HTTP failures without output capture to check for terminal pollution.
