# NDDev OpenNetwork · gds

AGPL-3.0-only NDS adapter. [NDDev OpenNetwork](https://nddev.ai).

Read GDS status for an explicitly selected repository. Native classifications, unproven scope and unknown remote freshness remain visible; no ref refresh or Git mutation.

The pinned native provider supports Linux/macOS. The existing Windows Rust API
is retained for consumers, but status returns unavailable without starting a
process. Portable DTOs do not imply a working native provider on other platforms.

The private decoder validates the pinned status envelope, outcome/exit pairing
and evidence shape. A successful local query still carries unknown remote
freshness; it does not establish GitHub consistency. Public presentation DTOs
remain compatible. Product proof/freshness rules belong to NDS core.

`contracts/module.json` is the canonical compiled-in runtime descriptor;
`module.yaml` identifies its repository package. Caller-owned local/process
capability handles share a bounded agent admission budget. Native responses,
private paths and content are never Debug or telemetry payloads.

Run `just check`, then `just provider-check` with the explicit `NDS_TEST_*`
provider paths used by CI. Acceptance exercises the real pinned native owner
in isolated temporary state. Linux provider acceptance and portable source
checks are distinct; an absent provider is unavailable, not a successful stub.
There is no backup/recovery implementation or private estate configuration.
