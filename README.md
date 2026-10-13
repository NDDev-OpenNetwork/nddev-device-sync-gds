# NDDev OpenNetwork · gds

AGPL-3.0-only NDS adapter. [NDDev OpenNetwork](https://nddev.ai).

Read local Git status for an explicitly selected repository through the bundled
Rust worker. Canonical core owns the reader port and classification. The existing
status DTO remains available; partial reads carry no clean projection. Cached
ref comparisons always report unknown remote freshness. The reader performs no
fetch, Git mutation, credential access or network request.

Linux worker reads run under a 512 MiB address-space limit, three CPU seconds,
bounded scan/history/object/output budgets and the shared ProcessIo deadline and
cancellation owner. Windows/macOS calls return unsupported before process I/O
until native resource enforcement and acceptance are qualified. Portable DTOs
do not imply runtime support.

`contracts/module.json` is the canonical compiled-in runtime descriptor;
`module.yaml` identifies its repository package. Caller-owned local/process
capability handles share a bounded agent admission budget. Worker responses,
private paths and content are never Debug or telemetry payloads.

Run `just check` and `just provider-check`. Linux acceptance starts the actual
Rust worker against isolated Git fixtures and verifies dirty/index/ref
preservation, incomplete submodule reads, allocation bounds and cancellation.
There is no backup/recovery implementation or private estate configuration.
