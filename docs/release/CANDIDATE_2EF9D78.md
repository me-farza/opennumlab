# Frozen local alpha candidate 2ef9d78

Candidate source: `2ef9d7806bac939114580fb634f95aad5b33d3f8`. This includes the authentic-reference importer `5dafc1c`, research packet `5746efc`, reviewed installed QA and its alias/tool-identity follow-up, exact source allowlists, native CI gate and report-directory fix.

A new detached local clone under `/private/tmp` is clean and points to that exact commit. No Cargo output was copied. The shared developer checkout remains on its existing branch; concurrent CP02 changes are outside this candidate.

The actual fixed-commit export contains 155 selected source files and matched its complete inventory. Its digest is `44907d20a58f2359613bc1365bb885776561fa6064d9eb0933fde14e56fcd21b`. Required QA files, root plans, canonical PNG and importer are included; raw research, MATLAB MAT/SLX data, vendor/runtime binaries and local artifacts remain excluded. Internal generated records are under `dist/candidate-2ef9d7806bac/`, including `freeze.json` with exact checkout and input identities.

Independent read-only review verified all 155 source hashes/sizes/modes and the export digest. All 56 retained notice references resolve and match their recorded hashes; CLI, engine, native, include and helper build inputs are present. The selected source contains no raw research/history/artifact payloads or user-home/chat/MATLAB-install references. No bounded export blocker was found. This is source-export evidence only: final build notices, native ABI and installed workflows remain unqualified. Only the allowlisted export passed this review; the detached clone and its Git history remain internal, and nothing was published.

Cargo registry sources, Apple SDK and external numerical/runtime dependencies remain separate prerequisites. The current packaging command requires a committed Git checkout. A future standalone source archive needs an export-specific recipe that preserves build identity; this audit does not establish such an archive as independently packageable.

The first attempt at importer revision `5dafc1c` exposed a source-manifest ordering defect. Every exported record matched by path/hash/size/permission; complete-name ordering differed from `Path` component ordering for a module file beside its same-basename directory. Candidate `2ef9d78` fixes that defect. All 15 packaging tests passed, including the new regression (6.950 seconds). Earlier synthetic QA, source-native gate and notice preflights remain recorded separately; they are not tests of this candidate's executables.

| Gate | Actual status |
| --- | --- |
| Exact clean detached source | Passed |
| Fixed-commit source export | Passed, 155 files |
| Source-native ABI build/load and regression gate | Passed, 27 named tests, no skips |
| Additional frozen language targets and Rustfmt | Passed, 25 named tests, no skips; formatting check passed |
| Private release build and app assembly | Passed; new unsigned native app |
| Installed CLI acceptance | Passed, all 12 required CLI checks |
| Installed authentic-reference conformance replay | Executed: common incomplete; known-gap mismatch; strict acceptance rejected |
| Desktop, recovery, read-only Resources and relocation | Not run |
| Linux hosted CI | Not run; no remote repository |
| Whole release | Not qualified |

The earlier capacity block was cleared by PM's authorized cleanup and an explicit serial build reservation. On 2026-10-15 the exact frozen source passed its native gate, private release assembly and reviewed installed CLI suite in that order. No source/candidate revision changed. The new app is `dist/candidate-2ef9d7806bac/NumJinn-alpha-2ef9d7806bac-native.app`; its final checksum-list SHA-256 is `05357daa9af8b462a8a2f2bd1a9fc52e5a001dd96b46511f66fe8c8ecc344ba6`. See [actual run evidence](CANDIDATE_2EF9D78_RUN.md) for commands, process handles, hashes and scope limits. Existing apps and user data were preserved; nothing was published. The serial slot was explicitly handed back after all three stages completed.

The planned cold-build floor of roughly 2 GiB remains a conservative local preflight based on an earlier approximately 1.4 GiB developer debug footprint and reserves, not a measured peak requirement. Capacity and competing jobs were checked before these builds. Follow [CANDIDATE_RUN.md](CANDIDATE_RUN.md) for a later authorized repeat. Installed saved-reference replay now retains separate common/known-gap reports, original packet layout and hashes, raw-forward-local unsupported/incomplete status and the sparse-logical self-roundtrip limitation. Both strict children correctly exit one; this is executed comparison evidence, not complete conformance acceptance. GUI journeys remain explicitly deferred by the user.
