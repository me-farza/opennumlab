# OpenNumLab public roadmap

Build an open scientific desktop and CLI through independently verified releases, then close the advanced ecosystem and Rust-native execution goals. This plan contains **132 tasks, 50 advanced families and 12 engineering tracks**. MATLAB compatibility and performance improvements require their own measured evidence.

[roadmap.json](roadmap.json) contains the normalized task, dependency, profile and evidence data. This document and that file form a self-contained public plan. Learning links point to primary documentation; illustration links remain external references, not redistributed screenshots.

Jump to [release profiles](#release-profiles), [construction sequence](#execution-overlay), [engineering tasks](#tasks), [advanced designs](#advanced-families) or [learning references](#learning-sources).

<a id="evidence-policy"></a>

## Planning and validation status

Every task's current implementation is **unassessed**; source and installed validation are **not_run** in this export. Historical baseline tags describe the planning snapshot of **2026-10-14**. They can be stale and never become present completion labels automatically.

A positive claim needs an exact task/API/type/file/option scope, immutable public source revision, actual public-safe result and declared runtime/profile/target. Installed claims additionally identify the installed artifact. Reference observations describe the reference software; they do not establish OpenNumLab implementation passes. Source navigation and this document's static validation establish no behavioral result.

The JSON keeps historical baseline, current implementation, source validation, installed validation, reference validation, external validation, revisions, evidence and known limits separate. Missing results remain unassessed or not_run. Hardware, standard-conformance and qualification claims need their own exact external proof.

<a id="release-profiles"></a>

## Release profiles

Ship a narrower named profile when broader mandatory work is incomplete. An earlier Development alpha does not satisfy Core, Full, Advanced or Rust execution requirements. Rust-native execution remains part of the long-term objective; an earlier runtime-backed profile does not close that goal.

<a id="profile-development-alpha"></a>

### Development alpha

An explicitly declared implemented subset and its prerequisite closure; no fixed Core/Full completion claim.

Required seeds: None.

No fixed completed task set is inferred. Declare the selected tasks and resolve their prerequisites. Its gates are frozen source and exact advertised scope; applicable enabled regressions and conformance; fresh assembly and declared prerequisites; actual installed workflows on every advertised target; writable state and failure recovery; independent review; public source, notices and artifact identity. Target metadata and installed proof remain required; alpha scope does not automatically acquire full Q06–Q08 breadth.

<a id="profile-core-desktop"></a>

### Core Desktop

Complete core desktop, graphics/data, tutorials, runtime and source/lifecycle delivery contracts.

Required seeds: [R01](#task-r01), [R02](#task-r02), [R03](#task-r03), [R04](#task-r04), [R05](#task-r05), [R06](#task-r06), [K01](#task-k01), [K02](#task-k02), [K03](#task-k03), [K04](#task-k04), [K05](#task-k05), [K06](#task-k06), [U01](#task-u01), [U02](#task-u02), [U03](#task-u03), [U04](#task-u04), [U05](#task-u05), [U06](#task-u06), [E01](#task-e01), [E02](#task-e02), [D01](#task-d01), [D02](#task-d02), [D03](#task-d03), [G01](#task-g01), [G02](#task-g02), [G03](#task-g03), [G04](#task-g04), [G05](#task-g05), [V01](#task-v01), [V02](#task-v02), [V03](#task-v03), [C01](#task-c01), [C02](#task-c02), [C03](#task-c03), [C04](#task-c04), [C05](#task-c05), [C06](#task-c06), [C07](#task-c07), [C08](#task-c08), [P01](#task-p01), [A01](#task-a01), [X01](#task-x01), [X02](#task-x02), [Q01](#task-q01), [Q02](#task-q02), [Q03](#task-q03), [Q04](#task-q04), [Q05](#task-q05), [Q09](#task-q09), [Q10](#task-q10), [Q11](#task-q11), [Q12](#task-q12).

The common prerequisite closure contains 52 tasks; selecting one advertised platform adds its gate, giving 53. These are requirement counts, not completion counts.

<a id="profile-full"></a>

### Full

Core plus scientific profiles, documents/apps, declared compute and simulation/import subset.

Required seeds: [R01](#task-r01), [R02](#task-r02), [R03](#task-r03), [R04](#task-r04), [R05](#task-r05), [R06](#task-r06), [K01](#task-k01), [K02](#task-k02), [K03](#task-k03), [K04](#task-k04), [K05](#task-k05), [K06](#task-k06), [U01](#task-u01), [U02](#task-u02), [U03](#task-u03), [U04](#task-u04), [U05](#task-u05), [U06](#task-u06), [E01](#task-e01), [E02](#task-e02), [E03](#task-e03), [D01](#task-d01), [D02](#task-d02), [D03](#task-d03), [G01](#task-g01), [G02](#task-g02), [G03](#task-g03), [G04](#task-g04), [G05](#task-g05), [V01](#task-v01), [V02](#task-v02), [V03](#task-v03), [C01](#task-c01), [C02](#task-c02), [C03](#task-c03), [C04](#task-c04), [C05](#task-c05), [C06](#task-c06), [C07](#task-c07), [C08](#task-c08), [P01](#task-p01), [P02](#task-p02), [P03](#task-p03), [P04](#task-p04), [P05](#task-p05), [P06](#task-p06), [P07](#task-p07), [A01](#task-a01), [A02](#task-a02), [A03](#task-a03), [A04](#task-a04), [A05](#task-a05), [H01](#task-h01), [H02](#task-h02), [H03](#task-h03), [X01](#task-x01), [X02](#task-x02), [X03](#task-x03), [B01](#task-b01), [B02](#task-b02), [B03](#task-b03), [B04](#task-b04), [Q01](#task-q01), [Q02](#task-q02), [Q03](#task-q03), [Q04](#task-q04), [Q05](#task-q05), [Q09](#task-q09), [Q10](#task-q10), [Q11](#task-q11), [Q12](#task-q12).

The common prerequisite closure contains 71 tasks; selecting one advertised platform adds its gate, giving 72. These are requirement counts, not completion counts.

<a id="profile-rust-execution"></a>

### Rust execution edition

Add Rust execution contracts to the explicitly selected product profile; numerical/performance proof remains separate.

Required seeds: [N01](#task-n01), [N02](#task-n02), [N03](#task-n03).

Add these tasks to an explicit Development alpha, Core, Full or Advanced base selection and its applicable release/target gates. N01–N03 alone are not a release profile. Advanced selections still require their AF/API variants; alpha still needs a declared scope and installed-target evidence. Preserve engine identity and compare the same semantic/numerical workloads before changing defaults.

<a id="profile-advanced-capability"></a>

### Advanced capability edition

Expand selected AF variants, required AF/J/foundation contracts and enabled conditions, then union common/target release gates.

Required seeds: [Q01](#task-q01), [Q02](#task-q02), [Q03](#task-q03), [Q04](#task-q04), [Q05](#task-q05), [Q09](#task-q09), [Q10](#task-q10), [Q11](#task-q11), [Q12](#task-q12).

Select exact AF/API/type/file/device variants. Expand their delivery selections and required/activated dependencies before unioning common release and selected platform gates. Required source, installed tutorial and claimed external-target evidence still gate delivery. All 50 families remain in the product scope even when a release selects fewer.

Selected platform gates: macos: [Q06](#task-q06), windows: [Q07](#task-q07), linux: [Q08](#task-q08). Q11 depends only on the selected target gates. Optional additions are remote services (H04), physical models (B05), specialist domains (Z01–Z03) and Rust execution (N01–N03), each with prerequisite and evidence closure.

Full retains live documents, user-app construction, declared parallel/GPU behavior and simulation/import work. Complete distributions include desktop/CLI, runtime or explicit prerequisite variant, native adapters, pinned packages, original offline tutorials, corresponding source, notices, artifact identities and installation/upgrade/recovery evidence. A self-contained claim requires execution without an undeclared preinstalled runtime.

<a id="execution-overlay"></a>

## First construction slice

CP01–CP06 organize the first integrated slice; they do not cover the complete product or replace the task/profile graph. Dependency-ready release infrastructure can prepare earlier, while final package acceptance retains all its gates.

| Package | Focus | Preparation after | Acceptance requires |
| --- | --- | --- | --- |
| [CP01](#cp01) | Freeze source and connect three-route conformance | None | None |
| [CP02](#cp02) | Capabilities and value revision envelope | [CP01](#cp01) | [CP01](#cp01) |
| [CP03](#cp03) | Lossless N-D pages and transactional edits | [CP02](#cp02) | [CP02](#cp02) |
| [CP04](#cp04) | Live figure objects and exact interaction | [CP02](#cp02) | [CP02](#cp02), [CP03](#cp03) |
| [CP05](#cp05) | Selected debugger frames and source lifetime | [CP02](#cp02) | [CP02](#cp02) |
| [CP06](#cp06) | Identified installed Core candidate | [CP03](#cp03), [CP04](#cp04), [CP05](#cp05) | [CP03](#cp03), [CP04](#cp04), [CP05](#cp05) |

<a id="cp01"></a>

### CP01: Freeze source and connect three-route conformance

Task mapping: [R01](#task-r01), [R02](#task-r02), [R03](#task-r03), [R06](#task-r06), [Q01](#task-q01). Independent prerequisites may follow the task dependency graph.

<a id="cp02"></a>

### CP02: Capabilities and value revision envelope

Task mapping: [K01](#task-k01), [K03](#task-k03), [K05](#task-k05), [K06](#task-k06). Independent prerequisites may follow the task dependency graph.

<a id="cp03"></a>

### CP03: Lossless N-D pages and transactional edits

Task mapping: [V01](#task-v01), [V02](#task-v02), [V03](#task-v03), [U04](#task-u04), [C01](#task-c01), [C04](#task-c04). Independent prerequisites may follow the task dependency graph.

<a id="cp04"></a>

### CP04: Live figure objects and exact interaction

Task mapping: [R05](#task-r05), [G01](#task-g01), [G02](#task-g02), [G03](#task-g03), [G04](#task-g04), [G05](#task-g05). Bounded graphics feasibility only; final acceptance also needs CP03.

<a id="cp05"></a>

### CP05: Selected debugger frames and source lifetime

Task mapping: [D01](#task-d01), [D02](#task-d02), [D03](#task-d03), [K04](#task-k04), [K05](#task-k05). Independent prerequisites may follow the task dependency graph.

<a id="cp06"></a>

### CP06: Identified installed Core candidate

Task mapping: [P01](#task-p01), [X01](#task-x01), [X02](#task-x02), [Q02](#task-q02), [Q03](#task-q03), [Q04](#task-q04), [Q05](#task-q05), [Q06](#task-q06), [Q09](#task-q09), [Q10](#task-q10), [Q11](#task-q11), [Q12](#task-q12). Independent prerequisites may follow the task dependency graph.

Acceptance additionally requires the complete selected release-profile closure, actual installed journeys and artifact/source identity. Finishing the package list alone is insufficient.

<a id="dependency-rules"></a>

## Resolving requirements

1. Start from the selected product profile and its explicit feature scope.
2. Expand advertised advanced delivery selections, then required AF contracts and activated conditional AF/foundation contracts. Match conditions by exact feature ID and condition name.
3. Union their J/foundation tasks, selected platform gates and enabled optional programs. Resolve ordinary task dependencies recursively; Q11 activates only selected Q06/Q07/Q08 gates.
4. Require the completed source, installed and claimed external evidence for that exact profile. A resolved dependency set is not a readiness verdict.

AF35 processor-in-the-loop delivery selects AF37 and target-execution evidence before closure. This is a delivery selection, not a J35→J37 implementation edge: J37 already requires J35. AF23 CPU training does not acquire GPU dependencies unless its GPU training variant is selected. Machine-readable examples preserve these distinctions.

<a id="tasks"></a>

## Engineering tasks

P0–P3 preserve planning priority; S/M/L/XL are relative effort labels, not dates. Each acceptance paragraph is work to implement and verify. Current implementation is unassessed and source/installed results are not_run for every task below.

<a id="milestone-m0"></a>

### M0: Evidence and declared scope

<a id="task-r01"></a>

#### R01 — Freeze API type option and platform registry

Priority P0; relative size M; discipline Compatibility. Historical baseline: partial (2026-10-14).

Prerequisites: None.

Acceptance: Every current claim links to exact fixture and runtime; unavailable and untested states are distinct

Learning references: [S01](#source-s01), [S19](#source-s19), [S20](#source-s20).

Source navigation (not completion evidence): [compatibility/README.md](compatibility/README.md), [FUNCTIONALITY_CATALOG.md](FUNCTIONALITY_CATALOG.md).

<a id="task-r02"></a>

#### R02 — Record reproducible runtime package toolkit adapter profile

Priority P0; relative size S; discipline Release. Historical baseline: partial (2026-10-14).

Prerequisites: [R01](#task-r01).

Acceptance: A clean profile records engine ABI package versions platform and build inputs

Learning references: [D14](#source-d14), [D15](#source-d15), [S20](#source-s20).

Source navigation (not completion evidence): [native/README.md](native/README.md).

<a id="task-r03"></a>

#### R03 — Expand independent semantic and file oracles

Priority P0; relative size M; discipline Numerics. Historical baseline: partial (2026-10-14).

Prerequisites: [R01](#task-r01).

Acceptance: Dense sparse complex empty indexing class closures and MAT cases have independent expected results; reference access is explicit

Learning references: [S02](#source-s02).

Source navigation (not completion evidence): [compatibility/README.md](compatibility/README.md).

<a id="task-r04"></a>

#### R04 — Refresh actual baseline proof scenes

Priority P1; relative size S; discipline Desktop. Historical baseline: partial (2026-10-14).

Prerequisites: [R02](#task-r02).

Acceptance: Normal busy error variable and paused captures state exact binary source profile and date

Learning references: [D01](#source-d01), [D04](#source-d04).

<a id="task-r05"></a>

#### R05 — Prove graphics event and property route

Priority P0; relative size M; discipline Graphics. Historical baseline: partial (2026-10-14).

Prerequisites: [R02](#task-r02).

Acceptance: One live line and axes property round-trip plus click callback; compare native bridge and Rust scene feasibility

Learning references: [D16](#source-d16), [D18](#source-d18).

Source navigation (not completion evidence): [crates/mathlab-engine/src/lib.rs](crates/mathlab-engine/src/lib.rs), [octave/mathlab_worker.m](octave/mathlab_worker.m).

<a id="task-r06"></a>

#### R06 — Select core release subset and migration projects

Priority P1; relative size S; discipline Product. Historical baseline: missing (2026-10-14).

Prerequisites: [R01](#task-r01), [R04](#task-r04).

Acceptance: Named release reference and representative original projects approved internally; no percentage inferred from smoke tests

Learning references: [D01](#source-d01), [D02](#source-d02), [S19](#source-s19).

Source navigation (not completion evidence): [compatibility/README.md](compatibility/README.md).

<a id="milestone-m1"></a>

### M1: Dependable kernel

<a id="task-k01"></a>

#### K01 — Version ordered kernel events and job lifecycle

Priority P0; relative size L; discipline Kernel. Historical baseline: partial (2026-10-14).

Prerequisites: [R01](#task-r01), [R02](#task-r02).

Acceptance: Long job streams ordered output before completion; job IDs epochs sequence and source revisions reject stale events

Learning references: [ER02](#source-er02).

Source navigation (not completion evidence): [crates/mathlab-engine/src/lib.rs](crates/mathlab-engine/src/lib.rs), [crates/mathlab-desktop/src/worker.rs](crates/mathlab-desktop/src/worker.rs), [octave/mathlab_worker.m](octave/mathlab_worker.m).

<a id="task-k02"></a>

#### K02 — Implement input prompt and reply flow

Priority P0; relative size M; discipline Kernel. Historical baseline: missing (2026-10-14).

Prerequisites: [K01](#task-k01).

Acceptance: User can answer input() and cancel pending prompt; multiline replies never enter an unrelated job

Learning references: [D01](#source-d01).

Source navigation (not completion evidence): [crates/mathlab-engine/src/lib.rs](crates/mathlab-engine/src/lib.rs), [crates/mathlab-desktop/src/worker.rs](crates/mathlab-desktop/src/worker.rs).

<a id="task-k03"></a>

#### K03 — Specify interruption restart and process cleanup

Priority P0; relative size M; discipline Kernel. Historical baseline: partial (2026-10-14).

Prerequisites: [K01](#task-k01), [K02](#task-k02).

Acceptance: Loop interruption retains workspace where supported; forced restart discards it visibly; children follow declared cleanup policy

Learning references: [D01](#source-d01).

Source navigation (not completion evidence): [crates/mathlab-engine/src/lib.rs](crates/mathlab-engine/src/lib.rs).

<a id="task-k04"></a>

#### K04 — Close source identity and registration boundaries

Priority P0; relative size L; discipline Language. Historical baseline: partial (2026-10-14).

Prerequisites: [R03](#task-r03).

Acceptance: User identifier collision and original filename/resource reflection fixtures pass in supported modes; coordinate active implementation

Learning references: [D04](#source-d04), [D17](#source-d17), [ER02](#source-er02).

Source navigation (not completion evidence): [crates/mathlab-language/src/lib.rs](crates/mathlab-language/src/lib.rs), [native/README.md](native/README.md).

<a id="task-k05"></a>

#### K05 — Budget execution inspection and retained source artifacts

Priority P0; relative size M; discipline Kernel. Historical baseline: missing (2026-10-14).

Prerequisites: [K01](#task-k01), [K04](#task-k04).

Acceptance: Memory/time limits and telemetry are explicit; live handles retain required generations; unreferenced generations and old previews are reclaimed

Learning references: [ER02](#source-er02).

Source navigation (not completion evidence): [crates/mathlab-engine/src/lib.rs](crates/mathlab-engine/src/lib.rs).

<a id="task-k06"></a>

#### K06 — Enable native adapter platform verification

Priority P0; relative size L; discipline Release. Historical baseline: partial (2026-10-14).

Prerequisites: [R02](#task-r02), [K03](#task-k03), [K04](#task-k04).

Acceptance: Supported OS jobs execute enabled native tests with matching SDK; a skipped adapter test cannot satisfy its gate

Learning references: [D17](#source-d17), [S02](#source-s02).

Source navigation (not completion evidence): [native/README.md](native/README.md), [scripts/verify.sh](scripts/verify.sh).

<a id="milestone-m2"></a>

### M2: Daily programming desktop

<a id="task-u01"></a>

#### U01 — Add layout tree tiling named layouts and child windows

Priority P1; relative size L; discipline Desktop. Historical baseline: partial (2026-10-14).

Prerequisites: [K01](#task-k01).

Acceptance: Move tile detach reset and restore panels without duplicate documents; disconnected monitors recover

Learning references: [D02](#source-d02), [D14](#source-d14), [D15](#source-d15).

Source navigation (not completion evidence): [crates/mathlab-desktop/src/lib.rs](crates/mathlab-desktop/src/lib.rs), [crates/mathlab-desktop/src/session.rs](crates/mathlab-desktop/src/session.rs).

<a id="task-u02"></a>

#### U02 — Add multiline commands completion and bounded transcript

Priority P0; relative size M; discipline Desktop. Historical baseline: partial (2026-10-14).

Prerequisites: [K01](#task-k01), [K02](#task-k02).

Acceptance: Paste a multiline block; recall prefix history; receive streaming text; transcript copy works during output; memory is bounded

Learning references: [D01](#source-d01).

Source navigation (not completion evidence): [crates/mathlab-desktop/src/session.rs](crates/mathlab-desktop/src/session.rs), [crates/mathlab-desktop/src/search.rs](crates/mathlab-desktop/src/search.rs).

<a id="task-u03"></a>

#### U03 — Complete Current Folder actions and search

Priority P1; relative size M; discipline Desktop. Historical baseline: partial (2026-10-14).

Prerequisites: [K01](#task-k01).

Acceptance: Create rename open filter and trash a file; source tabs follow renamed paths; failed mutations preserve original files

Learning references: [D12](#source-d12).

Source navigation (not completion evidence): [crates/mathlab-desktop/src/source_navigation.rs](crates/mathlab-desktop/src/source_navigation.rs).

<a id="task-u04"></a>

#### U04 — Add Workspace sort filter rename delete save selected

Priority P1; relative size M; discipline Desktop. Historical baseline: partial (2026-10-14).

Prerequisites: [K01](#task-k01).

Acceptance: Selected variable action updates correct kernel binding; canceled/failed operations preserve values

Learning references: [D03](#source-d03).

<a id="task-u05"></a>

#### U05 — Define portable project manifest and startup lifecycle

Priority P1; relative size M; discipline Desktop. Historical baseline: partial (2026-10-14).

Prerequisites: [R02](#task-r02), [U03](#task-u03).

Acceptance: Relocated project opens with relative paths and missing dependencies reported; source recovery stays separate from workspace snapshots

Learning references: [D12](#source-d12), [D15](#source-d15).

Source navigation (not completion evidence): [crates/mathlab-desktop/src/session.rs](crates/mathlab-desktop/src/session.rs).

<a id="task-u06"></a>

#### U06 — Implement focus scaling and accessibility review

Priority P1; relative size M; discipline Desktop. Historical baseline: partial (2026-10-14).

Prerequisites: [U01](#task-u01), [U02](#task-u02).

Acceptance: Keyboard completes open edit run inspect and stop; focus visible; status readable without color; essential controls survive supported scaling

Learning references: [D01](#source-d01), [D02](#source-d02).

Source navigation (not completion evidence): [crates/mathlab-desktop/src/lib.rs](crates/mathlab-desktop/src/lib.rs).

<a id="task-e01"></a>

#### E01 — Integrate revisioned syntax diagnostics and completion

Priority P1; relative size L; discipline Language. Historical baseline: partial (2026-10-14).

Prerequisites: [K04](#task-k04).

Acceptance: Delayed old diagnostics ignored; Unicode positions correct; documented language service capability subset works on dirty source

Learning references: [ER01](#source-er01).

Source navigation (not completion evidence): [crates/mathlab-language/src/lib.rs](crates/mathlab-language/src/lib.rs).

<a id="task-e02"></a>

#### E02 — Add autoindent folding signature help and symbol navigation

Priority P1; relative size M; discipline Desktop. Historical baseline: partial (2026-10-14).

Prerequisites: [E01](#task-e01).

Acceptance: Edits preserve text and undo; folded lines retain breakpoint/source mapping; signature help follows active call

Learning references: [ER01](#source-er01).

Source navigation (not completion evidence): [crates/mathlab-desktop/src/syntax.rs](crates/mathlab-desktop/src/syntax.rs), [crates/mathlab-desktop/src/search.rs](crates/mathlab-desktop/src/search.rs).

<a id="task-e03"></a>

#### E03 — Add references and reviewable rename

Priority P2; relative size L; discipline Language. Historical baseline: missing (2026-10-14).

Prerequisites: [E01](#task-e01), [U03](#task-u03).

Acceptance: Preview edits across original project files; cancel is harmless; rename does not modify strings or unrelated same-name symbols

Learning references: [ER01](#source-er01).

<a id="task-d01"></a>

#### D01 — Add condition enabled state and resolved breakpoint markers

Priority P0; relative size M; discipline Debugger. Historical baseline: partial (2026-10-14).

Prerequisites: [K04](#task-k04).

Acceptance: Condition uses correct scope; unresolved line shown; source edits rebind intent; unsupported built-ins do not appear bound

Learning references: [D04](#source-d04), [D17](#source-d17).

Source navigation (not completion evidence): [crates/mathlab-desktop/src/debugging.rs](crates/mathlab-desktop/src/debugging.rs), [crates/mathlab-desktop/src/prepared_debug.rs](crates/mathlab-desktop/src/prepared_debug.rs), [native/README.md](native/README.md).

<a id="task-d02"></a>

#### D02 — Complete stack selection watches and paused evaluation

Priority P0; relative size M; discipline Debugger. Historical baseline: partial (2026-10-14).

Prerequisites: [D01](#task-d01), [K01](#task-k01).

Acceptance: Caller frame locals/evaluation agree; watch failure does not resume; scoped evaluation remains responsive to Continue and Quit

Learning references: [D04](#source-d04), [D17](#source-d17).

Source navigation (not completion evidence): [crates/mathlab-debugger/src/lib.rs](crates/mathlab-debugger/src/lib.rs).

<a id="task-d03"></a>

#### D03 — Add supported pause on error and diagnostic recovery

Priority P1; relative size M; discipline Debugger. Historical baseline: partial (2026-10-14).

Prerequisites: [D02](#task-d02), [K06](#task-k06).

Acceptance: Named supported event modes pause at original source; parse error and Quit leave usable session; ABI errors appear before running

Learning references: [D17](#source-d17).

Source navigation (not completion evidence): [crates/mathlab-desktop/src/debugging.rs](crates/mathlab-desktop/src/debugging.rs), [native/README.md](native/README.md).

<a id="milestone-m3"></a>

### M3: Interactive graphics and scientific core

<a id="task-g01"></a>

#### G01 — Add typed revisioned graphics object service

Priority P0; relative size L; discipline Graphics. Historical baseline: missing (2026-10-14).

Prerequisites: [R05](#task-r05), [K01](#task-k01).

Acceptance: Figure axes line handles query without creating new objects; deleted/recycled handle refuses delayed mutation

Learning references: [D16](#source-d16), [D18](#source-d18), [ER02](#source-er02).

<a id="task-g02"></a>

#### G02 — Render live supported scenes and property inspector

Priority P0; relative size L; discipline Graphics. Historical baseline: missing (2026-10-14).

Prerequisites: [G01](#task-g01).

Acceptance: Select same object in tree/canvas; property change reaches script get(); code updates refresh same document

Learning references: [D05](#source-d05), [D16](#source-d16).

<a id="task-g03"></a>

#### G03 — Implement axis zoom pan restore and exact data tips

Priority P0; relative size M; discipline Graphics. Historical baseline: missing (2026-10-14).

Prerequisites: [G02](#task-g02).

Acceptance: Engine limits follow interactions; full-resolution sample coordinates remain correct under rendering decimation

Learning references: [D06](#source-d06).

<a id="task-g04"></a>

#### G04 — Add brushing selection and supported callbacks

Priority P1; relative size L; discipline Graphics. Historical baseline: missing (2026-10-14).

Prerequisites: [G03](#task-g03), [K03](#task-k03).

Acceptance: Selection exports correct original indices; click key close payloads tested; restart cancels old queued callbacks

Learning references: [D06](#source-d06), [D07](#source-d07), [D18](#source-d18).

<a id="task-g05"></a>

#### G05 — Extend export and supported scene persistence

Priority P1; relative size M; discipline Graphics. Historical baseline: partial (2026-10-14).

Prerequisites: [G02](#task-g02).

Acceptance: PNG and SVG labels/styles verified; saved supported scene restores object semantics; unsupported FIG variants report diagnostic

Learning references: [D05](#source-d05), [D06](#source-d06).

<a id="task-v01"></a>

#### V01 — Add lossless typed N-D slices and container paths

Priority P0; relative size M; discipline Data. Historical baseline: partial (2026-10-14).

Prerequisites: [K01](#task-k01).

Acceptance: Large sparse N-D complex and uint64 values inspect without full copy or precision loss; supported paths deterministic

Learning references: [D03](#source-d03), [S01](#source-s01).

Source navigation (not completion evidence): [crates/mathlab-desktop/src/variable_editor.rs](crates/mathlab-desktop/src/variable_editor.rs).

<a id="task-v02"></a>

#### V02 — Add range selection clipboard and transactional undo

Priority P1; relative size L; discipline Data. Historical baseline: partial (2026-10-14).

Prerequisites: [V01](#task-v01), [U04](#task-u04).

Acceptance: Paste preserves target class/shape; invalid/stale transaction makes no partial writes; committed patch can be undone

Learning references: [D03](#source-d03).

Source navigation (not completion evidence): [crates/mathlab-desktop/src/variable_editor.rs](crates/mathlab-desktop/src/variable_editor.rs).

<a id="task-v03"></a>

#### V03 — Add named-column table view metadata and summaries

Priority P1; relative size L; discipline Data. Historical baseline: missing (2026-10-14).

Prerequisites: [V01](#task-v01), [C01](#task-c01).

Acceptance: View sorting leaves source unchanged unless explicit operation; statistics report missing and sampling policy

Learning references: [D03](#source-d03), [S04](#source-s04), [S05](#source-s05).

<a id="task-c01"></a>

#### C01 — Validate modern string table categorical and time subset

Priority P0; relative size L; discipline Language. Historical baseline: partial (2026-10-14).

Prerequisites: [R03](#task-r03), [K04](#task-k04).

Acceptance: Pinned package candidate tested per class option and shape; timezone missing and equality semantics documented

Learning references: [S01](#source-s01), [S04](#source-s04), [S05](#source-s05), [S20](#source-s20).

Source navigation (not completion evidence): [compatibility/README.md](compatibility/README.md).

<a id="task-c02"></a>

#### C02 — Implement typed CSV TSV preview and generated import source

Priority P0; relative size L; discipline Data. Historical baseline: missing (2026-10-14).

Prerequisites: [V01](#task-v01).

Acceptance: Identifier text and ambiguous dates preserved; invalid cells visible; generated source reproduces chosen rows/types/missing policy

Learning references: [S03](#source-s03), [S04](#source-s04).

<a id="task-c03"></a>

#### C03 — Add declared spreadsheet and time import formats

Priority P1; relative size L; discipline Data. Historical baseline: missing (2026-10-14).

Prerequisites: [C02](#task-c02), [C01](#task-c01).

Acceptance: Named sheet/range and date timezone formats test with independent reader; malformed/truncated input cannot silently lose data

Learning references: [S03](#source-s03), [S05](#source-s05).

<a id="task-c04"></a>

#### C04 — Expand independently checked MAT and file interoperability

Priority P0; relative size L; discipline Data. Historical baseline: partial (2026-10-14).

Prerequisites: [R03](#task-r03), [V01](#task-v01).

Acceptance: Declared MAT versions/types read by independent reader and vice versa; unsupported object/live/app/model files are retained with diagnostics

Learning references: [S01](#source-s01).

Source navigation (not completion evidence): [compatibility/README.md](compatibility/README.md).

<a id="task-c05"></a>

#### C05 — Expand core algebra solve and factorization variants

Priority P0; relative size L; discipline Numerics. Historical baseline: partial (2026-10-14).

Prerequisites: [R03](#task-r03).

Acceptance: Known solves residuals QR/SVD/Cholesky reconstructions conditioning warnings and singular cases verified per type and option

Learning references: [S21](#source-s21).

Source navigation (not completion evidence): [compatibility/README.md](compatibility/README.md).

<a id="task-c06"></a>

#### C06 — Validate large sparse complex and precision behavior

Priority P1; relative size M; discipline Numerics. Historical baseline: partial (2026-10-14).

Prerequisites: [C05](#task-c05), [K05](#task-k05).

Acceptance: Sparse inputs avoid accidental densification; complex and single variants checked; peak memory and residual criteria reported separately

Learning references: [S21](#source-s21).

<a id="task-c07"></a>

#### C07 — Validate quadrature interpolation and error conventions

Priority P1; relative size M; discipline Numerics. Historical baseline: partial (2026-10-14).

Prerequisites: [R03](#task-r03).

Acceptance: Integral x^2 on 0..1 equals 1/3 within 1e-10; linear interpolation at 0.5 equals 1; extrapolation and malformed input explicit

Learning references: [S22](#source-s22).

<a id="task-c08"></a>

#### C08 — Validate script ODE stiff implicit and event subset

Priority P1; relative size L; discipline Numerics. Historical baseline: partial (2026-10-14).

Prerequisites: [R03](#task-r03), [G03](#task-g03).

Acceptance: Decay agrees with exp(-1) within 1e-6; upward terminal y-1 event for y prime=2 occurs at 0.5; solver warnings and consistency policy explicit

Learning references: [S22](#source-s22).

<a id="milestone-m4"></a>

### M4: Scientific package profiles

<a id="task-p01"></a>

#### P01 — Pin scientific package profiles and dependency policy

Priority P0; relative size M; discipline Release. Historical baseline: partial (2026-10-14).

Prerequisites: [R02](#task-r02), [U05](#task-u05).

Acceptance: Lockfile records exact versions hashes constraints notices; installation and current-session loading remain distinct

Learning references: [S20](#source-s20).

<a id="task-p02"></a>

#### P02 — Expand signal filter and spectral analysis workflow

Priority P1; relative size L; discipline Signal. Historical baseline: partial (2026-10-14).

Prerequisites: [P01](#task-p01), [G03](#task-g03).

Acceptance: Exact-bin FFT amplitudes correct; documented filter DC/cutoff and transients checked; peak width/prominence conventions explicit; fir1(20,0.25) has 21 symmetric coefficients and unit DC gain; generated source reproduces coefficients

Learning references: [S07](#source-s07), [S08](#source-s08).

<a id="task-p03"></a>

#### P03 — Expand transfer state-space and response analysis

Priority P1; relative size L; discipline Control. Historical baseline: partial (2026-10-14).

Prerequisites: [P01](#task-p01), [G03](#task-g03).

Acceptance: First-order response agrees with analytic trace; stability units and continuous/discrete sample time explicit

Learning references: [S09](#source-s09).

<a id="task-p04"></a>

#### P04 — Expand descriptive statistics regression and tests

Priority P1; relative size L; discipline Statistics. Historical baseline: partial (2026-10-14).

Prerequisites: [P01](#task-p01), [C01](#task-c01).

Acceptance: Known regression coefficients and fixed data test statistics correct; missing policies RNG conventions and diagnostics declared

Learning references: [S10](#source-s10).

<a id="task-p05"></a>

#### P05 — Expand optimization with convergence diagnostics

Priority P1; relative size L; discipline Optimization. Historical baseline: partial (2026-10-14).

Prerequisites: [P01](#task-p01).

Acceptance: Known quadratic optimum and least-squares fit recovered; infeasible/unconverged cases report status tolerances and evaluation counts

Learning references: [S11](#source-s11).

<a id="task-p06"></a>

#### P06 — Expand image pipelines and intermediate inspection

Priority P1; relative size L; discipline Image. Historical baseline: partial (2026-10-14).

Prerequisites: [P01](#task-p01), [G02](#task-g02).

Acceptance: Hand-authored binary connectivity/morphology correct; image dtype range channel order and borders preserved

Learning references: [S12](#source-s12).

<a id="task-p07"></a>

#### P07 — Add exact symbolic subset with declared backend

Priority P2; relative size L; discipline Symbolic. Historical baseline: missing (2026-10-14).

Prerequisites: [P01](#task-p01).

Acceptance: Polynomial derivatives and substitutions exact; branch assumptions and exact/float distinctions visible; backend not hidden

Learning references: [S13](#source-s13).

<a id="milestone-m5"></a>

### M5: Documents and application tools

<a id="task-a01"></a>

#### A01 — Add searchable licensed help and original executable tutorials

Priority P1; relative size M; discipline Documentation. Historical baseline: partial (2026-10-14).

Prerequisites: [P01](#task-p01), [U05](#task-u05).

Acceptance: Examples check prerequisites run against selected profile and surface assertions/errors; offline content has distribution rights

Learning references: [D01](#source-d01), [S02](#source-s02), [S20](#source-s20).

<a id="task-a02"></a>

#### A02 — Add user test runner and report browser

Priority P1; relative size M; discipline Desktop. Historical baseline: missing (2026-10-14).

Prerequisites: [U05](#task-u05), [K01](#task-k01).

Acceptance: Pass fail skip states and actual outputs link correct source revision; fixtures execute under named profile

Learning references: [S02](#source-s02).

<a id="task-a03"></a>

#### A03 — Add measured profiler and source view

Priority P2; relative size M; discipline Kernel. Historical baseline: missing (2026-10-14).

Prerequisites: [K05](#task-k05), [U05](#task-u05).

Acceptance: Measured function timing and call counts match native profile records; source links retain revision; any inclusive time derivation states recursion policy; export overhead separate

Learning references: [S23](#source-s23).

<a id="task-a04"></a>

#### A04 — Add open live documents with sections outputs and export

Priority P1; relative size XL; discipline Documents. Historical baseline: missing (2026-10-14).

Prerequisites: [E02](#task-e02), [G02](#task-g02), [A01](#task-a01).

Acceptance: Text/code/output round-trip; edit marks dependent results stale; section errors attach to correct cell; m export preserves code

Learning references: [D08](#source-d08).

<a id="task-a05"></a>

#### A05 — Build app controls callbacks and original visual designer

Priority P2; relative size XL; discipline Apps. Historical baseline: missing (2026-10-14).

Prerequisites: [E02](#task-e02), [G04](#task-g04).

Acceptance: Axes/slider app runs; rename preserves user callback source; layout undo and runtime error navigation work

Learning references: [D09](#source-d09), [D10](#source-d10), [D11](#source-d11), [D18](#source-d18).

<a id="milestone-m6"></a>

### M6: Compute and deployment

<a id="task-h01"></a>

#### H01 — Add isolated background jobs and process workers

Priority P1; relative size L; discipline Compute. Historical baseline: missing (2026-10-14).

Prerequisites: [K03](#task-k03), [P01](#task-p01).

Acceptance: Inputs outputs and dependencies explicit; fixed original map result equals serial; cancellation cleans worker processes

Learning references: [S14](#source-s14).

<a id="task-h02"></a>

#### H02 — Define selected parallel language/API subset

Priority P2; relative size XL; discipline Compute. Historical baseline: missing (2026-10-14).

Prerequisites: [H01](#task-h01), [R03](#task-r03).

Acceptance: Dependency restrictions and process-local state documented; reductions obey selected semantics; unsupported constructs diagnose

Learning references: [S14](#source-s14), [ER02](#source-er02).

<a id="task-h03"></a>

#### H03 — Add declared GPU array operations and device diagnostics

Priority P2; relative size XL; discipline GPU. Historical baseline: missing (2026-10-14).

Prerequisites: [R03](#task-r03), [P01](#task-p01).

Acceptance: Precision shape errors and residency agree with profile; compare CPU answers; unavailable device handled; transfer time measured

Learning references: [S15](#source-s15).

<a id="task-h04"></a>

#### H04 — Add authenticated reproducible remote jobs

Priority P2; relative size XL; discipline Compute. Historical baseline: missing (2026-10-14).

Prerequisites: [H01](#task-h01), [R02](#task-r02).

Acceptance: Artifact dependencies progress and cancellation checked; reconnect retrieves right job; server/session versions visible

Learning references: [S14](#source-s14).

<a id="task-x01"></a>

#### X01 — Make cross-platform app/runtime distributions reproducible

Priority P0; relative size L; discipline Release. Historical baseline: partial (2026-10-14).

Prerequisites: [R02](#task-r02), [K06](#task-k06).

Acceptance: Supported clean-machine launch and fixtures pass; external versus bundled runtimes explicit; complete dependency notices retained

Learning references: [RP01](#source-rp01), [RP03](#source-rp03), [RP04](#source-rp04), [RP05](#source-rp05), [RP07](#source-rp07), [RP08](#source-rp08).

Source navigation (not completion evidence): [scripts/package-macos.sh](scripts/package-macos.sh).

<a id="task-x02"></a>

#### X02 — Add platform signing install update and rollback

Priority P1; relative size L; discipline Release. Historical baseline: missing (2026-10-14).

Prerequisites: [X01](#task-x01).

Acceptance: Verified candidate installs starts and rolls back; preserve user projects; failed update cannot replace last usable app

Learning references: [RP04](#source-rp04), [RP06](#source-rp06), [RP07](#source-rp07).

<a id="task-x03"></a>

#### X03 — Package install run and remove supported user apps

Priority P2; relative size L; discipline Apps. Historical baseline: missing (2026-10-14).

Prerequisites: [A05](#task-a05), [P01](#task-p01), [X01](#task-x01).

Acceptance: Manifest/resources preview; relocatable app works on clean profile; missing dynamic resource produces diagnostic

Learning references: [D13](#source-d13).

<a id="milestone-m7"></a>

### M7: Simulation subset

<a id="task-b01"></a>

#### B01 — Build original block canvas and typed graph subset

Priority P1; relative size XL; discipline Simulation. Historical baseline: missing (2026-10-14).

Prerequisites: [U01](#task-u01), [G02](#task-g02).

Acceptance: Constant gain sum integrator delay ports and scope persist; invalid dimensions/ports and algebraic loops diagnose

Learning references: [S16](#source-s16), [S17](#source-s17).

<a id="task-b02"></a>

#### B02 — Implement continuous discrete solver and trace workflow

Priority P1; relative size XL; discipline Simulation. Historical baseline: missing (2026-10-14).

Prerequisites: [B01](#task-b01), [R03](#task-r03).

Acceptance: Gain-integrator trace equals 2t; tolerance refinement and discrete steps validated; cancellation retains valid trace status

Learning references: [S16](#source-s16), [S17](#source-s17).

<a id="task-b03"></a>

#### B03 — Add zero crossing events and hierarchical states

Priority P2; relative size XL; discipline Simulation. Historical baseline: missing (2026-10-14).

Prerequisites: [B02](#task-b02).

Acceptance: Upward terminal y-1 event with y prime=2 ends at t=0.5 within 1e-6; returns value 1; reset/restart and simultaneous-event ordering verified

Learning references: [S17](#source-s17), [S18](#source-s18), [S22](#source-s22).

<a id="task-b04"></a>

#### B04 — Import explicit supported simulation model subset

Priority P2; relative size XL; discipline Interoperability. Historical baseline: missing (2026-10-14).

Prerequisites: [B02](#task-b02), [C04](#task-c04).

Acceptance: Object inventory produced before conversion; unsupported blocks remain errors; imported supported traces match reference

Learning references: [S16](#source-s16), [S17](#source-s17).

<a id="task-b05"></a>

#### B05 — Specify physical models real-time and codegen expansion

Priority P3; relative size XL; discipline Simulation. Historical baseline: missing (2026-10-14).

Prerequisites: [B03](#task-b03), [B04](#task-b04).

Acceptance: Each expansion split into domain issues with solver consistency units deterministic scheduler and external target evidence

Learning references: [S19](#source-s19).

<a id="milestone-m8"></a>

### M8: Specialist domains

<a id="task-z01"></a>

#### Z01 — Inventory specialized scientific toolbox profiles

Priority P2; relative size XL; discipline Domain leads. Historical baseline: planned (2026-10-14).

Prerequisites: [R01](#task-r01), [P01](#task-p01).

Acceptance: Communications robotics vision ML finance biology geospatial PDE hardware and verification each have API owner and tutorial/migration inventory

Learning references: [S19](#source-s19), [S20](#source-s20).

Source navigation (not completion evidence): [FUNCTIONALITY_CATALOG.md](FUNCTIONALITY_CATALOG.md).

<a id="task-z02"></a>

#### Z02 — Implement independently versioned domain releases

Priority P3; relative size XL; discipline Domain leads. Historical baseline: planned (2026-10-14).

Prerequisites: [Z01](#task-z01), [H02](#task-h02), [B03](#task-b03).

Acceptance: Exact types algorithms file/model targets and domain correctness suite defined per release; no catalog-wide parity claim

Learning references: [S19](#source-s19).

<a id="task-z03"></a>

#### Z03 — Expand MEX language service hardware and deployment interoperability

Priority P3; relative size XL; discipline Interoperability. Historical baseline: planned (2026-10-14).

Prerequisites: [Z01](#task-z01), [C04](#task-c04), [X01](#task-x01).

Acceptance: Named ABI/file/protocol/hardware targets tested; unsupported proprietary services reported; notices and reproducibility complete

Learning references: [S19](#source-s19), [ER02](#source-er02).

<a id="milestone-mr"></a>

### MR: Rust execution engine

<a id="task-n01"></a>

#### N01 — Build full MATLAB-profile parser and semantic model

Priority P3; relative size XL; discipline Compiler. Historical baseline: missing (2026-10-14).

Prerequisites: [R03](#task-r03), [K04](#task-k04).

Acceptance: Column-major one-based indexing end transposes closures exceptions and type rules pass independently authored fixtures

Learning references: [ER03](#source-er03).

<a id="task-n02"></a>

#### N02 — Implement validated typed arrays and selected routines

Priority P3; relative size XL; discipline Numerics. Historical baseline: missing (2026-10-14).

Prerequisites: [N01](#task-n01).

Acceptance: Class/shape/overflow sparse complex rules preserved; numerical residuals and independent results checked; allocations measured

Learning references: [ER02](#source-er02), [ER03](#source-er03).

<a id="task-n03"></a>

#### N03 — Optimize and replace only justified execution paths

Priority P3; relative size XL; discipline Compiler. Historical baseline: missing (2026-10-14).

Prerequisites: [N02](#task-n02), [K01](#task-k01).

Acceptance: Same conformance gates pass; named accuracy-preserving benchmarks show improvement before switching default backend

Learning references: [ER03](#source-er03).

<a id="milestone-mp"></a>

### MP: Complete product distribution

<a id="task-q01"></a>

#### Q01 — Freeze combined source target matrix and package profile

Priority P0; relative size M; discipline Release. Historical baseline: missing (2026-10-14).

Prerequisites: [R01](#task-r01), [R02](#task-r02), [R06](#task-r06).

Acceptance: Application and roadmap commits integrated; mandatory feature IDs and exact OS CPU runtime targets frozen; version fields agree

Learning references: [RP01](#source-rp01), [RP03](#source-rp03).

<a id="task-q02"></a>

#### Q02 — Close pinned runtime package and native dependency graph

Priority P0; relative size L; discipline Release. Historical baseline: missing (2026-10-14).

Prerequisites: [Q01](#task-q01), [P01](#task-p01).

Acceptance: Hashes versions ABI toolkit fonts native libraries and source inventory complete; offline runtime claim tested without host installations

Learning references: [RP02](#source-rp02), [RP08](#source-rp08).

<a id="task-q03"></a>

#### Q03 — Build immutable package payload from empty staging

Priority P0; relative size L; discipline Release. Historical baseline: partial (2026-10-14).

Prerequisites: [Q02](#task-q02), [X01](#task-x01).

Acceptance: Removed inputs cannot survive next build; no developer paths/caches; manifest covers every shipped file; build parameters retained

Learning references: [RP01](#source-rp01), [RP03](#source-rp03).

<a id="task-q04"></a>

#### Q04 — Move writable state and editable tutorials outside app payload

Priority P0; relative size M; discipline Desktop. Historical baseline: partial (2026-10-14).

Prerequisites: [U01](#task-u01), [U05](#task-u05).

Acceptance: Read-only installed application launches; recovery figures preferences and user projects use writable external paths; example copy flow works

Learning references: [D15](#source-d15), [RP04](#source-rp04), [RP05](#source-rp05), [RP07](#source-rp07).

Source navigation (not completion evidence): [crates/mathlab-desktop/src/session.rs](crates/mathlab-desktop/src/session.rs).

<a id="task-q05"></a>

#### Q05 — Run required workshops through installed binaries

Priority P0; relative size L; discipline Compatibility. Historical baseline: missing (2026-10-14).

Prerequisites: [Q03](#task-q03), [Q04](#task-q04), [C04](#task-c04), [G04](#task-g04), [D03](#task-d03), [A01](#task-a01).

Acceptance: Core and Full mandatory journey suites run without developer checkout; numeric file graphics debugger results tied to installed build; missing prerequisite fails

Learning references: [D04](#source-d04), [D06](#source-d06), [S02](#source-s02), [RP03](#source-rp03), [RP08](#source-rp08).

<a id="task-q06"></a>

#### Q06 — Create and validate selected macOS installer

Priority P0; relative size L; discipline Release. Historical baseline: missing (2026-10-14).

Prerequisites: [Q05](#task-q05).

Acceptance: Dependency payload and adapters resolve after installation; promised Developer ID/notarization gates pass; downloaded final artifact and external state tested

Learning references: [RP04](#source-rp04).

<a id="task-q07"></a>

#### Q07 — Create and validate selected Windows distribution

Priority P0; relative size L; discipline Release. Historical baseline: missing (2026-10-14).

Prerequisites: [Q05](#task-q05).

Acceptance: Runtime DLLs engine subprocess and adapters work installed; manifest publisher/signing identity agree; installation and supported lifecycle pass

Learning references: [RP05](#source-rp05), [RP06](#source-rp06).

<a id="task-q08"></a>

#### Q08 — Create and validate selected Linux distribution

Priority P0; relative size L; discipline Release. Historical baseline: missing (2026-10-14).

Prerequisites: [Q05](#task-q05).

Acceptance: Runtime toolkit file dialogs and project permissions work under declared distribution/sandbox; runtime download/offline policy accurate; install/run/remove pass

Learning references: [RP07](#source-rp07).

<a id="task-q09"></a>

#### Q09 — Assemble rebuildable project and dependency source artifacts

Priority P0; relative size M; discipline Release. Historical baseline: partial (2026-10-14).

Prerequisites: [Q02](#task-q02).

Acceptance: Exact sources revisions notices native prerequisites and Cargo vendor inputs supplied; extracted source rebuilds in declared prepared environment

Learning references: [RP02](#source-rp02), [RP08](#source-rp08).

<a id="task-q10"></a>

#### Q10 — Verify upgrade failure recovery rollback and uninstall

Priority P0; relative size L; discipline Release. Historical baseline: missing (2026-10-14).

Prerequisites: [X02](#task-x02), [Q04](#task-q04), [Q05](#task-q05).

Acceptance: Interrupted update retains usable prior app; user projects/results retained; state migration versioned; uninstall policy exercised on selected target

Learning references: [RP04](#source-rp04), [RP06](#source-rp06), [RP07](#source-rp07).

<a id="task-q11"></a>

#### Q11 — Generate final distribution metadata and release directory

Priority P0; relative size M; discipline Release. Historical baseline: missing (2026-10-14).

Prerequisites: [Q05](#task-q05), [Q09](#task-q09), [Q10](#task-q10).

Acceptance: Hashes computed after signing/notarization processing; manifests reports source package ABI and artifact versions agree; every selected target has evidence; all selected target processing gates complete before final hashing

Target condition: include only the selected platform gates [Q06](#task-q06), [Q07](#task-q07), [Q08](#task-q08).

Learning references: [RP03](#source-rp03).

<a id="task-q12"></a>

#### Q12 — Accept and deliver complete installable Core or Full package

Priority P0; relative size M; discipline Product. Historical baseline: missing (2026-10-14).

Prerequisites: [Q11](#task-q11).

Acceptance: Every mandatory task and selected platform gate passes; included tutorials work installed; full installer/source/report set present; unfinished profile cannot say ready

Learning references: [RP01](#source-rp01), [RP02](#source-rp02), [RP03](#source-rp03), [RP04](#source-rp04), [RP05](#source-rp05), [RP06](#source-rp06), [RP07](#source-rp07), [RP08](#source-rp08).

<a id="milestone-ma"></a>

### MA: Advanced capability families

<a id="task-j01"></a>

#### J01 — Advanced classes, property contracts and object lifetime

Priority P2; relative size XL; discipline Language/data runtime. Historical baseline: planned (2026-10-14).

Prerequisites: [R01](#task-r01), [R03](#task-r03), [K04](#task-k04), [K05](#task-k05), [V01](#task-v01).

Acceptance: Author ValueGauge and HandleGauge with property Reading=4, plus a value Wrapper holding a HandleGauge. After b=a and b.Reading=9, ValueGauge a stays 4; HandleGauge a becomes 9. Copying Wrapper then mutating its referenced gauge must affect both wrappers. Delete the shared gauge: isvalid on both handle aliases is false, and later mutation errors. Record this exact trace without treating displayed struct fields as object restoration.

Advanced design: [AF01](#feature-af01).

Learning references: [AT01](#source-at01).

<a id="task-j02"></a>

#### J02 — Function argument blocks and name-value resolution

Priority P2; relative size XL; discipline Language/data runtime. Historical baseline: planned (2026-10-14).

Prerequisites: [R01](#task-r01), [R03](#task-r03), [K04](#task-k04), [E01](#task-e01), [D03](#task-d03), [J01](#task-j01).

Acceptance: Create scaledTotal(x,options) whose contract requires a nonempty double column x and positive scalar options.Gain defaulting to 2; body increments a separate counter and returns Gain*sum(x). Input [2;5] yields 14; Gain=3 yields 21. Gain=-1 fails before the body counter changes. An unknown named argument also fails. Add distinct fixtures for class conversion and shape expansion, rather than guessing they must reject every non-double or row input.

Advanced design: [AF02](#feature-af02).

Learning references: [AT02](#source-at02).

Source navigation (not completion evidence): [crates/mathlab-language/src/lib.rs](crates/mathlab-language/src/lib.rs), [native/README.md](native/README.md).

<a id="task-j03"></a>

#### J03 — Events, listeners, timers and guaranteed cleanup

Priority P2; relative size XL; discipline Language/data runtime. Historical baseline: planned (2026-10-14).

Prerequisites: [K01](#task-k01), [K03](#task-k03), [K04](#task-k04), [K05](#task-k05), [J01](#task-j01).

Acceptance: A local original function creates onCleanup that appends C to an external test log, appends A, then errors with Test:Stop. The harness catches it and appends B; expected log is A,C,B with C once. Separately a fake-clock fixedSpacing timer with three callbacks must produce exactly three callbacks then one stop record; deleting its token before a queued delivery causes zero further callbacks. No wall-clock deadline claim follows.

Conditional variant “Graphical callbacks”: [G04](#task-g04).

Advanced design: [AF03](#feature-af03).

Learning references: [AT03](#source-at03).

<a id="task-j04"></a>

#### J04 — Custom indexing, operator dispatch and class reflection

Priority P2; relative size XL; discipline Language/data runtime. Historical baseline: planned (2026-10-14).

Prerequisites: [R01](#task-r01), [R03](#task-r03), [K04](#task-k04), [V01](#task-v01), [V02](#task-v02), [E01](#task-e01), [J01](#task-j01).

Acceptance: Author OffsetVector storing [10,20,30] with custom parentheses reads returning stored value+7 and writes storing assigned value-7. v(2) is 27; assigning v(2)=50 makes backing storage element 43 and subsequent v(2)=50. Dot access to an ordinary public marker remains unchanged. Metadata inspection must not call a constructor whose separate count starts at zero. metaclass('OffsetVector') describes char; fromName resolves OffsetVector.

Advanced design: [AF04](#feature-af04).

Learning references: [AT04](#source-at04).

Source navigation (not completion evidence): [compatibility/README.md](compatibility/README.md), [native/README.md](native/README.md).

<a id="task-j05"></a>

#### J05 — Tables, timetables, calendars, time zones and grouped joins

Priority P2; relative size XL; discipline Language/data runtime. Historical baseline: planned (2026-10-14).

Prerequisites: [C01](#task-c01), [V01](#task-v01), [V03](#task-v03), [P01](#task-p01).

Acceptance: Original DST fixture: noon 2026-03-07 in America/Los_Angeles plus caldays(1) gives noon March 8, with 23 elapsed hours; plus hours(24) gives 13:00 March 8. Original join fixture keys [1;1;2] and [1;2;2] yields four rows under innerjoin. Group values [2;6;10] for keys [1;1;2] produce means [4;10] with declared key order. Check exact instants and counts, not formatted timestamps alone.

Conditional variant “External CSV/spreadsheet import”: [C02](#task-c02), [C03](#task-c03).

Advanced design: [AF05](#feature-af05).

Learning references: [AT05](#source-at05).

<a id="task-j06"></a>

#### J06 — Datastores, lazy tall plans and bounded-memory MapReduce

Priority P2; relative size XL; discipline Language/data runtime. Historical baseline: planned (2026-10-14).

Prerequisites: [K01](#task-k01), [K03](#task-k03), [K05](#task-k05), [C02](#task-c02), [P01](#task-p01), [J05](#task-j05).

Acceptance: Write an original file containing integers 1 through 1,000,000. A lazy sum/count reduction must return sum 500000500000, count 1000000 and mean 500000.5 for block sizes 997 and 8192. Under a declared 64 MiB operator-memory cap, operator buffers stay below the cap; report total process RSS separately. A full gather exceeding its configured output limit must diagnose before allocating it. Repeat with a much larger source for the out-of-memory gate.

Conditional variant “distributed tall execution”: [J20](#task-j20), [J21](#task-j21).

Conditional variant “Worker/background data plans”: [H01](#task-h01).

Advanced design: [AF06](#feature-af06).

Learning references: [AT06](#source-at06).

Source navigation (not completion evidence): [compatibility/README.md](compatibility/README.md).

<a id="task-j07"></a>

#### J07 — MAT 7.3, HDF5 partial access and object serialization

Priority P2; relative size XL; discipline Language/data runtime. Historical baseline: planned (2026-10-14).

Prerequisites: [R03](#task-r03), [C04](#task-c04), [V01](#task-v01), [K05](#task-k05).

Acceptance: Create A=reshape(1:24,4,6) in an original numeric fixture. Partial A(2:3,4:5) must be [14,18;15,19], preserving a 2-by-2 double result. Write only A(2,4)=99, reopen independently, and verify every other element unchanged. For a second oversized chunked file, report bytes/chunks read and peak memory to prove no full materialization. An unknown serialized class must yield unsupported restoration while its file bytes remain unchanged.

Conditional variant “serialized class reconstruction”: [J01](#task-j01).

Conditional variant “Matrix editing workflow”: [V02](#task-v02).

Advanced design: [AF07](#feature-af07).

Learning references: [AT07](#source-at07).

<a id="task-j08"></a>

#### J08 — MEX source compatibility and stable C/Rust embedding

Priority P2; relative size XL; discipline Language/data runtime. Historical baseline: planned (2026-10-14).

Prerequisites: [R02](#task-r02), [R03](#task-r03), [K01](#task-k01), [K03](#task-k03), [K06](#task-k06).

Acceptance: Author scaleComplex multiplying its input by 3. Input [2+4i,-1+5i] returns [6+12i,-3+15i] with identical shape/class, while the original input stays unchanged. A C host creates a session, computes sum([4,7,9])=20, gets the value and releases both value and session. Repeat 1000 calls under allocation instrumentation: every owned result is released and no exception crosses the C ABI. Wrong ABI metadata must be rejected.

Conditional variant “Installed native distribution”: [X01](#task-x01).

Advanced design: [AF08](#feature-af08).

Learning references: [AT08](#source-at08).

Source navigation (not completion evidence): [compatibility/README.md](compatibility/README.md).

<a id="task-j09"></a>

#### J09 — Optional Python, Java and .NET object interoperability

Priority P2; relative size XL; discipline Language/data runtime. Historical baseline: planned (2026-10-14).

Prerequisites: [R02](#task-r02), [K01](#task-k01), [K03](#task-k03), [U05](#task-u05), [V01](#task-v01).

Acceptance: Author three tiny libraries with no vendor samples: Python echo of integer 2^60+3 must return 1152921504606846979 exactly through an integer codec; a Java add(8,9) and a .NET Add(8,9) each return 17. A row-major Python 2-by-3 array [[1,2,3],[4,5,6]] must arrive with those logical entries and shape, regardless of storage order. Each missing runtime reports unavailable; it cannot count as a passed fixture.

Conditional variant “in-process C/MEX runtime bridge”: [J08](#task-j08).

Conditional variant “Installed foreign-runtime distribution”: [X01](#task-x01).

Advanced design: [AF09](#feature-af09).

Learning references: [AT09](#source-at09).

<a id="task-j10"></a>

#### J10 — Parameterized tests, mocks, coverage and measured memory

Priority P2; relative size XL; discipline Language/data runtime. Historical baseline: planned (2026-10-14).

Prerequisites: [A02](#task-a02), [A03](#task-a03), [K01](#task-k01), [K04](#task-k04), [U05](#task-u05), [R03](#task-r03), [J01](#task-j01), [J04](#task-j04).

Acceptance: Author a suite with two named types and three lengths, and two methods using both parameters exhaustively: discovery yields 12 unique cases. Deliberately fail exactly one method for length=3 under both types: report 10 pass and 2 fail, with teardown called 12 times. A mock read method returning 41 is called exactly once. Allocation instrumentation of one unshared 1000-element double buffer records 8000 payload bytes; metadata/temporary allocations are reported separately.

Advanced design: [AF10](#feature-af10).

Learning references: [AT10](#source-at10).

Source navigation (not completion evidence): [compatibility/README.md](compatibility/README.md).

<a id="task-j11"></a>

#### J11 — Project isolation, build task graphs and toolbox lifecycle

Priority P2; relative size XL; discipline Language/data runtime. Historical baseline: planned (2026-10-14).

Prerequisites: [U05](#task-u05), [P01](#task-p01), [A02](#task-a02), [R02](#task-r02).

Acceptance: Original DAG: check→test→archive, with archive depending on both check and test. Running archive produces exactly check,test,archive once in dependency order. Force test failure: archive is blocked and no final archive appears. Add cycle archive→check: validation rejects it before any task runs. Relocate the original project and install its package: relative resources resolve; an interrupted update leaves the prior usable version and its manifest intact.

Conditional variant “parameterized test/coverage build tasks”: [J10](#task-j10).

Conditional variant “User-app package artifacts”: [X03](#task-x03).

Conditional variant “Whole-application release assembly”: [Q02](#task-q02), [Q03](#task-q03).

Conditional variant “Signed installation/update”: [X02](#task-x02).

Advanced design: [AF11](#feature-af11).

Learning references: [AT11](#source-at11).

Source navigation (not completion evidence): [compatibility/README.md](compatibility/README.md), [native/README.md](native/README.md).

<a id="task-j12"></a>

#### J12 — Symbolic assumptions, arbitrary precision and generated functions

Priority P2; relative size XL; discipline Language/data runtime. Historical baseline: planned (2026-10-14).

Prerequisites: [P07](#task-p07), [P01](#task-p01), [A01](#task-a01).

Acceptance: Original polynomial f=(x+2)^3 differentiates exactly to 3*(x+2)^2; at x=1 the generated derivative returns 27. For positive x, sqrt(x^2) simplifies to x; for real x it must not become x universally (x=-2 requires 2). Construct exact 2/7 and evaluate at 50 digits: absolute error versus the rational is below 1e-49. Record assumption reset and generated argument-order behavior separately from display formatting.

Conditional variant “Python/SymPy backend selected”: [J09](#task-j09).

Conditional variant “Live symbolic document”: [A04](#task-a04).

Advanced design: [AF12](#feature-af12).

Learning references: [AT12](#source-at12).

<a id="task-j13"></a>

#### J13 — 3D scenes, volumes, cameras, lighting and animation

Priority P2; relative size XL; discipline Language/data runtime. Historical baseline: planned (2026-10-14).

Prerequisites: [R05](#task-r05), [G01](#task-g01), [G02](#task-g02), [G03](#task-g03), [G04](#task-g04), [G05](#task-g05), [K05](#task-k05).

Acceptance: Original field V(x,y,z)=x on a regular grid x=[0,1,2,3], y=z=[0,1,2]. isosurface at 1.5 must produce vertices whose x coordinate equals 1.5 within 1e-12 and y/z lie within [0,2]; exact triangulation counts are intentionally not assumed. Rotate then reset restores recorded camera properties. A 100-update line animation retains one object ID and final sample 100; exported source data remains complete despite display decimation.

Conditional variant “timer/listener animation callbacks”: [J03](#task-j03).

Advanced design: [AF13](#feature-af13).

Learning references: [AT13](#source-at13).

<a id="task-j14"></a>

#### J14 — Live tasks, reusable components and app code ownership

Priority P2; relative size XL; discipline Language/data runtime. Historical baseline: planned (2026-10-14).

Prerequisites: [A04](#task-a04), [A05](#task-a05), [G01](#task-g01), [G04](#task-g04), [E02](#task-e02), [J01](#task-j01), [J03](#task-j03).

Acceptance: Original task converts x=[2,5,8] using y=Gain*x+Offset. Gain=3 and Offset=-1 generate deterministic source and yield [5,14,23]. After Gain changes to 4, old output becomes stale until rerun yields [7,19,31]. A reusable component emits one ValueChanged event for one explicit user change. Rename its designer ID and undo: user callback bytes and resource references survive; an unsupported imported widget is reported rather than silently deleted.

Conditional variant “modern argument validation in components”: [J02](#task-j02).

Conditional variant “Installed user-app distribution”: [X03](#task-x03).

Conditional variant “Vendor document conversion”: [C04](#task-c04).

Advanced design: [AF14](#feature-af14).

Learning references: [AT14](#source-at14).

Source navigation (not completion evidence): [compatibility/README.md](compatibility/README.md).

<a id="task-j15"></a>

#### J15 — Reusable decompositions, generalized eigenproblems, and sparse Krylov solvers

Priority P2; relative size XL; discipline Scientific computing. Historical baseline: planned (2026-10-14).

Prerequisites: [C05](#task-c05), [C06](#task-c06), [K05](#task-k05), [V01](#task-v01).

Acceptance: Original fixtures: A=diag([2,6]), B=diag([1,2]) yields generalized eigenvalues {2,3}, with normalized residual <1e-12. expm(diag([1,2])) matches diag([exp(1),exp(2)]) within 1e-12 relative error. For a 4096-by-4096 tridiagonal SPD A with diagonal 4 and both offdiagonals -1, b=A*ones(4096,1), require solution shape 4096-by-1, relative residual <=1e-9 and max error <=1e-7. Repeated solves reuse one factorization. A singular preconditioner reports failure rather than success.

Advanced design: [AF15](#feature-af15).

Learning references: [AT15](#source-at15).

<a id="task-j16"></a>

#### J16 — Constrained nonlinear, LP, QP, MILP, and global optimization

Priority P2; relative size XL; discipline Scientific computing. Historical baseline: planned (2026-10-14).

Prerequisites: [P05](#task-p05), [P01](#task-p01), [K03](#task-k03), [A01](#task-a01).

Acceptance: Original constrained quadratic: minimize (x-2)^2+(y-1)^2 with x+y<=2; a 2-by-1 answer must be [1.5;0.5] within 1e-7, objective 0.5 within 1e-10, violation <=1e-9. Binary MILP minimizing -3x-2y with 2x+y<=3 yields [1;1], objective -5, and zero certified gap. Bounds 0<=x<=1 plus x>=2 must produce an infeasible status. Every global run reports its actual evidence level.

Advanced design: [AF16](#feature-af16).

Learning references: [AT16](#source-at16).

<a id="task-j17"></a>

#### J17 — GLMs, ensembles, cross-validation, and Bayesian diagnostics

Priority P2; relative size XL; discipline Scientific computing. Historical baseline: planned (2026-10-14).

Prerequisites: [P04](#task-p04), [C01](#task-c01), [V03](#task-v03), [P01](#task-p01).

Acceptance: Original intercept-only Poisson GLM with responses [1;2;3;2] and log link must return intercept log(2), four predictions equal to 2, and deviance 2*(-log(2)+3*log(1.5)), within 1e-8. Each explicit four-fold partition excludes its held-out row from fitted preprocessing. A normal prior N(0,1) and observation 2 with unit variance gives analytic posterior N(1,0.5); sampler checks use reported Monte Carlo uncertainty. Negative Poisson responses fail validation.

Conditional variant “parallel model fitting”: [J20](#task-j20).

Advanced design: [AF17](#feature-af17).

Learning references: [AT17](#source-at17).

<a id="task-j18"></a>

#### J18 — Streaming DSP, System objects, wavelets, and time-frequency analysis

Priority P2; relative size XL; discipline Scientific computing. Historical baseline: planned (2026-10-14).

Prerequisites: [P02](#task-p02), [G03](#task-g03), [G04](#task-g04), [K05](#task-k05), [P01](#task-p01), [J01](#task-j01).

Acceptance: Original FIR b=[0.25,0.5,0.25], zero initial state, and impulse [1,0,0,0] split into two two-sample frames must concatenate to [0.25,0.5,0.25,0] exactly for double binary fractions; resetting between frames must yield a different documented trace. Orthonormal Haar DWT of [1,1] gives approximation sqrt(2), detail 0 within 1e-12. STFT/ISTFT with a declared reconstructible window returns original length and relative error <1e-10. Invalid overlap fails before streaming.

Advanced design: [AF18](#feature-af18).

Learning references: [AT18](#source-at18).

<a id="task-j19"></a>

#### J19 — Robust control, model predictive control, and system identification

Priority P2; relative size XL; discipline Scientific computing. Historical baseline: planned (2026-10-14).

Prerequisites: [P03](#task-p03), [P05](#task-p05), [P01](#task-p01), [J15](#task-j15), [J16](#task-j16).

Acceptance: Original noiseless y(k)=0.5*y(k-1)+2*u(k-1), with full-rank regressor and sample time 1 second, gives regression coefficients [0.5,2]. arx(...,[1,1,1]) must return idpoly A=[1,-0.5], B=[0,2], with input delay nk=1; coefficient errors and prediction residual are <=1e-10. One-step MPC minimizing (1+u)^2+u^2 with -0.25<=u<=0.25 returns scalar u=-0.25 and cost 0.625 within 1e-10. For G=1/(s+a), a in [1,2], K=1, closed-loop poles are -(a+1). Contradictory MPC bounds fail validation.

Conditional variant “Simulink-integrated controllers”: [J29](#task-j29).

Advanced design: [AF19](#feature-af19).

Learning references: [AT19](#source-at19).

<a id="task-j20"></a>

#### J20 — parfor classification, SPMD, distributed arrays, and pool scheduling

Priority P2; relative size XL; discipline Scientific computing. Historical baseline: planned (2026-10-14).

Prerequisites: [H01](#task-h01), [H02](#task-h02), [C06](#task-c06), [K05](#task-k05).

Acceptance: Original parfor map B(i)=i*i for i=1:100 returns a 1-by-100 vector with B(1)=1, B(100)=10000 and exact integer-valued sum 338350 under double. Distribute reshape(1:12,3,4), double every partition, and gather exactly the same 3-by-4 matrix as serial execution. A dependent update A(i)=A(i-1)+1 must be rejected. A missing collective participant must time out with worker identities, without publishing a successful array.

Advanced design: [AF20](#feature-af20).

Learning references: [AT20](#source-at20).

<a id="task-j21"></a>

#### J21 — Futures, data queues, background workers, and remote batch jobs

Priority P2; relative size XL; discipline Scientific computing. Historical baseline: planned (2026-10-14).

Prerequisites: [H01](#task-h01), [K03](#task-k03), [U05](#task-u05).

Acceptance: Original futures f(n)=sum(1:n), submitted for n={3,5,8}, return scalars {6,15,36} associated with their submitted identities regardless of completion order. Each job sends exactly one tagged completion event; duplicates are ignored by sequence ID. Cancel an instrumented job before its final commit and verify no successful output is published. Reconnect to the same remote job retrieves identical artifact hashes; a corrupt output or unsupported thread-only call fails explicitly.

Conditional variant “Remote batch/reconnect service”: [H04](#task-h04).

Advanced design: [AF21](#feature-af21).

Learning references: [AT21](#source-at21).

<a id="task-j22"></a>

#### J22 — GPU array residency, CUDA kernels, precision, and GPU code generation

Priority P2; relative size XL; discipline Scientific computing. Historical baseline: planned (2026-10-14).

Prerequisites: [H03](#task-h03), [C06](#task-c06), [V01](#task-v01), [P01](#task-p01).

Acceptance: Original device GEMM [1,2;3,4]*[2,0;1,2] returns a resident 2-by-2 array [4,4;10,8], with max error <=1e-12 double or <=1e-5 single after explicit gather. FFT of a 64-by-1 impulse yields 64 ones within the same class-specific tolerances. A two-operation chain performs no intermediate host transfer according to instrumentation. Oversized allocation, invalid launch geometry, and stale device handles must fail without corrupting the session.

Conditional variant “generated GPU kernels”: [J35](#task-j35).

Advanced design: [AF22](#feature-af22).

Learning references: [AT22](#source-at22).

<a id="task-j23"></a>

#### J23 — dlnetwork, labeled tensors, autodiff, custom training, and ONNX

Priority P3; relative size XL; discipline Scientific computing. Historical baseline: planned (2026-10-14).

Prerequisites: [Z01](#task-z01), [K03](#task-k03), [K05](#task-k05), [P01](#task-p01), [G02](#task-g02).

Acceptance: Original loss L(w)=0.5*sum((w*[1,2]-[3,5]).^2) at scalar w=1 gives L=6.5 and gradient -8 within 1e-12 double. One SGD step at learning rate 0.1 gives w=1.8 and loss 1.7. A labeled 1-by-2 channel/batch tensor preserves its declared shape. Export/import an original affine network and compare predictions to independent ONNX Runtime within 1e-6 single. Unsupported gradient or ONNX operators fail explicitly.

Conditional variant “GPU training”: [J22](#task-j22).

Conditional variant “Background/parallel training”: [H01](#task-h01).

Conditional variant “Tabular input conversion”: [C01](#task-c01).

Advanced design: [AF23](#feature-af23).

Learning references: [AT23](#source-at23).

<a id="task-j24"></a>

#### J24 — Reinforcement learning environments, agents, rollout, and training

Priority P3; relative size XL; discipline Scientific computing. Historical baseline: planned (2026-10-14).

Prerequisites: [Z01](#task-z01), [K03](#task-k03), [K05](#task-k05), [P01](#task-p01).

Acceptance: Original environment resets state to 0; action 1 increments it, rewards 1 each step, and terminates at state 3. Deterministic rollout must produce observations [0,1,2,3], actions [1,1,1], rewards [1,1,1], and discounted return 2.71 for gamma=0.9. A DQN target with reward 1 and next maximum Q=2 is 2.8 when nonterminal and 1 when terminal, within 1e-12. An out-of-spec action fails before changing environment state.

Conditional variant “neural policy/value agents”: [J23](#task-j23).

Conditional variant “parallel rollouts”: [J20](#task-j20).

Conditional variant “model-based environments”: [J29](#task-j29).

Conditional variant “Parallel rollout jobs”: [H01](#task-h01), [H02](#task-h02).

Conditional variant “Simulation-backed environment”: [B02](#task-b02).

Advanced design: [AF24](#feature-af24).

Learning references: [AT24](#source-at24).

<a id="task-j25"></a>

#### J25 — PDE and finite elements, adaptive meshes, boundaries, and transient physics

Priority P3; relative size XL; discipline Scientific computing. Historical baseline: planned (2026-10-14).

Prerequisites: [Z01](#task-z01), [C06](#task-c06), [C08](#task-c08), [G02](#task-g02), [G03](#task-g03), [P01](#task-p01), [J15](#task-j15).

Acceptance: Freeze [0,1]^2 into a uniform N-by-N square grid, each square split southwest-to-northeast into two right triangles; h=1/N. Use conforming Galerkin P1, homogeneous Dirichlet conditions, and coefficient one in -Delta. Load integrals and numerical L2 norms use 6-by-6 Gauss-Legendre/Duffy quadrature per triangle. Poisson u=x*(1-x)*y*(1-y), f=2*(x*(1-x)+y*(1-y)): at N=16 and 32 require absolute L2 error <=1e-3 and coarse/fine error ratio >=1.5. Heat u_t=Delta*u, nodally interpolated initial sin(pi*x)*sin(pi*y): consistent-mass backward Euler, N=32, dt=1e-4, 200 steps; at t=0.02 require relative L2 error <=2% against exp(-2*pi^2*t) times the initial field. Zero-flux all-Neumann conditions with f=1 must report incompatibility.

Conditional variant “physical-network coupling”: [J32](#task-j32).

Advanced design: [AF25](#feature-af25).

Learning references: [AT25](#source-at25).

<a id="task-j26"></a>

#### J26 — Computer vision, medical 3D segmentation, registration, calibration, and tracking

Priority P3; relative size XL; discipline Scientific computing. Historical baseline: planned (2026-10-14).

Prerequisites: [P06](#task-p06), [C04](#task-c04), [V01](#task-v01), [G02](#task-g02), [Z01](#task-z01), [P01](#task-p01).

Acceptance: Original 5-by-5-by-5 volume with a centered 3-by-3-by-3 binary cube contains 27 voxels; spacing [2,3,4] mm gives volume 648 mm^3 exactly, and mask Dice against itself is 1. An undistorted pinhole with zero skew, identity rotation, zero translation, fx=400, fy=500, cx=100, cy=80 maps world [1,2,10] to pixel [140,180] within 1e-12. A known 3D translation preserves physical landmarks; asymmetric-phantom registration requires landmark RMS <0.1 voxel. Missing orientation and nonmonotonic tracking timestamps produce diagnostics.

Advanced design: [AF26](#feature-af26).

Learning references: [AT26](#source-at26).

<a id="task-j27"></a>

#### J27 — Robotics rigid-body trees, motion planning, SLAM, ROS 2, and sensor fusion

Priority P3; relative size XL; discipline Scientific computing. Historical baseline: planned (2026-10-14).

Prerequisites: [Z01](#task-z01), [G02](#task-g02), [P01](#task-p01).

Acceptance: Original planar two-link arm with lengths 1 and 1 has endpoint [2,0,0] at q=[0,0] and [0,2,0] at q=[pi/2,0], within 1e-12; its planar position Jacobian at zero is [0,0;2,1]. Scalar Kalman update with prior mean 0, variance 1, observation 2 and measurement variance 1 gives mean 1 and variance 0.5. Every returned path segment passes the declared collision check. Conflicting ROS frame IDs or expired timestamps fail explicitly.

Conditional variant “remote ROS targets”: [J21](#task-j21).

Conditional variant “simulated robot models”: [J29](#task-j29).

Conditional variant “Control-system integration”: [P03](#task-p03).

Conditional variant “Remote execution service”: [H04](#task-h04).

Conditional variant “Model-based robot simulation”: [B02](#task-b02).

Advanced design: [AF27](#feature-af27).

Learning references: [AT27](#source-at27).

<a id="task-j28"></a>

#### J28 — Communications, 5G standards, RF networks, radar, and phased arrays

Priority P3; relative size XL; discipline Scientific computing. Historical baseline: planned (2026-10-14).

Prerequisites: [Z01](#task-z01), [P02](#task-p02), [G03](#task-g03), [P01](#task-p01), [J18](#task-j18).

Acceptance: Original 64-carrier OFDM with CP length 16 returns 80 samples and recovers the original grid with relative error <1e-12 double; noiseless declared 4-QAM mapping round-trips every symbol exactly. A four-element half-wavelength ULA using phase pi*n*sin(theta) has unnormalized broadside amplitude 4 and endfire amplitude 0 within 1e-12. A matched 50-ohm load has reflection 0. Delay 1 microsecond yields radar range 149.896229 m within 1e-6. These checks do not certify 5G; invalid allocations must fail.

Conditional variant “GPU acceleration”: [J22](#task-j22).

Advanced design: [AF28](#feature-af28).

Learning references: [AT28](#source-at28).

<a id="task-j29"></a>

#### J29 — Multirate simulation, buses, variants and model references

Priority P3; relative size XL; discipline Modeling/compiler/domain. Historical baseline: planned (2026-10-14).

Prerequisites: [B01](#task-b01), [B02](#task-b02), [K01](#task-k01), [U05](#task-u05).

Acceptance: An original single-thread deterministic profile samples integer counter u at 0.01 s and a consumer at 0.03 s over 0..0.09 s. With producer-before-consumer ordering and zero-latency same-tick transfer, consumer values are exactly 0,3,6,9. A separately declared one-tick buffered profile must match its own specified latency rather than this oracle.

Two instances of one accumulator reference receive 1 and 2 per tick with initial state zero; after four updates their states are 4 and 8. Switching the selected gain between 2 and 3 recompiles and changes outputs without changing another instance. A missing nested bus field fails compilation with its complete element path; unrelated inactive unsupported branches remain disclosed.

Conditional variant “Imported vendor models”: [B04](#task-b04).

Advanced design: [AF29](#feature-af29).

Learning references: [AT29](#source-at29).

<a id="task-j30"></a>

#### J30 — Stiff ODEs, DAEs, algebraic loops and event consistency

Priority P3; relative size XL; discipline Modeling/compiler/domain. Historical baseline: planned (2026-10-14).

Prerequisites: [C08](#task-c08), [C06](#task-c06), [K05](#task-k05), [J15](#task-j15).

Acceptance: For F1=x'+x=0 and F2=z-x²=0, use x(0)=1 and z(0)=1. Over 0..1 s, compare x=e^(-t), z=e^(-2t), with max absolute error below 1e-6 and algebraic residual below 1e-8 under the declared tolerance profile. Fixing z(0)=2 simultaneously must yield a consistency error.

A declared upward terminal event g=x-0.5 for x'=1, x(0)=0 occurs at 0.5 s within 1e-6 s. Simultaneous events resolve by documented stable ID order, which must repeat across runs. The algebraic loop y=0.5y+1 yields y=2; y=y+1 reports inconsistency, preserving the original equations.

Conditional variant “Block algebraic loops or hybrid event integration”: [B02](#task-b02), [B03](#task-b03).

Advanced design: [AF30](#feature-af30).

Learning references: [AT30](#source-at30).

<a id="task-j31"></a>

#### J31 — Hierarchical statecharts, parallel regions, history and temporal logic

Priority P3; relative size XL; discipline Modeling/compiler/domain. Historical baseline: planned (2026-10-14).

Prerequisites: [B03](#task-b03), [K01](#task-k01), [D02](#task-d02), [A02](#task-a02).

Acceptance: An original parent P contains exclusive children A and B, initially A. Event switch enters B, suspend leaves P, and resume reenters P through history into B; resetting the chart clears history and enters A. The trace must show the declared exit/transition/entry action sequence exactly, including repeated initialization.

Parallel region R1 increments n before R2 records n; after three dispatches starting at n=0, observations are 1,2,3. Reversing priorities gives 0,1,2. A declared after(3,tick) transition fires on the third tick after entry, and exiting/reentering restarts its counter. A self-broadcast cycle stops with a bounded-cascade error without freezing the editor.

Advanced design: [AF31](#feature-af31).

Learning references: [AT31](#source-at31).

<a id="task-j32"></a>

#### J32 — Equation-based physical networks, units and coupled DAEs

Priority P3; relative size XL; discipline Modeling/compiler/domain. Historical baseline: planned (2026-10-14).

Prerequisites: [B02](#task-b02), [C08](#task-c08), [C06](#task-c06), [G02](#task-g02), [J30](#task-j30).

Acceptance: An original source-resistor-capacitor network uses V=1 V, R=1 ohm, C=1 F, vC(0)=0 V. Its capacitor voltage must agree with 1-e^(-t) within 1e-6 V on 0..5 s, and current balance at every recorded node must be below 1e-8 A under the declared solver profile.

Entering resistance as 1000 milliohm produces the same physical trace; resistance in meters fails before execution. An isolated conservative mass-spring network with m=1 kg, k=4 N/m, x(0)=1 m and v(0)=0 m/s follows x=cos(2t) within 1e-5 m over 0..5 s. Energy drift and tolerance sensitivity are reported separately; incompatible fixed initial capacitor constraints yield an actionable conflict.

Advanced design: [AF32](#feature-af32).

Learning references: [AT32](#source-at32).

<a id="task-j33"></a>

#### J33 — Entities, queues, resources and reproducible discrete-event studies

Priority P3; relative size XL; discipline Modeling/compiler/domain. Historical baseline: planned (2026-10-14).

Prerequisites: [K01](#task-k01), [K03](#task-k03), [K05](#task-k05), [P04](#task-p04).

Acceptance: Three original arrivals at t=0,1,2 s enter a capacity-unbounded FIFO single server with 2 s service; departures must occur at 2,4,6 s and waits at 0,1,2 s. With observation [0,6) and arrivals joining after same-time departures, waiting-queue area is 3 entity-seconds, so mean queue length is 0.5 and server utilization is 1.

Generated plus initial entities equal terminated plus resident plus dropped entities at every checkpoint. A resource is owned by at most one entity for capacity one. Repeating a seeded replication reproduces its event sequence under the same backend/version; changing parallel worker count leaves assigned replication streams unchanged. Confidence intervals report their estimator and independence assumptions.

Conditional variant “Parallel independent replications”: [H01](#task-h01).

Conditional variant “Continuous/hybrid coupling”: [B02](#task-b02), [B03](#task-b03).

Advanced design: [AF33](#feature-af33).

Learning references: [AT33](#source-at33).

<a id="task-j34"></a>

#### J34 — FMI 2/3 interoperability and Modelica co-simulation

Priority P3; relative size XL; discipline Modeling/compiler/domain. Historical baseline: planned (2026-10-14).

Prerequisites: [B02](#task-b02), [K03](#task-k03), [R02](#task-r02).

Acceptance: An independently authored Co-Simulation FMU exposes x'=-x with x(0)=1 and its own solver. At communication times 0..1 s, compare x=e^(-t) within the declared FMU solver tolerance; the importer must preserve reported lastSuccessfulTime rather than publish a requested future time after early return.

An original FMI 3 Scheduled Execution FMU has partitions at 0.01 and 0.02 s with explicit priorities; activation logs must match the frozen schedule, including coincident ticks. Invalid calls before initialization fail without entering simulation. Standard-schema, lifecycle and reference-FMU checks are separate from numerical smoke tests; cross-tool interoperability is recorded by exact exporting/importing tool versions.

Conditional variant “FMI Model Exchange”: [J30](#task-j30).

Conditional variant “FMI Scheduled Execution or multirate export”: [J29](#task-j29).

Conditional variant “Imported model adapters”: [B04](#task-b04).

Advanced design: [AF34](#feature-af34).

Learning references: [AT34](#source-at34).

<a id="task-j35"></a>

#### J35 — Generated C/C++, executable modules, MEX and SIL/PIL verification

Priority P3; relative size XL; discipline Modeling/compiler/domain. Historical baseline: planned (2026-10-14).

Prerequisites: [K04](#task-k04), [R03](#task-r03), [R02](#task-r02), [A02](#task-a02).

Acceptance: An original bounded 2x2 transform y=A*x with A=[1,2;3,4] and x=[5;6] returns [17;39] in the interpreter, generated host executable and declared extension harness. A stateful y[n]=0.5y[n-1]+u[n], y[-1]=0, impulse input yields 1,0.5,0.25,0.125 with exact binary floating-point values.

Unsupported unbounded eval or dynamic callbacks must fail at the source location before generation. Two generated filter instances retain independent state. A clean-machine executable runs without an undeclared interpreter installation. PIL acceptance additionally records actual target/compiler/transport and captured output; passing host SIL cannot satisfy PIL. Changed source invalidates older generated artifacts and their equivalence report.

Conditional variant “model/multirate code generation”: [J29](#task-j29).

Conditional variant “physical-network code generation”: [J32](#task-j32).

Conditional variant “MEX source output”: [J08](#task-j08).

Conditional variant “Model-to-code front end”: [B01](#task-b01), [B02](#task-b02).

Conditional variant “Installed generated artifact distribution”: [X01](#task-x01).

Delivery selection “processor-in-the-loop delivery”: [AF37](#feature-af37); required external evidence: target_execution.

Advanced design: [AF35](#feature-af35).

Learning references: [AT35](#source-at35).

<a id="task-j36"></a>

#### J36 — Fixed-point arithmetic and embedded, HDL and PLC generation

Priority P3; relative size XL; discipline Modeling/compiler/domain. Historical baseline: planned (2026-10-14).

Prerequisites: [R03](#task-r03), [P01](#task-p01), [P02](#task-p02), [K04](#task-k04).

Acceptance: For an original signed 8-bit value with fraction length 4, representing 8 saturates to 7.9375 or wraps to -8 according to the declared policy. Convergent rounding maps 1.03125 to 1 and 1.09375 to 1.125. Verify stored integers independently, including negatives, overflow and intermediate widening.

A two-tap FIR with coefficients [0.5,0.5], zero state and an impulse yields [0.5,0.5,0] in fixed-point simulation; generated RTL must reproduce these bits after its declared cycle latency and reset protocol. A scan-based hysteresis pump initially off, on below 2 m and off above 4 m, gives on,on,off for levels 1,3,5 m. Physical deployment requires recorded board/PLC, clock, toolchain and captured I/O.

Conditional variant “generated C/HDL/PLC targets”: [J35](#task-j35).

Conditional variant “statechart code emission”: [J31](#task-j31).

Conditional variant “Block/model target generation”: [B02](#task-b02).

Advanced design: [AF36](#feature-af36).

Learning references: [AT36](#source-at36).

<a id="task-j37"></a>

#### J37 — Real-time target execution, HIL schedules and hardware I/O

Priority P3; relative size XL; discipline Modeling/compiler/domain. Historical baseline: planned (2026-10-14).

Prerequisites: [B02](#task-b02), [K03](#task-k03), [R02](#task-r02), [J29](#task-j29), [J35](#task-j35).

Acceptance: A virtual-time fixture with a 1 ms base task and 5 ms secondary task must log exactly 1000 and 200 activations over [0,1) s, with fixed coincident-task order. Injecting a delayed device sample exercises the declared stale-input rule and preserves its timestamp, instead of silently relabeling it as current.

For the selected physical target, define a release budget such as execution below a 1 ms deadline and jitter at most 50 microseconds during a specified 10-minute load/I/O test. Report observed maxima, sample count, clock method and overruns; these measurements apply only to that setup. A host-only run cannot satisfy this gate. Stop, disconnect and watchdog tests must demonstrate the specified target output behavior.

Conditional variant “physical-plant HIL”: [J32](#task-j32).

Conditional variant “instrument/acquisition hardware I/O”: [J44](#task-j44).

Conditional variant “Background deployment jobs”: [H01](#task-h01).

Conditional variant “Hybrid plant events”: [B03](#task-b03).

Advanced design: [AF37](#feature-af37).

Learning references: [AT37](#source-at37).

<a id="task-j38"></a>

#### J38 — Requirements, model tests, structural coverage and formal verification

Priority P3; relative size XL; discipline Modeling/compiler/domain. Historical baseline: planned (2026-10-14).

Prerequisites: [R01](#task-r01), [A02](#task-a02), [K04](#task-k04).

Acceptance: An original controller requirement says a nonnegative demand below 10 yields output equal to demand and demand at least 10 yields 10. Boundary tests at 0,9,10,11 verify the contract and source-linked verdicts. A source edit makes earlier evidence stale; rerunning regenerates a report tied to the new model hash.

For decision (a && b), declare independent MC/DC obligations and use vectors (0,1),(1,0),(1,1), checking the recorded truth evaluations and masking policy. An original unsigned 8-bit counter uses an explicitly declared wrap arithmetic profile, initial value 0, and increments by 1. The property that values increase monotonically fails at the 256th update with witness 255→0; this witness must reproduce in simulation. This wrap profile is separate from MATLAB built-in uint8 saturating arithmetic. Removing an input assumption must invalidate the previous formal result. Qualification remains 'not established' until a separately reviewed version/configuration-specific evidence program is completed.

Conditional variant “statechart structural coverage”: [J31](#task-j31).

Conditional variant “generated-code verification”: [J35](#task-j35).

Conditional variant “Model test harnesses”: [B02](#task-b02).

Advanced design: [AF38](#feature-af38).

Learning references: [AT38](#source-at38).

<a id="task-j39"></a>

#### J39 — Econometric models, derivative pricing, portfolio risk and backtesting

Priority P3; relative size XL; discipline Modeling/compiler/domain. Historical baseline: planned (2026-10-14).

Prerequisites: [Z01](#task-z01), [C01](#task-c01), [P04](#task-p04), [P05](#task-p05), [J16](#task-j16), [J17](#task-j17).

Acceptance: With constant 10-unit asset prices, initial cash 1000 and a cost-free all-cash strategy, every ledger row remains 1000. A buy of 50 units at 10 with a 1% fee leaves cash 495 and asset value 500, hence total 995. Supplying future prices to a decision callback must fail the declared point-in-time contract.

For an original European option fixture, set S=K=100 currency units, r=0 per year, T=1 year, continuous dividend yield=0 per year, and volatility sigma=0.2 per square-root year. The independent closed-form oracle is C=P=100*(2*Phi(0.1)-1)=7.9655674554058 currency units, with absolute error at most 1e-8 for each price. Verify call-put parity C-P=0 and positivity as additional invariants rather than the sole price proof. A two-asset minimum-variance problem with covariance diag(1,4), full investment and no shorts has weights [0.8,0.2]. A zero-innovation AR(1) with coefficient 0.5 and final value 8 forecasts 4,2,1 under the frozen convention.

Conditional variant “Imported market formats”: [C03](#task-c03).

Conditional variant “Live analysis workshop”: [A04](#task-a04).

Advanced design: [AF39](#feature-af39).

Learning references: [AT39](#source-at39).

<a id="task-j40"></a>

#### J40 — Bioinformatics pipelines and quantitative biological model fitting

Priority P3; relative size XL; discipline Modeling/compiler/domain. Historical baseline: planned (2026-10-14).

Prerequisites: [Z01](#task-z01), [C08](#task-c08), [P04](#task-p04), [P05](#task-p05), [C02](#task-c02), [J17](#task-j17).

Acceptance: An original FASTQ fixture contains reads ACGT with Phred scores [30,30,30,30] and NNNN with [5,5,5,5]. A declared mean-quality threshold 20 retains exactly one read and preserves its identifier and qualities. A hand-authored alignment fixture has original record IDs r1,r2,r3 and one-based inclusive aligned spans [10,12], [12,14], [15,17]. Inclusive query [12,14] overlaps exactly two records, r1 and r2; query [14,14] overlaps exactly one record, r2. Their zero-based half-open query translations are [11,14) and [13,14), and aligned-span translations are [9,12), [11,14), [14,17). Verify matching record IDs and record counts, not the number of covered bases, with the original IDs preserved.

For dC/dt=-kC, C(0)=10 mg/L and k=0.5 per hour, compare C(2)=10e^-1 mg/L within 1e-6. Fitting noiseless synthetic observations with declared positive transform and bounds must recover k within 1e-5 per hour. Adding a 2 mg/L instantaneous dose at 1 hour produces separately logged pre/post values differing by 2; unsupported required SBML packages must stop import rather than disappear.

Conditional variant “biological DAE models”: [J30](#task-j30).

Conditional variant “Parallel pipeline/model fitting”: [H01](#task-h01).

Advanced design: [AF40](#feature-af40).

Learning references: [AT40](#source-at40).

<a id="task-j41"></a>

#### J41 — Geospatial rasters, NetCDF, DICOM and domain-aware visualization

Priority P3; relative size XL; discipline Modeling/compiler/domain. Historical baseline: planned (2026-10-14).

Prerequisites: [Z01](#task-z01), [C04](#task-c04), [V01](#task-v01), [G02](#task-g02), [G03](#task-g03), [P06](#task-p06).

Acceptance: A hand-authored 2x3 geographic cells raster spans longitude 0..3 degrees and latitude 0..2 degrees with northern rows first. Cell centers must be longitudes 0.5,1.5,2.5 and latitudes 1.5,0.5 degrees. Swapping axes or treating cells as postings must be detected by independent coordinate assertions.

A packed int16 NetCDF variable [0,2,-999] with fill -999, scale 0.5 and offset 10 decodes to [10,11,NaN] under the selected convention; strided subsets match independent NetCDF-C reads. Build an original five-slice DICOM series with UInt16 pixels=reshape(uint16(1:60),3,4,5), uncompressed Explicit VR Little Endian transfer syntax UID 1.2.840.10008.1.2.1, row-direction [1,0,0], column-direction [0,1,0], PixelSpacing=[2,3] mm (row, column spacing), and slice-k origin [10,20,30+4*(k-1)] mm. One-based voxel row 2, column 3, slice 4 has intensity 44 and patient position [16,22,42] mm. Independently check P=origin+(column-1)*3*row-direction+(row-1)*2*column-direction; the direction of a row advances the column index. Preserve intensity and slice ordering. Unsupported transfer syntax remains a codec error, never a blank successful image.

Advanced design: [AF41](#feature-af41).

Learning references: [AT41](#source-at41).

<a id="task-j42"></a>

#### J42 — Automotive, aerospace and UAV scenarios with validated dynamics

Priority P3; relative size XL; discipline Modeling/compiler/domain. Historical baseline: planned (2026-10-14).

Prerequisites: [Z01](#task-z01), [C08](#task-c08), [G02](#task-g02), [G04](#task-g04), [K01](#task-k01), [K05](#task-k05), [J30](#task-j30).

Acceptance: An original vehicle with speed 2 m/s, heading zero and initial position [0,0] m on a straight road reaches [10,0] m after 5 s. A declared bicycle model with wheelbase 2 m, speed 4 m/s and steer atan(0.5) rad has yaw rate 1 rad/s; with initial position [0,0] and heading zero, require x=4sin(t), y=4(1-cos(t)) m within 1e-5 m over 0..5 s.

An original UAV rigid body has mass 1 kg, initial position [0,0,10] m in a z-up frame, zero initial linear and angular velocities, level scalar-first quaternion [1,0,0,0], zero torque and no external disturbance. Gravity is [0,0,-9.80665] m/s² and upward thrust is [0,0,9.80665] N. It must retain altitude and quaternion norm within 1e-5 m and 1e-8 respectively for 10 s. A noiseless sensor aimed at an obstacle 5 m away returns 5 m at each scheduled timestamp. Replays reproduce actor/sensor logs; these tests do not establish aero/vehicle fidelity or calibrated digital-twin validity.

Conditional variant “geographic scene coordinates”: [J41](#task-j41).

Conditional variant “physical component dynamics”: [J32](#task-j32).

Conditional variant “Model/hybrid integration”: [B02](#task-b02), [B03](#task-b03).

Conditional variant “Scenario batches”: [H01](#task-h01).

Advanced design: [AF42](#feature-af42).

Learning references: [AT42](#source-at42).

<a id="task-j43"></a>

#### J43 — Production services and deployable scientific applications

Priority P3; relative size XL; discipline Services/interoperability. Historical baseline: planned (2026-10-14).

Prerequisites: [H01](#task-h01), [K01](#task-k01), [K03](#task-k03), [R02](#task-r02).

Acceptance: Original endpoint returns f(x)=2*x+1. Send [1,2,3] and require [3,5,7] with the declared shape and nargout. Concurrent isolated requests cannot observe another request's sentinel variable. Cancel a deliberately long request, verify terminal state and a succeeding next request. Unknown entry point and malformed shapes fail clearly. Repeating an idempotency key retrieves one durable committed result under the declared retention policy. A crash/retry exercise checks duplicate suppression; external side effects require their own transactional adapter and are not promised universal exactly-once behavior.

Conditional variant “browser apps with reusable components”: [J14](#task-j14).

Conditional variant “Durable asynchronous remote scheduling”: [H04](#task-h04).

Conditional variant “Hosted browser user apps”: [A05](#task-a05), [X03](#task-x03).

Advanced design: [AF43](#feature-af43).

Learning references: [AT43](#source-at43).

<a id="task-j44"></a>

#### J44 — Instrument control and streamed data acquisition

Priority P3; relative size XL; discipline Services/interoperability. Historical baseline: planned (2026-10-14).

Prerequisites: [K01](#task-k01), [K03](#task-k03), [R02](#task-r02), [J03](#task-j03).

Acceptance: An original loopback instrument splits the response to MEAS? into multiple network chunks and returns 1.250 plus LF; require one parsed value 1.25 with units V, no duplicate callback and correct retained framing. Test byte payload [0,1,255,10] losslessly. A silent response times out without freezing UI; close/reconnect invalidates old callbacks. A device claim additionally records named hardware/driver, calibrated input and actual sample/trigger/drop results.

Conditional variant “Compute-worker processing”: [H01](#task-h01).

Conditional variant “Live graphical scopes”: [G04](#task-g04).

Advanced design: [AF44](#feature-af44).

Learning references: [AT44](#source-at44).

<a id="task-j45"></a>

#### J45 — Transactional database queries and typed result streaming

Priority P3; relative size XL; discipline Services/interoperability. Historical baseline: planned (2026-10-14).

Prerequisites: [C01](#task-c01), [V01](#task-v01), [V03](#task-v03), [K03](#task-k03), [J05](#task-j05).

Acceptance: Original local database rows have identifiers 001,002,003, with measurements 2,NULL,5. A bound query selects measurement>1 in explicit identifier order and returns 001/003 as text, preserving NULL in the full result. Roll back an inserted row and confirm row count remains three. Bind the text value containing quote/SQL-looking characters and require no unintended additional rows; cancel a long query and release its cursor.

Conditional variant “CSV import/export”: [C02](#task-c02).

Advanced design: [AF45](#feature-af45).

Learning references: [AT45](#source-at45).

<a id="task-j46"></a>

#### J46 — Graph and network algorithms with inspectable results

Priority P3; relative size XL; discipline Services/interoperability. Historical baseline: planned (2026-10-14).

Prerequisites: [C05](#task-c05), [C06](#task-c06), [C01](#task-c01), [G02](#task-g02), [G03](#task-g03).

Acceptance: Original directed weighted edges A-B=2,A-C=1,B-D=2,C-D=5,B-C=1 give unique shortest path A-B-D and total cost4. For a separate capacity network A-B=3,A-C=2,B-C=1,B-D=2,C-D=3, require max flow5 with conservation at B/C and equal cut capacity. Invalid negative capacities fail; deleting an edge invalidates cached result; plot relayout leaves graph/answers unchanged.

Advanced design: [AF46](#feature-af46).

Learning references: [AT46](#source-at46).

<a id="task-j47"></a>

#### J47 — Custom simulation blocks, callback lifecycles and masked libraries

Priority P3; relative size XL; discipline Simulation/architecture/text/orbit. Historical baseline: planned (2026-10-14).

Prerequisites: [B01](#task-b01), [B02](#task-b02), [B03](#task-b03), [K04](#task-k04), [K01](#task-k01), [R03](#task-r03), [A02](#task-a02), [J29](#task-j29).

Acceptance: An original scalar double delay has initial DWork state 0, sample time 0.1 s and no direct feedthrough. At ticks 0,0.1,0.2 s, apply inputs [1,2,3]; Outputs reads previous state and Update stores current input, giving exactly [0,1,2]. Record one Outputs then one Update per tick for this fixed discrete profile; a second instance starts independently at 0, and a fresh run resets state.

An original masked saturation block with bounds [0,2] maps [-1,1,3] to [0,1,2]. Changing the upper bound to 1.5 between runs changes only the last result to 1.5. Invalid bounds fail before simulation; an undeclared callback phase or direct-feedthrough contradiction produces a source-linked diagnostic. These proofs do not establish arbitrary SimStruct ABI, continuous solver callback or existing C MEX binary compatibility.

Conditional variant “System-object block backend”: [J01](#task-j01).

Conditional variant “native source blocks”: [J08](#task-j08).

Conditional variant “generated blocks”: [J35](#task-j35).

Advanced design: [AF47](#feature-af47).

Learning references: [AT47](#source-at47).

<a id="task-j48"></a>

#### J48 — Architecture components, typed interfaces, allocations and engineering views

Priority P3; relative size XL; discipline Simulation/architecture/text/orbit. Historical baseline: planned (2026-10-14).

Prerequisites: [B01](#task-b01), [U05](#task-u05), [C01](#task-c01), [R01](#task-r01), [A02](#task-a02), [Z01](#task-z01).

Acceptance: An original architecture has Sensor, Controller and Actuator components, four leaf ports and two connectors: Sensor.out→Controller.measurement and Controller.command→Actuator.in. Measurement is {value:double scalar in meters, valid:logical scalar}; Command is {u:double scalar in newtons}. Save/reopen must preserve exactly three components, four ports, two connectors, two interface definitions and every stable ID. An output→output connection or incompatible meter/newton element is rejected transactionally.

Declare an original serial-path latency analysis summing Sensor=2 ms, Controller=3 ms and Actuator=1 ms exactly once: total is 6 ms, satisfying a linked requirement ≤7 ms and failing ≤5 ms. Renaming Controller preserves the connectors and allocation ID; changing its latency marks prior analysis evidence stale. Reordering a filtered view cannot alter the sum. This is architecture analysis, not a hardware deadline measurement or qualification claim.

Conditional variant “requirements/test traceability”: [J38](#task-j38).

Conditional variant “behavior-model allocation”: [J29](#task-j29).

Advanced design: [AF48](#feature-af48).

Learning references: [AT48](#source-at48).

<a id="task-j49"></a>

#### J49 — Text tokenization, sparse document features and topic-model workflows

Priority P3; relative size XL; discipline Simulation/architecture/text/orbit. Historical baseline: planned (2026-10-14).

Prerequisites: [C01](#task-c01), [C06](#task-c06), [P04](#task-p04), [V01](#task-v01), [G02](#task-g02), [K05](#task-k05), [A01](#task-a01), [Z01](#task-z01).

Acceptance: Use original document IDs d1,d2,d3 with text ["red blue red","blue green",""]; the declared profile uses lowercase whitespace tokens and vocabulary ["blue","green","red"]. Its document-by-term counts are [1,0,2;1,1,0;0,0,0], token totals [3,2,0], and nonzero count 4. Preserve d3 as an empty row with an explicit empty-document policy. Transforming "red yellow" under the fixed-vocabulary ignore-unknown profile gives [0,0,1] and reports one unknown token.

For a separately declared one-topic posterior-mean profile with fixed per-word Dirichlet pseudo-count 1, disable topic-prior fitting and use WordConcentration=3 for this three-word vocabulary. Aggregate counts [2,1,2] yield topic-word probabilities [3/8,2/8,3/8] within 1e-8 and topic mixture 1 for each nonempty document. Higher-topic comparisons must allow declared label permutation and compare likelihood/probability invariants; this fixture does not establish every fitlda algorithm or language model.

Conditional variant “deferred large document corpora”: [J06](#task-j06).

Conditional variant “neural text embeddings”: [J23](#task-j23).

Advanced design: [AF49](#feature-af49).

Learning references: [AT49](#source-at49).

<a id="task-j50"></a>

#### J50 — Satellite orbits, ground-station access and reproducible link budgets

Priority P3; relative size XL; discipline Simulation/architecture/text/orbit. Historical baseline: planned (2026-10-14).

Prerequisites: [Z01](#task-z01), [C01](#task-c01), [C08](#task-c08), [G02](#task-g02), [G03](#task-g03), [P02](#task-p02).

Acceptance: Use double precision for these scalar oracles. Take an original spherical Earth of radius 6371000 m, station [6371000,0,0] m and satellite [6871000,0,0] m in the same Earth-fixed frame at the same epoch. Range is exactly 500000 m, elevation 90 degrees and access true for a 10-degree mask. The opposite satellite [-6871000,0,0] is Earth-occluded. For a separate unperturbed inertial circular orbit of radius r=6871000 m and mu=3.986004418e14 m³/s², set position [r,0,0], velocity [0,sqrt(mu/r),0]; at T/4, T=2*pi*sqrt(r³/mu), require position [0,r,0] within 1 m.

For range 500000 m, frequency 2e9 Hz and c=299792458 m/s, require FSPL=20*log10(4*pi*range*frequency/c)=152.447783221883 dB within 1e-9 dB. With transmit power 10 dBW, gains 20/30 dBi and other loss 2 dB, received power is -94.447783221883 dBW. These spherical, two-body and far-field proofs do not establish SGP4, terrain, atmospheric or standards-conformance claims.

Conditional variant “RF link budgets and communications waveforms”: [J28](#task-j28).

Conditional variant “Earth-surface raster scenes”: [J41](#task-j41).

Advanced design: [AF50](#feature-af50).

Learning references: [AT50](#source-at50).

<a id="advanced-families"></a>

## Advanced capability designs

Each family maps to one J task. MA is the advanced program track; the implementation milestone and rollout stages identify its integration context. The following are original functional and interface design descriptions, not captures of a finished interface. Supported API variants and source/installed evidence must be declared separately.

<a id="feature-af01"></a>

### AF01 — Advanced classes, property contracts and object lifetime

Implementation task: [J01](#task-j01). Group: Language and data; integration milestone: M8.

How it works: Value objects copy independently; handle aliases share identity. Advanced properties, inheritance and destruction require verified evaluator contracts and safe instance inspection.

Flow: Class/source definition → Metadata and access rules → Value or handle storage → Dispatch and lifetime → Inspector and original errors.

Interface design: Workspace rows show class, value/handle kind and object validity. An object inspector has Members and Class tabs with access, storage and validation badges. Dependent values show an Evaluate action and busy/error state. Deletion invalidates all aliases visibly; private properties remain noneditable. Source navigation opens the owning original class file and preserves the selected instance.

Construction:

1. Inventory each attribute, inheritance combination, constructor/error path and destruction trigger against R01; save original source fixtures with K04 mappings.
2. Expose read-only class/member metadata and stable object tokens through the worker; include kernel epoch, validity and revision without invoking getters during preview.
3. Test native value/handle behavior first; implement missing dispatch and property validation at the evaluator boundary, retaining alias identity and shared nested handles.
4. Add explicit inspector operations for accessor evaluation, setters, copy and delete; access violations must return original source diagnostics without changing state.
5. Gate each declared subset with lifetime, exception, cycle and constructor-order fixtures; publish deviations and migrate validated semantics into N01/N02 only after equivalence checks.

API surface to qualify: `classdef`, `handle`, `matlab.mixin.Copyable`, `copy`, `delete`, `isvalid`, `dynamicprops`, `addprop`, `properties (Dependent)`, `properties (Transient)`, `properties (Abstract)`, `matlab.lang.WeakReference`.

Boundary: Base MATLAB language/desktop APIs; optional MathLab backends require separate runtime/profile gates.

<a id="feature-af02"></a>

### AF02 — Function argument blocks and name-value resolution

Implementation task: [J02](#task-j02). Group: Language and data; integration milestone: M8.

How it works: Argument blocks bind defaults, conversions, validation and name-value options before function execution; release-specific rules and original source diagnostics must be preserved.

Flow: Authored function → Argument syntax tree → Positional/name-value binding → Validation and defaults → Source-mapped invocation.

Interface design: Editor signature help separates positional inputs from optional names, shows defaults and required shapes, and lists profile support. Validation failure highlights the original declaration and offending call. Command Window provides value/class/shape diagnostics. Unsupported contracts receive a clear pre-execution error; they are never shown as successfully validated because a function ran.

Construction:

1. Freeze supported block forms and binding/conversion/error precedence; write small source fixtures independently of vendor examples, including default expressions referencing earlier inputs.
2. Extend the block-aware parser with dimensions, class, validators, name-value syntax and source locations; do not insert executable helpers until parsing completes.
3. Implement ordered binding and validation with typed values; reject unsupported repeating/output forms before entering the function body and track actual error IDs.
4. Feed signature metadata into completion and hover; preserve the requested source revision through source preparation, debugger stops and validation failures.
5. Add original positive/negative contract fixtures, verify no body side effects on invalid input, and reuse the same contract tests when implementing the Rust evaluator.

API surface to qualify: `arguments`, `arguments (Repeating)`, `arguments (Output)`, `mustBePositive`, `mustBeMember`, `mustBeNumeric`, `inputParser`, `name=value`, `mustBeNonempty`.

Boundary: Base MATLAB language/desktop APIs; optional MathLab backends require separate runtime/profile gates.

<a id="feature-af03"></a>

### AF03 — Events, listeners, timers and guaranteed cleanup

Implementation task: [J03](#task-j03). Group: Language and data; integration milestone: M5.

How it works: Events and timers dispatch owned callbacks; cleanup follows object lifetime. Queue conflicts, graceful interruption and forced worker termination need distinct guarantees.

Flow: Object or timer owner → Listener registration → Bounded event queue → Owning-thread callback → Cleanup and invalidation.

Interface design: A session activity pane lists timer mode, next scheduled run, callbacks executed, pending count and stopped/error state. Event logs link callback source and epoch. Stop and Delete are separate actions. On restart, the pane marks old resources invalid. Cleanup failures attach to the original job and remain inspectable without overwriting the primary error.

Construction:

1. Define callback payloads, ownership, reentrancy, exception propagation and destruction rules; preserve native event behavior where proven by the selected Octave build.
2. Introduce callback records with object token, epoch, source revision and registration order; discard deleted targets and old-session events before dispatch.
3. Implement singleShot and fixedSpacing timers first, then fixedRate conflict modes using a deterministic test clock; bound the queue and retain useful lag telemetry.
4. Hook cleanup into normal return, caught/uncaught errors and supported interruption; define what cannot be guaranteed after worker crash or forced kill.
5. Exercise restart, delete-during-callback, nested event, callback exception and escaped cleanup references; enable graphical callbacks only after the same dispatch checks pass.

API surface to qualify: `events`, `notify`, `addlistener`, `listener`, `SetObservable`, `PostSet`, `onCleanup`, `timer`, `start`, `stop`, `delete`, `drawnow`.

Boundary: Base MATLAB language/desktop APIs; optional MathLab backends require separate runtime/profile gates.

<a id="feature-af04"></a>

### AF04 — Custom indexing, operator dispatch and class reflection

Implementation task: [J04](#task-j04). Group: Language and data; integration milestone: M8.

How it works: Custom indexing needs overload-aware reads/writes and output counts. Reflection must preserve declared namespace/profile semantics and avoid side-effecting evaluation during inspection.

Flow: Index or operator syntax → Class precedence lookup → Declared overload dispatch → Typed result contract → Reflection and diagnostics.

Interface design: The class browser shows inheritance, methods, properties and index overrides with source links. Object view distinguishes stored fields from accessor previews. Index-expression preview presents requested outputs and dispatch target. Editing a virtual element reports the actual setter used, supports stale-request rejection, and retains the original value if evaluation fails.

Construction:

1. Inventory indexing dispatch forms, output counts, end contexts, operator precedence and reflection namespaces; include base-class/subclass method lookup cases.
2. Build a class metadata graph tied to immutable original source; expose attributes without constructing objects or evaluating dependent getters.
3. Add structured index chains and requested output counts to worker operations; route each supported overload with correct assignment and error semantics.
4. Implement inspector/edit transaction paths against overload-aware operations; never bypass subsasgn by mutating serialized properties and calling the result an object edit.
5. Validate chained accesses, repeated indices, errors and reflection source identity; defer unproven modular mixins or metadata aliases with a named missing-capability diagnostic.

API surface to qualify: `subsref`, `subsasgn`, `subsindex`, `numel`, `end`, `matlab.mixin.indexing.RedefinesParen`, `matlab.mixin.indexing.RedefinesBrace`, `matlab.mixin.indexing.RedefinesDot`, `metaclass`, `?ClassName`, `matlab.metadata.Class.fromName`.

Boundary: Base MATLAB language/desktop APIs; optional MathLab backends require separate runtime/profile gates.

<a id="feature-af05"></a>

### AF05 — Tables, timetables, calendars, time zones and grouped joins

Implementation task: [J05](#task-j05). Group: Language and data; integration milestone: M3.

How it works: Typed tables preserve heterogeneous data and metadata. Timetable joins/resampling, timezones and calendar arithmetic require explicit missing, duplicate and daylight-saving policies.

Flow: Typed columns and times → Schema/calendar contract → Join/group/synchronize plan → Missing and time-zone rules → Bounded table inspector.

Interface design: Variable Editor shows pinned row-time column, units, variable descriptions, timezone and missing indicators. A transform pane previews Join, Group and Resample with explicit keys/bins/method, output count and generated source. Ambiguous local times receive a visible resolution control. Sorting the view is separate from changing source table order.

Construction:

1. Freeze row/variable indexing, metadata, key ordering, category and missing semantics; record IANA database version and timezone ambiguity policy in the runtime profile.
2. Implement lossless column payloads and datetime representations that distinguish UTC instants, local/unzoned values, display formats and missing values.
3. Add declared key joins/group reductions with explicit multiplicity and missing behavior; retain source row provenance in preview and generated code.
4. Implement retime and synchronize in separate interpolation/aggregation increments; specify time-bin endpoint, duplicates and calendar versus elapsed-step options.
5. Run original DST, mixed-zone, duplicate-key, empty-group and missing fixtures; package the selected zone database and verify installed projects reproduce the same results.

API surface to qualify: `table`, `timetable`, `innerjoin`, `outerjoin`, `findgroups`, `splitapply`, `groupsummary`, `retime`, `synchronize`, `datetime`, `duration`, `calendarDuration`, `caldays`, `categorical`.

Boundary: Base MATLAB language/desktop APIs; optional MathLab backends require separate runtime/profile gates.

<a id="feature-af06"></a>

### AF06 — Datastores, lazy tall plans and bounded-memory MapReduce

Implementation task: [J06](#task-j06). Group: Language and data; integration milestone: M6.

How it works: Datastores feed blocks into deferred tall or MapReduce plans; gathered output and global operations still require measured memory limits and explicit execution profiles.

Flow: Files and schema → Partition reader → Deferred typed plan → Spill/repartition/reduce → Budgeted gather and preview.

Interface design: Workspace shows tall type, known columns and unknown row extent instead of fabricated dimensions. A plan view lists operations, passes, reader progress, working-memory peak and spill bytes. Preview displays a bounded sample with provenance. Evaluate and Gather are separate actions; oversized result collection explains the limit and offers checkpoint export.

Construction:

1. Define datastore reader contracts: block types, reset/partition, source revision, errors and cancellation; select CSV and one chunked binary format first.
2. Implement lazy plan nodes for projection, filtering and associative reduction; track estimated sizes, source passes and capability restrictions before execution.
3. Add spill partitions and bounded shuffle for grouped operations; refuse arbitrary callbacks whose effects make reordering unsafe, and state numerical reduction ordering.
4. Integrate gather/result-size checks, checkpoints, progress and cancellation with K01/H01; preserve a deterministic source manifest and invalidate stale checkpoints.
5. Prove memory ceilings on data larger than the configured cap and correctness across block sizes; only then enable selected parallel/remote plans through H02/H04.

API surface to qualify: `datastore`, `tabularTextDatastore`, `fileDatastore`, `read`, `hasdata`, `reset`, `partition`, `tall`, `gather`, `mapreduce`, `matlab.tall.transform`, `matlab.tall.reduce`, `write`.

Boundary: Base MATLAB local datastore/tall/MapReduce; parallel pools require Parallel Computing Toolbox and vendor clusters require MATLAB Parallel Server.

<a id="feature-af07"></a>

### AF07 — MAT 7.3, HDF5 partial access and object serialization

Implementation task: [J07](#task-j07). Group: Language and data; integration milestone: M3.

How it works: MAT 7.3 requires HDF5 metadata and references beyond ordinary HDF5; partial access, class restoration and unknown content need separate version/type contracts.

Flow: MAT/HDF5 object manifest → Class/shape/chunk metadata → Partial range access → Object reconstruction policy → Round-trip and limits report.

Interface design: A file inspector displays format/version, variable class/shape, chunks and supported actions. Selecting a slice loads only the requested region. Read-only is the default file mode; enabling writes names the target and records a transaction. Unknown classes show metadata and retained source-file path. Restore Object is a distinct execution action.

Construction:

1. Add version detection and a type/encoding inventory before reading; distinguish plain HDF5, MAT Level 5 family and genuine MAT 7.3 in compatibility records.
2. Implement lossless hyperslab reads/writes for numeric, complex and logical arrays with checked dimensions, chunk telemetry and a transactional write policy.
3. Add references, cells, structs, sparse values and selected containers one at a time; compare original files with an independent reader and licensed reference when available.
4. Define an explicit restore action for known trusted class serializers, separating safe metadata preview from execution; report absent classes and unsupported hooks.
5. Validate partial I/O and memory budgets, failed writes, unknown class retention and relocatable files through the installed Q05 workshop suite.

API surface to qualify: `save -v7.3`, `matfile`, `size(matObj,varName)`, `h5info`, `h5read`, `h5write`, `saveobj`, `loadobj`, `Transient`.

Boundary: Base MATLAB language/desktop APIs; optional MathLab backends require separate runtime/profile gates.

<a id="feature-af08"></a>

### AF08 — MEX source compatibility and stable C/Rust embedding

Implementation task: [J08](#task-j08). Group: Language and data; integration milestone: M8.

How it works: MEX source families and binary ABIs are distinct. A versioned MathLab C/Rust embedding interface must define ownership, errors, callbacks and runtime requirements.

Flow: Native source and ABI → SDK/profile compilation → Owned value/handle bridge → Isolated invocation → Diagnostics and ABI report.

Interface design: Extension Manager shows Source Build versus Binary Load, API family, target, symbols and compatibility report. A build log links compiler errors to original files. Session Monitor exposes host jobs and owned value handles. ABI mismatch prevents execution with a precise diagnostic. Native embedding examples appear in offline documentation for the installed runtime.

Construction:

1. Publish extension API/ABI manifests with platform, architecture, index width, complex representation, compiler and runtime hashes; enumerate every supported mx/mex entry point.
2. Build original C extension sources in an isolated worker and pin compilation commands; use Octave oct-file interfaces for MathLab-native extensions where useful.
3. Define stable C session/value handles with size queries, owned buffers, explicit releases and error objects; expose an idiomatic Rust layer without leaking panics across FFI.
4. Implement callback and shutdown contracts, extension lifetime and unsupported-symbol diagnostics; isolate faults so a crashed extension cannot corrupt a shared desktop session.
5. Run clean-install host/client fixtures and verify memory ownership across repeated calls; assess binary loading or modern C++ support as separate later research work.

API surface to qualify: `mex`, `mexFunction`, `mxArray`, `mxGetDoubles`, `mxGetComplexDoubles`, `mexErrMsgIdAndTxt`, `mexAtExit`, `mexLock`, `matlab::mex::Function`, `engEvalString`, `engGetVariable`, `engPutVariable`.

Boundary: Base MATLAB MEX and Engine APIs; vendor MATLAB Runtime/Compiler SDK deployment is separate from the proposed MathLab runtime.

<a id="feature-af09"></a>

### AF09 — Optional Python, Java and .NET object interoperability

Implementation task: [J09](#task-j09). Group: Language and data; integration milestone: M8.

How it works: Optional Python, Java and .NET adapters need precise conversion, object lifetime, overload, exception and runtime-switching contracts under declared platform profiles.

Flow: Foreign runtime profile → Object/type conversion → Owned proxy lifetime → Call and error translation → Runtime/source diagnostics.

Interface design: Environment panel has Python, JVM and .NET cards with exact version, path, execution mode and Loaded/Unavailable/Error status. Workspace labels foreign objects by language and validity. Call errors retain both original foreign stack and MathLab call site. Restart invalidates that worker's handles visibly. Conversion preview names any copies or unsupported values.

Construction:

1. Publish per-language runtime/platform matrices and conversion rules for integer width, strings, null/missing, arrays, complex values, objects and exceptions.
2. Implement versioned request codecs and worker discovery; pin foreign executable/library/dependency versions in the project manifest without requiring them for core startup.
3. Add object token ownership, method resolution, explicit disposal and callback routing; restrict cross-runtime cycles and report what persists across calls.
4. Expose environment switching/restart and assembly/classpath actions; separate unavailable-runtime errors from unsupported method/type conversions.
5. Run original per-language fixtures under installed optional profiles, including worker crash, stale object and thrown exception; add embedding directions only through explicit independent adapters.

API surface to qualify: `pyenv`, `py.module.function`, `pyrun`, `pyrunfile`, `javaObject`, `javaMethod`, `javaaddpath`, `dotnetenv`, `NET.addAssembly`, `NET.unloadAssembly`.

Boundary: Base MATLAB language/desktop APIs; optional MathLab backends require separate runtime/profile gates.

<a id="feature-af10"></a>

### AF10 — Parameterized tests, mocks, coverage and measured memory

Implementation task: [J10](#task-j10). Group: Language and data; integration milestone: M5.

How it works: Test cases expand named parameters with fixture lifetimes; mocks, coverage and timing/allocation/memory reports need separately defined semantics and original-source identity.

Flow: Parameterized original tests → Fixture and mock lifecycle → Execution/coverage hooks → Measured result artifacts → Source-linked test report.

Interface design: Test Browser groups suites and expanded parameter cases with pass/fail/incomplete counts. Failure rows show actual/expected and original source. Coverage overlays show metric, denominator and uninstrumented reasons. Profiler separates exclusive/inclusive CPU time, allocation bytes and process peaks. Export includes runtime profile and source hash, so stale results can be identified.

Construction:

1. Define test discovery, parameter naming/combination, fixture scopes, pass/fail/incomplete and expected-error behavior with independent examples; retain test source hashes.
2. Implement a selected function/class-test adapter and event stream; track teardown after failures and isolate stateful parameter factories between test cases.
3. Add mock behavior/interaction records using verified object dispatch; diagnose unsupported save/load of mocks or unsupported mocked methods.
4. Instrument executable source statements with revision maps; report uncovered/uninstrumentable code separately, and introduce additional coverage metrics only with explicit semantics.
5. Connect A03 timing to measured allocator/RSS telemetry; export profile, fixture environment, instrumentation overhead and original failure navigation to installed release evidence.

API surface to qualify: `matlab.unittest.TestCase`, `TestParameter`, `ClassSetupParameter`, `MethodSetupParameter`, `matlab.unittest.TestSuite`, `matlab.mock.TestCase`, `createMock`, `verifyCalled`, `CodeCoveragePlugin`, `profile`, `profiler`.

Boundary: Base MATLAB unittest/mocking/statement coverage; current vendor decision, condition and MC/DC code coverage require MATLAB Test. MathLab memory instrumentation is proposed.

<a id="feature-af11"></a>

### AF11 — Project isolation, build task graphs and toolbox lifecycle

Implementation task: [J11](#task-j11). Group: Language and data; integration milestone: M5.

How it works: Project and package lifecycle requires explicit paths, dependencies, startup code, transactional updates and task graphs with tested freshness and failure behavior.

Flow: Project/build manifest → Dependency task graph → Isolated build environment → Versioned package assembly → Install/update/rollback report.

Interface design: Project pane shows active root, runtime profile, dependencies and pending startup actions. Build view renders a dependency list with stale/up-to-date/running/failed/blocked states and output links. Package preview shows files, licenses and declared startup code. Install and Activate are separate states. Failed updates retain the previous version and explain the conflicting constraint.

Construction:

1. Define project manifest version, relative paths, runtime lock, explicit startup/shutdown entry points and package ownership; separate project execution from safe metadata inspection.
2. Implement cycle-checked task DAGs with source/input/output hashes and declared environment; choose conservative rebuild behavior until cache invalidation is proven.
3. Connect tests/help/example builds and package assembly to the graph; failed prerequisites block downstream outputs, and canceled work does not publish incomplete archives.
4. Add install/update/remove transactions with version constraints, notices, project-scoped activation and rollback; validate imported vendor manifests before conversion.
5. Run relocation, dependency conflict, startup failure, repeat-build and installed tutorial fixtures; treat mltbx import/export and vendor build classes as independently tested compatibility increments.

API surface to qualify: `currentProject`, `openProject`, `buildtool`, `buildplan`, `matlab.buildtool.Plan`, `matlab.buildtool.Task`, `matlab.addons.toolbox.packageToolbox`, `matlab.addons.install`, `matlab.addons.uninstall`.

Boundary: Base MATLAB language/desktop APIs; optional MathLab backends require separate runtime/profile gates.

<a id="feature-af12"></a>

### AF12 — Symbolic assumptions, arbitrary precision and generated functions

Implementation task: [J12](#task-j12). Group: Language and data; integration milestone: M8.

How it works: Symbolic assumptions, exact numbers, arbitrary precision and generated functions need explicit domain/session rules and independent checks under a named symbolic backend.

Flow: Expressions and assumptions → Symbolic expression graph → CAS/precision adapter → Declared transformations → Numeric/generated validation.

Interface design: Symbolic inspector displays expression, exact/approximate mode, significant digits, active assumptions and backend. An assumption edit marks dependent results stale. Generate Function opens a source preview with argument order and unsupported-operation warnings. Run Comparison shows symbolic and generated numerical results with declared tolerance. Long simplifications can be canceled without corrupting session state.

Construction:

1. Inventory supported symbolic nodes, exact numbers, assumptions and branch conventions; retain backend version and per-session symbol identities in the runtime profile.
2. Implement exact rational construction and expression codecs before approximate conversion; distinguish exact decimal strings, binary double values and arbitrary precision objects.
3. Build explicit replacement/addition/clear assumption operations and bounded symbolic evaluation; preserve undecidable relations instead of coercing them to true.
4. Generate reviewable m-functions from a validated expression subset with explicit argument order; optionally produce C through SymPy codegen behind a separate compiler/profile gate.
5. Validate original algebra/calculus identities and precision error bounds; evaluate generated functions at independently chosen inputs and publish unsupported branch/solver behavior.

API surface to qualify: `sym`, `syms`, `assume`, `assumeAlso`, `assumptions`, `simplify`, `isAlways`, `solve`, `vpa`, `digits`, `matlabFunction`, `ccode`, `fortran`.

Boundary: Symbolic Math Toolbox; MATLAB Coder/Compiler deployment and vendor symbolic internals are separate unsupported/unassessed product contracts.

<a id="feature-af13"></a>

### AF13 — 3D scenes, volumes, cameras, lighting and animation

Implementation task: [J13](#task-j13). Group: Language and data; integration milestone: M3.

How it works: Geometry, cameras, lighting, volume extraction and animation require separate numerical, object-interaction and rendering checks while preserving complete full-resolution source data.

Flow: Scene and volume objects → Typed geometry/materials → Camera and lighting state → Renderer/event bridge → Animation/export comparison.

Interface design: Figure has a 3D toolbar with Rotate, Pan, Zoom and Reset Camera; object tree and inspector share selection. Volume controls expose slice position, threshold, colormap and opacity with source-generation preview. A timeline has frame/clock/play/pause states. Busy extraction retains the last valid scene; errors attach to the operation and permit cancellation.

Construction:

1. Freeze supported geometry objects, transforms, camera fields, interpolation, color/alpha and lighting modes; record differences separately from numerical extraction APIs.
2. Extend the revisioned graphics object schema for vertices/faces, normals, scalar fields and camera state with stable IDs; avoid full scene copies for every update.
3. Implement rotate/pan/zoom and property edits that round-trip to engine properties; retain full-resolution source arrays and support bounded texture/mesh uploads.
4. Add slice/isosurface workflows and animation dirty regions with a bounded event queue; separate redraw throttling from callback suppression and computation state.
5. Validate geometry analytically, interaction semantics structurally and exports visually; introduce transparency, headless rendering and accelerated large volumes as named profile gates.

API surface to qualify: `plot3`, `surf`, `mesh`, `patch`, `slice`, `isosurface`, `isonormals`, `streamline`, `view`, `campos`, `camtarget`, `camup`, `camproj`, `camlight`, `lighting`, `material`, `animatedline`, `drawnow`.

Boundary: Base MATLAB 3D/volume primitives; specialist volume, geographic and domain apps may belong to additional products.

<a id="feature-af14"></a>

### AF14 — Live tasks, reusable components and app code ownership

Implementation task: [J14](#task-j14). Group: Language and data; integration milestone: M5.

How it works: Live tasks generate reviewable code; reusable components and app designers require explicit source ownership, output invalidation and loss-reporting format adapters.

Flow: Live task or app source → Component/control metadata → Data binding and callbacks → Owned code generation → Editable packaged artifact.

Interface design: Live task card has controls, generated-code disclosure, Run/Reset and stale/error badges. Designer offers canvas, component tree, properties and Code tabs. Generated regions have clear ownership markers; callback methods remain editable. Unsupported imported components appear as named placeholders with conversion details. Save/export reports format and losses and preserves the original file.

Construction:

1. Define versioned live cell/task and app/component schemas with stable IDs, resource paths and generated-versus-user-owned source sections; preserve original imported bytes.
2. Implement task parameter state, validation and deterministic generated source; changing a parameter invalidates dependent outputs rather than quietly showing stale results as current.
3. Add setup/update component lifecycle and event/callback plumbing through tested handle semantics; map repeated property changes to bounded scheduled updates.
4. Build designer operations with source-aware renames, undo and callback navigation; generated layout edits must preserve user callback text byte-for-byte unless explicitly refactored.
5. Import declared mlx/mlapp/plain-text subsets with loss inventories, then package original apps via X03; verify clean-install examples, resources, recovery and canceled edits.

API surface to qualify: `matlab.task.LiveTask`, `generateCode`, `setup`, `reset`, `matlab.task.configureMetadata`, `matlab.ui.componentcontainer.ComponentContainer`, `update`, `HasCallbackProperty`, `uifigure`, `uigridlayout`, `.mlx`, `.mlapp`.

Boundary: Base MATLAB language/desktop APIs; optional MathLab backends require separate runtime/profile gates.

<a id="feature-af15"></a>

### AF15 — Reusable decompositions, generalized eigenproblems, and sparse Krylov solvers

Implementation task: [J15](#task-j15). Group: Scientific computing; integration milestone: M4.

How it works: Reusable factors amortize repeated solves; generalized eigenproblems include a second matrix. Sparse Krylov methods apply operators and preconditioners without forming dense inverses, returning convergence diagnostics.

Flow: Matrix/type contract → Sparse storage and ordering → Factorization/preconditioner → Solve or spectral iteration → Residual and reuse evidence.

Interface design: A Numerical Solve inspector shows matrix class/shape, factor type, condition warning, sparse nonzeros, residual curve, and solve count. States are analyzing, factoring, iterating, converged, stagnated, failed, and canceled; retained partial iterates carry their termination reason. Editing inputs marks reports stale.

Construction:

1. Inventory real/complex single/double variants, rectangular versus square inputs, singular pencils, branch conventions, function-handle operators, and every output shape. Expose capability records per routine rather than granting an entire matrix-algorithm family.
2. Wrap Octave routines behind immutable matrix snapshots and generation-scoped factor handles. Preserve permutations, scaling, conjugate-transpose state, and least-squares policy; prevent later edits to the original workspace variable from corrupting cached factors.
3. Evaluate LAPACK/SuiteSparse candidates exposed by the selected Octave build; retain a Rust FFI service boundary for future engines. These libraries supply algorithms, not evidence of MATLAB option or object compatibility.
4. Implement operator callbacks, preconditioner solves, restart budgets, and cancellation at iteration boundaries. Return both requested-profile diagnostics and raw backend telemetry; keep sparse entries sparse and separate factorization memory from solve memory.
5. Add Schur-based matrix-function adapters with explicit branch/singularity diagnostics, then generalized eigenvalue residual checks. Publish supported factors and object operations only after installed examples exercise persistence, cleanup, and repeated right-hand sides.

API surface to qualify: `decomposition`, `eig(A,B)`, `qz`, `expm`, `logm`, `sqrtm`, `sylvester`, `pcg`, `gmres`, `ichol`, `ilu`.

Boundary: Freeze the named toolbox/product, release, supported API variants and backend/target boundaries in this feature's profile; see its detailed implementation card.

<a id="feature-af16"></a>

### AF16 — Constrained nonlinear, LP, QP, MILP, and global optimization

Implementation task: [J16](#task-j16). Group: Scientific computing; integration milestone: M4.

How it works: Solvers minimize objectives under bounds and constraints, exposing termination and feasibility. Global searches explore multiple candidate regions; a local optimum or small residual alone does not certify global optimality.

Flow: Variables/objective/constraints → Scaled problem model → Solver/derivative adapter → Termination certificate → Feasibility and optimum report.

Interface design: An Optimization workbench places variables and bounds at left, objective/constraint forms in the center, and iteration, feasibility, and gap charts at right. States distinguish preparing, evaluating, feasible incumbent, converged, budget exhausted, infeasible, unbounded, callback error, and canceled. Export includes options and generated source.

Construction:

1. Define one solver-neutral problem representation with objective, gradient/Hessian, linear rows, nonlinear callbacks, bounds, integer indices, and units. Translate documented MATLAB syntax into it while retaining original variable shape and source locations.
2. Start with pinned Octave optim routines and assess HiGHS for LP/convex QP/MILP plus NLopt for named nonlinear/global methods. Candidate availability does not establish matching defaults, option names, iteration sequences, or exit flags.
3. Map derivative callbacks across the worker boundary with revision identity and explicit evaluation counts. Provide finite-difference checks, sparse Jacobian transport, and safe callback interruption; never hide an infeasible or nonfinite evaluation behind a successful result.
4. Preserve incumbent, lower bound, absolute/relative gap, maximum constraint violation, and KKT evidence as separate report fields. Add start-point sweeps and seeded stochastic methods with saved budgets; global certification requires a documented solver certificate.
5. Implement app-to-source round trips and package solver profiles with versions and notices. Expand problem-based expressions only after their transformations produce equivalent constraints, gradients, and installed test results for the declared subset.

API surface to qualify: `fmincon`, `linprog`, `quadprog`, `intlinprog`, `optimoptions`, `optimproblem`, `optimvar`, `solve`, `ga`, `patternsearch`, `MultiStart`, `GlobalSearch`.

Boundary: Freeze the named toolbox/product, release, supported API variants and backend/target boundaries in this feature's profile; see its detailed implementation card.

<a id="feature-af17"></a>

### AF17 — GLMs, ensembles, cross-validation, and Bayesian diagnostics

Implementation task: [J17](#task-j17). Group: Scientific computing; integration milestone: M4.

How it works: GLMs connect a response distribution to predictors through a link; ensembles combine learners. Held-out folds estimate generalization, Bayesian optimization chooses hyperparameters, and MCMC separately samples a target distribution.

Flow: Typed dataset and split → Training-only preprocessing → Estimator/inference backend → Held-out diagnostics → Reproducible analysis artifact.

Interface design: A Model Analysis workspace combines a dataset/fold panel, response and predictor selectors, model comparison table, and residual/confusion/calibration tabs. Training and tuning have separate progress states; invalid folds, rank deficiency, nonconvergence, stale preprocessing, and missing backends show actionable diagnostics. A Bayesian tab labels optimizer traces or posterior chains explicitly.

Construction:

1. Specify model objects independently of their storage backend: predictor names/categories, distribution/link, coefficient ordering, observation exclusions, weights, and response type. Freeze train/predict serialization and rejection rules before adding interactive model comparisons.
2. Extend pinned Octave statistics routines and assess statsmodels GLMs and scikit-learn ensembles through optional isolated Python adapters. These are candidate algorithms; formula coding, categorical references, class scores, and MATLAB model methods need independent conformance.
3. Make preprocessing part of each training fold, including missing-value fitting, scaling, and feature selection. Store row identifiers and fold assignments so held-out observations cannot enter transformations or hyperparameter tuning unnoticed.
4. Add seeded ensemble fitting, calibration, residual/influence views, and Bayesian objective traces. Implement log-density MCMC callbacks separately, reporting acceptance, warmup, effective sample estimates, and chain diagnostics without labeling optimization traces posterior samples.
5. Export compact prediction artifacts and full audit artifacts separately. Installed tutorials must reproduce declared fold losses from original data, diagnose singular designs/separation, and preserve backend/version and stochastic policy when models are reopened.

API surface to qualify: `fitglm`, `fitcensemble`, `fitrensemble`, `cvpartition`, `crossval`, `kfoldPredict`, `kfoldLoss`, `predict`, `plotResiduals`, `bayesopt`, `mhsample`.

Boundary: Freeze the named toolbox/product, release, supported API variants and backend/target boundaries in this feature's profile; see its detailed implementation card.

<a id="feature-af18"></a>

### AF18 — Streaming DSP, System objects, wavelets, and time-frequency analysis

Implementation task: [J18](#task-j18). Group: Scientific computing; integration milestone: M4.

How it works: Streaming objects retain filter state across frames. Spectral windows trade time and frequency resolution; wavelets vary resolution with scale, and normalization and boundary extension determine reported coefficients.

Flow: Frames and sample clocks → Stateful DSP object → Transform/filter kernel → State/reset and latency → Trace and reconstruction proof.

Interface design: A Streaming Analyzer has transport controls above linked time trace, spectrum, and scalogram panes. A right panel shows filter/wavelet parameters, frame latency, sample rate, units, queue depth, and dropped frames. States are configured, running, paused, draining, stopped, underrun, and error. Parameter edits identify retained versus reset filter state.

Construction:

1. Represent streams with sample rate, channel count, timestamp, frame length, latency, and sample class. Implement stateful Octave wrapper objects with explicit reset/release behavior, and distinguish tunable controls from configuration changes that require unlocking.
2. Pin signal/DSP package functions and assess FFT libraries plus PyWavelets as candidate implementations. Publish coefficient normalization, wavelet definitions, extension modes, and overlap conventions per adapter; another transform library is not compatibility proof.
3. Build a Rust bounded frame queue with backpressure, drop accounting, and processing cancellation. Retain numerical filter state separately from rendering buffers so scope decimation or a closed chart cannot change the resulting samples.
4. Implement sample-accurate frame continuation, STFT overlap-add reconstruction, spectral unit conversion, and wavelet scale/frequency metadata. Report startup transients and cone-of-influence regions rather than presenting edge coefficients as uniformly reliable.
5. Add scope recording, replay, and generated scripts before device acquisition. Package an original offline signal source and its exact expected transforms, then validate real audio/device adapters separately with measured clock and underrun behavior.

API surface to qualify: `dsp.FIRFilter`, `dsp.SineWave`, `dsp.NotchPeakFilter`, `spectrumAnalyzer`, `reset`, `release`, `isLocked`, `stft`, `istft`, `spectrogram`, `cwt`, `icwt`, `dwt`, `idwt`.

Boundary: Freeze the named toolbox/product, release, supported API variants and backend/target boundaries in this feature's profile; see its detailed implementation card.

<a id="feature-af19"></a>

### AF19 — Robust control, model predictive control, and system identification

Implementation task: [J19](#task-j19). Group: Scientific computing; integration milestone: M4.

How it works: Identification estimates models from input/output histories; robust control studies uncertainty. MPC repeatedly estimates state, solves a constrained finite-horizon control problem, applies its first move, and updates controller state.

Flow: Plant/data and units → Identify or uncertainty model → Controller/optimizer design → Closed-loop prediction → Margins/constraints/fit report.

Interface design: A Control workbench shows plant/model cards and uncertainty ranges at left, response and sensitivity plots centrally, and constraints/horizon/cost controls at right. Identification adds validation residuals. MPC playback shows predicted versus applied trajectories and active constraints; infeasible, unstable, inconclusive robustness, canceled, and stale-model states remain distinguishable.

Construction:

1. Extend Octave control wrappers with consistent continuous/discrete time, units, named channels, initial conditions, and uncertain-parameter descriptions. Store identification estimation/validation splits and lag definitions with model objects rather than relying on global workspace state.
2. Assess SLICOT-backed control routines and python-control as candidates for linear/robust algorithms; evaluate a constrained QP service for MPC. None supplies blanket MATLAB uncertain-object, tuning-app, solver-default, or nonlinear-MPC compatibility.
3. Implement ARX regression with stable QR solves, residual diagnostics, delay/order selection, and explicit rank tests. Validate generated models against untouched input/output histories, exposing prediction horizon and initial-state policy with each fit score.
4. Build MPC state and warm-start services with constraint scaling, move/output limits, horizon, and sample time. Each controller step records feasibility and first applied action; deadline or solver failure uses an explicitly configured fallback policy.
5. Add uncertainty sweeps, H-infinity synthesis/bound reports, and source-generating tuning views. Distinguish sampling evidence from certified robust bounds; package independently validated examples before linking controllers to the simulation scheduler or hardware.

API surface to qualify: `ss`, `tf`, `iddata`, `arx`, `ssest`, `compare`, `resid`, `ureal`, `uss`, `mixsyn`, `hinfsyn`, `robuststab`, `mpc`, `mpcstate`, `mpcmove`, `nlmpc`.

Boundary: Freeze the named toolbox/product, release, supported API variants and backend/target boundaries in this feature's profile; see its detailed implementation card.

<a id="feature-af20"></a>

### AF20 — parfor classification, SPMD, distributed arrays, and pool scheduling

Implementation task: [J20](#task-j20). Group: Scientific computing; integration milestone: M6.

How it works: Parallel loops classify independent data slices and reductions before scheduling. SPMD runs coordinated worker programs; distributed arrays describe global shape while partitions and collective communication reside across workers.

Flow: Loop or distributed input → Variable/partition analysis → Worker and RNG allocation → Local compute and reduction → Shape/result/cancel proof.

Interface design: A Parallel Monitor shows pool type and worker tiles above an iteration/chunk timeline. The array inspector displays global shape plus partition ownership and transfer estimates. States include starting, executing, waiting on collective, draining, stopped, and failed; a source diagnostic highlights rejected variable classification. A failed worker invalidates affected outputs.

Construction:

1. Define a semantic analyzer for sliced, broadcast, reduction, temporary, and loop variables with source-local diagnostics. Reject dependent loop writes and unsupported dynamic indexing before worker startup; preserve one-based array shape and deterministic output placement.
2. Use independent Octave processes for the first supported subset, each with locked packages and original source revisions. Build Rust pool orchestration with explicit startup, idle, busy, draining, and failed states rather than executing arbitrary interpreter code on unsafe shared threads.
3. Assess Rayon for Rust-owned kernels and Open MPI for later SPMD collectives; both are candidate infrastructure. Neither validates MATLAB loop classification, worker-local persistent state, distributed object methods, or reduction rounding semantics.
4. Create distributed descriptors containing partition axes, global dimensions, class, worker ownership, and generation. Implement selected map, sum, transpose, and gather operations; validate collective participation, prohibit silent whole-array copies, and expose communication bytes.
5. Specify scheduler chunk policy, capacity, timeouts, worker failure cleanup, seeded substreams, and thread/process capability tables. Installed examples must compare serial and parallel results while recording floating reduction tolerances and runtime profile.

API surface to qualify: `parfor`, `parpool`, `gcp`, `spmd`, `spmdIndex`, `spmdSize`, `distributed`, `codistributed`, `codistributor1d`, `gather`, `parallel.pool.Constant`.

Boundary: Freeze the named toolbox/product, release, supported API variants and backend/target boundaries in this feature's profile; see its detailed implementation card.

<a id="feature-af21"></a>

### AF21 — Futures, data queues, background workers, and remote batch jobs

Implementation task: [J21](#task-j21). Group: Scientific computing; integration milestone: M6.

How it works: Asynchronous evaluation returns a future before completion. Queues deliver progress while work runs; batch jobs package isolated execution, and remote schedulers retain job identity and artifacts for later retrieval.

Flow: Job input and revision → Future/queue scheduler → Isolated local/remote worker → Progress and durable artifacts → Reconnect/cancel result state.

Interface design: A Jobs panel lists location, profile, source revision, state, progress, elapsed time, and outputs. A detail drawer provides queue events, logs, retry history, and cancellation status. Reconnecting or cancel-requested remains visible until acknowledgement; failed outputs cannot overwrite workspace variables. Import results is an explicit revision-aware action.

Construction:

1. Introduce a versioned future/job protocol with queued, running, finished, failed, cancel-requested, and canceled states. Give each input snapshot, output set, source revision, and error a stable identity; fetching a failed future must surface its original stack.
2. Implement isolated Octave workers first and assess Tokio channels/process supervision for Rust orchestration. Tokio is candidate infrastructure, not a MATLAB threading or future-object implementation; background-thread functions require an audited capability allowlist.
3. Provide bounded progress queues with creator ownership, ordered per-producer events, callback revision checks, and explicit backpressure. UI callbacks execute on the client event loop; kernel restart invalidates old callbacks without attaching them to a new session.
4. Add an authenticated remote transport with pinned environment manifests, content-addressed inputs, reconnectable job IDs, logs, checksummed outputs, and cancellation acknowledgement. Retries cannot publish two successful result sets or implicitly rerun non-idempotent tasks.
5. Implement a scheduler adapter boundary, local batch exercise, and remote failure/recovery tests. Distinguish detached durable jobs from interactive futures; document client-close policy, retained artifacts, cleanup, and exact thread/process support in the installed help.

API surface to qualify: `parfeval`, `backgroundPool`, `parallel.pool.DataQueue`, `send`, `afterEach`, `afterAll`, `fetchNext`, `fetchOutputs`, `cancel`, `batch`, `parcluster`, `createJob`, `createTask`.

Boundary: Freeze the named toolbox/product, release, supported API variants and backend/target boundaries in this feature's profile; see its detailed implementation card.

<a id="feature-af22"></a>

### AF22 — GPU array residency, CUDA kernels, precision, and GPU code generation

Implementation task: [J22](#task-j22). Group: Scientific computing; integration milestone: M6.

How it works: GPU arrays retain device memory across supported operations; gather copies results to the host. Custom kernels declare launch geometry. Automatic CUDA generation is a separate compiler workflow with constrained inputs.

Flow: Host/device typed array → Explicit transfer ownership → Device kernel dispatch → Synchronization and gather → Precision/memory/device proof.

Interface design: A GPU inspector shows selected hardware, free/used memory, resident variables, precision, and transfer counters. Kernel details show inputs and launch dimensions; a compiler view later adds readiness diagnostics and generated files. States distinguish no compatible device, ready, allocating, executing, synchronizing, device lost, and failed. CPU/GPU execution is labeled.

Construction:

1. Extend the array service with device ID, precision, complex/sparse layout, owner generation, synchronization state, and allocation lifetime. A workspace preview requests bounded data pages instead of gathering complete device arrays behind the user's back.
2. Assess ArrayFire as an open-source array runtime candidate and direct CUDA bindings for named NVIDIA operations. CUDA/cuBLAS are vendor runtime dependencies, not open-source replacements or proof of MATLAB residency, sparse, or precision compatibility.
3. Implement selected transfer, creation, arithmetic, GEMM, and FFT operations with capability checks and explicit synchronization at result boundaries. Record host/device bytes, allocation peaks, kernel duration, and transfer duration separately; reject unsupported device functions before mutation.
4. Add a constrained CUDA-kernel adapter validating argument types, read/write ownership, grid/block sizes, and launch errors. Restart or device reset invalidates old handles; asynchronous errors must be associated with the launching source revision.
5. Treat automatic code generation as a later typed-IR project: infer bounded shapes, identify supported parallel kernels, compile declared targets, and verify generated artifacts. Runtime GPU execution does not complete the separate compiler/code-generation release gate.

API surface to qualify: `gpuArray`, `gather`, `gpuDevice`, `gpurng`, `arrayfun`, `parallel.gpu.CUDAKernel`, `feval`, `mexcuda`, `gpucoder`, `coder.gpu.kernelfun`.

Boundary: Freeze the named toolbox/product, release, supported API variants and backend/target boundaries in this feature's profile; see its detailed implementation card.

<a id="feature-af23"></a>

### AF23 — dlnetwork, labeled tensors, autodiff, custom training, and ONNX

Implementation task: [J23](#task-j23). Group: Scientific computing; integration milestone: M8.

How it works: Labeled differentiable tensors carry dimensions and operations; reverse-mode autodiff computes parameter gradients. Custom training controls minibatches, state, and updates, while ONNX exchange requires explicit operator and layout mapping.

Flow: Labeled tensors and parameters → Supported forward graph → Scalar loss and reverse tape → Optimizer/checkpoint state → Gradient/training/import proof.

Interface design: A Training workspace has a network graph, tensor/parameter inspector, loss/validation curves, epoch and minibatch progress, device/memory summary, and checkpoint controls. States are initialized, training, paused, validating, diverged, canceled, and complete. ONNX import presents supported/unsupported nodes and proposed layout conversions before creating a runnable network.

Construction:

1. Define labeled tensor descriptors, broadcasting rules, network topology, named learnables, and nonlearnable state in an engine-owned schema. Audit layout conversion between column-major language arrays and backend tensors; labels must not silently transpose batch or channel axes.
2. Evaluate LibTorch/PyTorch through Rust C++ FFI or an isolated optional worker, with ONNX Runtime for inference. These are candidate backends, not matching MATLAB tensor formatting, layer definitions, optimizer defaults, or import/export behavior.
3. Implement a declared primitive autodiff set and custom model-loss callback contract, tracking tape lifetime and stale tensors. Verify analytical gradients before adding layers, and separate training-state updates from inference so repeated predictions remain state-consistent.
4. Add minibatch iteration, seeded initialization, checkpoints, explicit optimizer state, pause/resume, and resource cancellation. Persist data order, preprocessing, dtype, device, and epoch counters; report NaNs and unavailable operators at their actual source.
5. Implement ONNX operator/opset capability inventories with shape and channel-order conversion reports. Round-trip selected original networks against an independent runtime; never execute imported custom code automatically or substitute a missing operator without a visible diagnostic.

API surface to qualify: `dlarray`, `dlnetwork`, `dlfeval`, `dlgradient`, `forward`, `predict`, `minibatchqueue`, `adamupdate`, `trainingProgressMonitor`, `trainnet`, `importNetworkFromONNX`, `exportONNXNetwork`.

Boundary: Freeze the named toolbox/product, release, supported API variants and backend/target boundaries in this feature's profile; see its detailed implementation card.

<a id="feature-af24"></a>

### AF24 — Reinforcement learning environments, agents, rollout, and training

Implementation task: [J24](#task-j24). Group: Scientific computing; integration milestone: M8.

How it works: An environment resets and steps through observations, rewards, and terminal state. An agent selects actions, collects experiences, and updates policy/value functions; evaluation rollout is distinct from training.

Flow: Environment step/reset → Observation/action contract → Policy/value update → Seeded rollout/checkpoint → Reward/termination evaluation.

Interface design: An RL workspace places environment/action specifications beside agent settings, episode reward and length curves, and a rollout timeline with observations/actions/rewards. Training and evaluation use separate controls. States include validating, collecting, updating, evaluating, paused, stopped, and failed; checkpoint restore shows which state components were recovered.

Construction:

1. Define observation/action specifications, reset seed, step result, reward, termination, and retained environment state as a versioned protocol. Validate shape, bounds, and finite values before agent construction; distinguish episode termination from budget truncation in adapters.
2. Implement original deterministic environments in Octave callbacks and assess Gymnasium plus Stable-Baselines3 as optional isolated candidates. Map their conventions explicitly; algorithm names alone do not establish MATLAB policy object, update schedule, or episode compatibility.
3. Build a reproducible experience/rollout store with transition IDs, episode boundaries, discount policy, and replay sampling state. Implement a small tabular agent first, followed by named neural agents through the tested autodiff service and frozen hyperparameter profiles.
4. Separate training, exploration, deterministic evaluation, and checkpoint restoration. Persist optimizer/replay/environment state when claiming resume equivalence; parallel rollouts need independent seeded streams, bounded queues, and explicit policy-version tagging.
5. Add learning curves, policy export, and a simulation environment adapter only after its timestep and reset semantics pass fixtures. Evaluate policies on untouched original scenarios; stochastic learning performance requires repeated-run evidence rather than one attractive reward curve.

API surface to qualify: `rlFunctionEnv`, `rlNumericSpec`, `rlFiniteSetSpec`, `validateEnvironment`, `rlDQNAgent`, `rlPPOAgent`, `rlSACAgent`, `train`, `sim`, `rlTrainingOptions`, `rlSimulationOptions`.

Boundary: Freeze the named toolbox/product, release, supported API variants and backend/target boundaries in this feature's profile; see its detailed implementation card.

<a id="feature-af25"></a>

### AF25 — PDE and finite elements, adaptive meshes, boundaries, and transient physics

Implementation task: [J25](#task-j25). Group: Scientific computing; integration milestone: M8.

How it works: Finite elements discretize geometry into basis functions and assemble sparse systems from material laws and boundary conditions. Transient models also assemble mass terms; refinement changes approximation and error estimates.

Flow: Geometry/units/boundaries → Frozen mesh and basis → Sparse residual assembly → Linear/nonlinear/time solve → Refinement and field error.

Interface design: A FEM workbench contains geometry/mesh/region trees, a 2D/3D canvas, and assignment panels for materials, boundaries, and time integration. Mesh-quality and convergence plots accompany results and a time scrubber. States include unmeshed, stale assignments, assembling, solving, refining, converged, singular, and canceled; incomplete traces are labeled.

Construction:

1. Create a geometry/mesh/coefficient schema with stable boundary IDs, units, element order, material laws, loads, initial values, and solution times. Geometry edits invalidate affected assignments.
2. Assess MFEM and pinned meshing/sparse dependencies through a C/Rust service. Candidate algorithms do not validate MATLAB geometry APIs, element choices, physical models, result objects, or defaults.
3. Assemble scalar Poisson/heat stiffness, consistent mass, and loads with Dirichlet/Neumann/Robin conditions. Verify normal directions, boundary integrals, callbacks, and sparse assembly before nonlinear or coupled physics.
4. Add residual estimators, refinement, transient integrators, and resource budgets tied to mesh revisions. Preserve accepted steps on cancellation; diagnose singular all-Neumann and inconsistent initial/boundary-condition cases.
5. Implement interpolation, gradients, fluxes, and region queries; expand structural/electromagnetic/multiphysics profiles separately. Package manufactured solutions, refinement studies, and units so each domain has reproducible mathematical evidence.

API surface to qualify: `femodel`, `createpde`, `generateMesh`, `specifyCoefficients`, `applyBoundaryCondition`, `materialProperties`, `edgeBC`, `faceBC`, `solve`, `solvepde`, `pdemesh`, `pdeplot`, `interpolateTemperature`.

Boundary: Freeze the named toolbox/product, release, supported API variants and backend/target boundaries in this feature's profile; see its detailed implementation card.

<a id="feature-af26"></a>

### AF26 — Computer vision, medical 3D segmentation, registration, calibration, and tracking

Implementation task: [J26](#task-j26). Group: Scientific computing; integration milestone: M8.

How it works: Segmentation assigns pixel/voxel labels; registration estimates physical transforms. Calibration maps world geometry to image coordinates, and tracking associates detections through time while retaining uncertainty and lifecycle state.

Flow: Images/volumes/calibration → Coordinate and label model → Vision/registration kernel → Tracking/segmentation state → Physical-coordinate validation.

Interface design: A Vision/Volume workspace offers three linked orthogonal slices plus 3D labels, dataset/label trees, intensity window controls, and physical coordinates. Registration uses overlay/blink and residual views; calibration shows reprojection errors. States identify loading, editing, stale transform, optimizing, tracking, lost track, and failed metadata. Export includes annotations and spatial reference.

Construction:

1. Define lossless image/volume descriptors with dtype, intensity scale, channel order, voxel spacing, orientation, origin, and physical coordinate convention. Preserve label definitions and independent annotation revisions; require explicit resampling when spatial frames differ.
2. Extend Octave image primitives and assess OpenCV for calibration/features/tracking plus ITK/SimpleITK for 3D registration/medical geometry. These candidates do not establish MATLAB object classes, transform direction, interpolation, or medical-file metadata fidelity.
3. Implement threshold/morphology and manual slice annotations first, with original voxel fixtures and transactional undo. Add registration optimizer/metric/pyramid controls, showing moving-to-fixed versus sampling-transform conventions and preserving masks through nearest-neighbor resampling.
4. Add calibration parameter estimation and reprojection diagnostics, then feature and detection tracking with association, covariance, tentative/confirmed/lost states, and timestamp checks. Deep-learning inference is a separate profile requiring validated model/operator provenance.
5. Build DICOM/NIfTI adapters and three orthogonal linked views, then independent import/export round trips. Package synthetic phantoms and pinhole/tracking fixtures; domain workflows require geometry and numerical checks beyond visually aligned overlays.

API surface to qualify: `medicalVolume`, `medicalImageLabeler`, `volumeSegmenter`, `imbinarize`, `imregtform`, `imwarp`, `estimateCameraParameters`, `cameraIntrinsics`, `worldToImage`, `vision.PointTracker`, `trackerGNN`.

Boundary: Freeze the named toolbox/product, release, supported API variants and backend/target boundaries in this feature's profile; see its detailed implementation card.

<a id="feature-af27"></a>

### AF27 — Robotics rigid-body trees, motion planning, SLAM, ROS 2, and sensor fusion

Implementation task: [J27](#task-j27). Group: Scientific computing; integration milestone: M8.

How it works: Rigid-body trees compose joint transforms; planners search valid configurations. SLAM jointly estimates poses/maps, sensor fusion combines measurements with covariance, and ROS 2 transports typed messages under explicit frame/time/QoS conventions.

Flow: Robot and sensor frames → Kinematics/dynamics graph → Plan/estimate/fuse state → ROS/simulation adapter → Trajectory/frame/collision proof.

Interface design: A Robotics workspace pairs link/frame trees with a 3D robot/map scene, joint sliders, path controls, and sensor timelines. An estimator view shows covariance and accepted loop closures; ROS monitoring lists nodes/topics and QoS. States distinguish valid pose, IK failure, collision, planning timeout, localization uncertainty, disconnected transport, and stale sensor data.

Construction:

1. Define joint/link/frame schemas with transform convention, meters/radians, limits, inertias, collision geometry, and named coordinate frames. Implement original Octave-facing objects and revisioned Rust scene descriptors; URDF import must report unsupported joints/material references.
2. Assess Pinocchio for kinematics/dynamics and OMPL for planning through isolated native adapters. OMPL requires a collision/validity integration; these candidates do not prove MATLAB rigid-body objects, joint ordering, collision semantics, or planning-status compatibility.
3. Implement forward kinematics/Jacobians first, then constrained IK and collision-checked trajectories. Report residual pose error and validity independently; interpolated segments require checking at the declared resolution, not just successful endpoint tests.
4. Add pose-graph and filtering services with normalized orientations, covariance checks, gauge fixing, sensor timestamps, and replayable original measurements. Distinguish loop-closure hypotheses from accepted constraints and reject unobservable or inconsistent estimation setups.
5. Integrate ROS 2 through a pinned middleware profile with message schemas, frame trees, QoS, discovery, bag replay, and reconnect diagnostics. Keep hardware transport separate from simulation, enforce stale-command policy, and test named physical targets only after offline fixtures pass.

API surface to qualify: `rigidBodyTree`, `rigidBody`, `rigidBodyJoint`, `getTransform`, `inverseKinematics`, `geometricJacobian`, `manipulatorRRT`, `lidarSLAM`, `poseGraph`, `insfilterErrorState`, `ros2node`, `ros2publisher`, `ros2subscriber`.

Boundary: Freeze the named toolbox/product, release, supported API variants and backend/target boundaries in this feature's profile; see its detailed implementation card.

<a id="feature-af28"></a>

### AF28 — Communications, 5G standards, RF networks, radar, and phased arrays

Implementation task: [J28](#task-j28). Group: Scientific computing; integration milestone: M8.

How it works: Communication chains map bits into coded resource grids and waveforms. RF networks represent complex port responses; array phases steer beams, while radar processing estimates range/Doppler from propagation delay and phase evolution.

Flow: Symbols/waveform/array → Named standard/profile → DSP/channel/RF kernels → Receiver/beam/radar state → Round-trip and conformance lanes.

Interface design: A Wireless/RF workspace uses a configuration tree and validated parameter form beside resource-grid, constellation, spectrum, beam-pattern, and range-Doppler tabs. Show standards version, sample rate, power normalization, and port units prominently. States include invalid allocation, generating, analyzing, unsupported standard option, hardware disconnected, and completed with validation evidence.

Construction:

1. Create separate domain profiles for modulation/coding, 5G resource mapping, RF network data, arrays, and radar. Record complex layout, sample rate, carrier frequency, coordinate/polarization conventions, port impedance, normalization, and standard revision in every result.
2. Extend signal primitives and assess srsRAN components for named 5G algorithms plus scikit-rf for RF networks. These candidates need version/license/distribution review and do not prove MATLAB configuration objects, reference vectors, or standards conformance.
3. Implement original QAM/OFDM/channel fixtures before standards mapping. Add each 3GPP clause as a traceable requirement with release/version, grid indices, coding/rate-matching rules, and independently authored vectors; unsupported configurations must reject instead of approximating silently.
4. Implement antenna steering and beamforming with declared phase, coordinate, and element-pattern conventions; add range/Doppler and detection chains separately. Validate physical units, bin-to-range/velocity conversion, and noise/false-alarm assumptions with analytical fixtures.
5. Add waveform/configuration source export, Touchstone read/write, spectrum/grid/beam views, and optional hardware adapters. Full standard or radio claims require independent conformance/interoperability evidence tied to the exact profile, rather than a successful waveform plot.

API surface to qualify: `qammod`, `qamdemod`, `ldpcEncode`, `ldpcDecode`, `nrCarrierConfig`, `nrResourceGrid`, `nrWaveformGenerator`, `nrOFDMModulate`, `nrOFDMDemodulate`, `sparameters`, `phased.ULA`, `phased.SteeringVector`, `phased.RangeDopplerResponse`, `radarTransceiver`.

Boundary: Freeze the named toolbox/product, release, supported API variants and backend/target boundaries in this feature's profile; see its detailed implementation card.

<a id="feature-af29"></a>

### AF29 — Multirate simulation, buses, variants and model references

Implementation task: [J29](#task-j29). Group: Modeling and deployment; integration milestone: M7.

How it works: Compiled sample times determine block execution. Rate transitions mediate exchanges; buses constrain signal structure. References supply reusable components, while variant controls select implementations at declared activation times.

Flow: Typed hierarchical graph → Sample-time/variant compile → State and rate scheduler → Bus/rate/reference execution → Deterministic trace inspection.

Interface design: The model canvas gains a Sample Times overlay with labels and a separate schedule table; color is supplementary. Selecting a bus opens a tree of element types, dimensions and units. A configuration panel previews the active variant and affected references before recompiling. Run states distinguish editing, compiling, running, paused and failed; changing a variant marks prior traces stale and retains their configuration snapshots.

Construction:

1. Extend the original Rust model schema with stable component identities, named bus element paths, shapes, units, sample periods and offsets. Preserve referenced source revisions and instance arguments so repeated instances share definitions while maintaining independent state.
2. Compile the active variant before graph scheduling. Resolve inherited rates and port types, validate reference cycles, and build direct-feedthrough dependencies. Begin with update-time variant selection; report unsupported startup, runtime and code-compile activation modes explicitly in migration reports.
3. Represent supported periods as rational ticks rather than accumulating floating-point time. Produce task groups, deterministic priority rules and explicit read/write phases. Reject unsupported irrational or inconsistent inherited rates with the offending port and dependency chain.
4. Implement selected fast-to-slow and slow-to-fast transition profiles with initial values, buffering and documented latency. Use Octave only for supported pure block kernels; maintain scheduling and bus transport in Rust rather than relying on worker message arrival order.
5. Build an original nested controller workshop with two referenced instances and alternative gains. Export the compiled schedule, active configuration and input/output trace. Add import adapters only after native scheduling fixtures pass, preserving all unsupported blocks as migration errors.

API surface to qualify: `sim`, `Simulink.SimulationInput`, `Simulink.Bus`, `Simulink.BusElement`, `Simulink.VariantControl`, `Rate Transition`, `Model block`.

Boundary: Freeze the named toolbox/product, release, supported API variants and backend/target boundaries in this feature's profile; see its detailed implementation card.

<a id="feature-af30"></a>

### AF30 — Stiff ODEs, DAEs, algebraic loops and event consistency

Implementation task: [J30](#task-j30). Group: Modeling and deployment; integration milestone: M7.

How it works: Stiff solvers solve implicit steps with error control. DAEs impose algebraic constraints and require consistent initialization. Algebraic loops need simultaneous solutions; zero crossings locate events between accepted steps.

Flow: Residuals and constraints → Consistent initialization → ODE/DAE step → Event localization and reset → Residual and event-order proof.

Interface design: Solver settings show equation form, backend, relative/absolute tolerances and initialization constraints. A residual inspector links failed constraints to model ports. The trace view marks accepted events and reset pairs, with pre-event and post-event values inspectable separately. Run status differentiates initialization failure, convergence failure, terminal event and user cancellation; partial results retain their validity interval instead of appearing as a completed solution.

Construction:

1. Freeze distinct contracts for explicit ODEs, mass-matrix DAEs and fully implicit residual systems F(t,y,y')=0. Record supported DAE index, singular-mass handling, Jacobian options and event directions; do not advertise one algorithm as every MATLAB solver.
2. Audit the pinned Octave solver profile with independent fixtures before reusing it. Evaluate SUNDIALS IDA behind a versioned native service for missing residual-system capabilities; record its algorithm and option translations instead of claiming identical internal stepping.
3. Compile direct-feedthrough strongly connected components into residual equations. Supply sparse Jacobian structure where supported, scaling and damped nonlinear iterations; expose convergence residuals. Diagnose singular or unsupported loops rather than silently inserting a delay that changes the model.
4. Add a consistency service with explicit fixed/free initial variables, residual norms and failure reasons. Preserve user constraints. Restart numerical history after declared state resets or parameter discontinuities, and recompute algebraic variables before publishing a valid post-event sample.
5. Implement event bracketing, direction filters, terminal flags and deterministic simultaneous-event policy. Write an original analytic DAE and bouncing-state workshop, including event traces and tolerance refinement; make accepted solver steps and requested output samples distinguishable in results.

API surface to qualify: `ode15s`, `ode23t`, `ode15i`, `decic`, `odeset`, `Mass`, `Jacobian`, `Events`.

Boundary: Freeze the named toolbox/product, release, supported API variants and backend/target boundaries in this feature's profile; see its detailed implementation card.

<a id="feature-af31"></a>

### AF31 — Hierarchical statecharts, parallel regions, history and temporal logic

Implementation task: [J31](#task-j31). Group: Modeling and deployment; integration milestone: M7.

How it works: Exclusive hierarchy selects one child; parallel regions can be active together but execute in defined order. Transitions run actions, history restores children, and temporal operators use specified event/time contexts.

Flow: Statechart hierarchy → Priority/history semantics → Event and temporal queue → Run-to-completion transitions → State/coverage trace.

Interface design: The chart editor provides a state tree, canvas and ordered transition list. Dashed region borders denote parallel decomposition; visible priority numbers let users review shared-data ordering. A debugger highlights the active configuration and lists microsteps, action source and pending timers. Pausing preserves event queue state. A history badge explains the stored child, and editing guards marks existing traces as belonging to the older revision.

Construction:

1. Design an original statechart representation with explicit parent/child relationships, exclusive or parallel decomposition, default transitions, junctions, guards and separate condition/transition/state actions. Preserve stable IDs for traces and undo; scope the supported action-language subset independently.
2. Compile a deterministic interpreter in Rust that performs initialization, entry, event dispatch, transition selection and exit using a documented semantics table. Separate evaluation priorities from canvas position and serialization order; make observable shared-variable updates part of the contract.
3. Implement history storage and scoped local data with clear initialization and reentry rules. Treat parallel regions as ordered logical execution rather than implicit OS threads. Bound local event broadcasts and detect nonterminating event cascades with a reproducible diagnostic trace.
4. Add separate event counters and simulation-time wakeups for supported temporal forms. Specify which entry or reentry resets each timer and counter, and whether intervals are sampled or scheduled. Connect continuous-time triggers through the shared event scheduler rather than desktop timers.
5. Author thermostat and supervisor/worker workshops that inspect active paths, state-local variables and transition actions. Expand imported charts only by validated syntax and semantics; unsupported graphical functions, messages, generated artifacts or protected content remain identified in an inventory.

API surface to qualify: `Stateflow.Chart`, `Stateflow.State`, `Stateflow.Transition`, `entry/during/exit actions`, `after`, `every`, `temporalCount`, `send`, `in`.

Boundary: Freeze the named toolbox/product, release, supported API variants and backend/target boundaries in this feature's profile; see its detailed implementation card.

<a id="feature-af32"></a>

### AF32 — Equation-based physical networks, units and coupled DAEs

Implementation task: [J32](#task-j32). Group: Modeling and deployment; integration milestone: M7.

How it works: Physical components express simultaneous equations rather than signal assignments. Connections enforce conservation and shared quantities; unit-aware equations form a DAE network initialized to satisfy constraints and chosen targets.

Flow: Unitful physical network → Equation and constraint IR → Consistent DAE assembly → Coupled solve/event reset → Conservation and analytic proof.

Interface design: A domain-colored physical canvas includes port symbols and unit text. Selecting a component opens parameters, variables, initial targets and equations. An equation inspector traces each residual back to the component or connection. Initialization has a dedicated result showing satisfied targets and conflicts. Scope panels show units and optional conservation/energy residuals; backend and model hash remain visible when comparing runs or exporting the workshop.

Construction:

1. Define original physical-domain schemas for across/through quantities, units, orientations and connection compatibility. Start with electrical voltage/current and translational position/force; keep physical conserving ports distinct from sampled signal ports and explicitly model conversion boundaries.
2. Implement dimension analysis and supported scale/offset conversions before numerical compilation. Normalize to documented solver units while retaining display units. Reject incompatible equations during compilation, with both dimensions and source expressions; treat affine temperature conversions separately from multiplicative units.
3. Flatten selected components and connection equations into a sparse residual IR. Track source-to-equation provenance, structural singularity and differential/algebraic variable classification. Use the validated DAE service for index-1 networks; index reduction and additional physical domains require their own gates.
4. Add consistent initialization with soft/high-priority target metadata and explicit diagnostics for overconstrained networks. Implement selected switches and discontinuities through solver events and reinitialization. Evaluate OpenModelica as an optional independent backend, preserving a clearly named native equation-language subset.
5. Publish original RC and mass-spring workshops with conservation ledgers, solver tolerances and energy plots. Add selected thermal or fluid components only after domain-specific units, boundary conditions and conservation tests pass; a rendered component icon never establishes physical correctness.

API surface to qualify: `Simscape component language`, `nodes`, `branches`, `equations`, `der`, `value`, `Solver Configuration`, `initial targets`.

Boundary: Freeze the named toolbox/product, release, supported API variants and backend/target boundaries in this feature's profile; see its detailed implementation card.

<a id="feature-af33"></a>

### AF33 — Entities, queues, resources and reproducible discrete-event studies

Implementation task: [J33](#task-j33). Group: Modeling and deployment; integration milestone: M7.

How it works: Discrete-event simulation advances a calendar as entities arrive, wait and complete service. Queue/server policies control movement; statistics aggregate waiting, occupancy and utilization over explicitly defined observation intervals.

Flow: Entities and resources → Stable event calendar → Queue/service transitions → Seeded replication → Wait/utilization confidence report.

Interface design: The queue editor presents capacity, policy and service settings in an inspector alongside an entity table. Animated movement supports explanation while an event timeline remains the authoritative view. Statistics panels show observation duration, warm-up policy and time-weighted definitions. An experiment tab displays replication seeds, completion/failure states and intervals; clicking an outlier replication opens its event log and resource-ownership history.

Construction:

1. Add original typed entity records with immutable identity, mutable attributes, creation time and logged lifecycle. Specify supported FIFO/priority policies, capacity, blocking and resource acquisition/release semantics; distinguish entities from numeric sample streams in ports and serialization.
2. Build a Rust event calendar keyed by simulation time, phase, priority and insertion sequence. Define simultaneous departure/arrival and zero-duration behavior explicitly. Support cancellation and rollback of obsolete scheduled events using generation tokens, and diagnose unbounded same-time event cascades.
3. Implement generators, queues, servers, resource pools and termination with exact time-weighted occupancy accounting. Record queue wait, service time, throughput, drops and resource ownership. Pair entity conservation checks with source-tagged event logs rather than deriving statistics from animation frames.
4. Create a replication runner through isolated jobs, with seed/stream assignment, warm-up boundaries, stopping rules and confidence-interval method recorded per experiment. Reuse pinned Octave statistics only for declared estimators; RNG equality with MATLAB requires separate exact-sequence evidence.
5. Author a deterministic single-server workshop before adding stochastic arrivals and service distributions. Export individual event sequences and a compact experiment manifest. Treat hybrid continuous/discrete coupling as an additional scheduling gate, not a side effect of sharing the same timeline view.

API surface to qualify: `Entity Generator`, `Entity Queue`, `Entity Server`, `Entity Terminator`, `MATLAB Discrete-Event System`, `Simulink.SimulationInput`, `Sequence Viewer`.

Boundary: Freeze the named toolbox/product, release, supported API variants and backend/target boundaries in this feature's profile; see its detailed implementation card.

<a id="feature-af34"></a>

### AF34 — FMI 2/3 interoperability and Modelica co-simulation

Implementation task: [J34](#task-j34). Group: Modeling and deployment; integration milestone: M7.

How it works: FMUs package metadata and executable interfaces. Model Exchange externalizes integration, Co-Simulation exchanges values at communication points, and FMI 3 Scheduled Execution externalizes partition scheduling; legal calls depend on lifecycle state/capabilities.

Flow: FMU/Modelica manifest → Version/capability checks → Lifecycle and variable binding → Master solver/co-simulation → Numerical plus standard report.

Interface design: The import wizard first displays metadata, interface kinds, variables, units, platforms and capability flags. Users select an interface and communication/solver profile before execution. A connection view shows exchanged variables and clocks; lifecycle errors link to the offending API call. The run inspector includes accepted communication times, early returns and cleanup status, making unsupported rollback or target-platform binaries visible before running.

Construction:

1. Freeze separate FMI 2.0.5 and 3.0.2 support matrices for interface kind, variable types, platforms and capability flags. Parse modelDescription.xml into bounded metadata records before loading binaries; inventory resources and dependencies, and disclose unsupported features without pretending to simulate them.
2. Implement explicit lifecycle state machines in a versioned Rust C-ABI adapter. Route executable FMUs through isolated workers, pair allocations with the originating library and guarantee termination/free-instance cleanup. Capture call/status traces, with binary loading separate from passive metadata preview.
3. Deliver Co-Simulation first using a documented communication grid, input hold policy and dependency ordering. Handle rejected steps, early return, events and state rollback only when the corresponding interface version and capabilities permit them; never assume every FMU supports save/restore.
4. Add Model Exchange through the validated solver service, correctly handling state/event modes and root localization. Add Scheduled Execution independently with clock periods, priorities and partition activation. Clocks do not imply that a desktop process meets wall-clock deadlines.
5. Evaluate OpenModelica-generated, source-available reference FMUs as independent interoperability fixtures and optional backend inputs. Create an original coupled decay/controller workshop with step-refinement plots; check notices and runtime dependencies. Protected models or vendor-licensed FMUs retain their explicit prerequisites.

API surface to qualify: `FMU Import block`, `FMI 2 Model Exchange/Co-Simulation`, `fmi3DoStep`, `fmi3SetClock`, `fmi3ActivateModelPartition`, `modelDescription.xml`.

Boundary: Freeze the named toolbox/product, release, supported API variants and backend/target boundaries in this feature's profile; see its detailed implementation card.

<a id="feature-af35"></a>

### AF35 — Generated C/C++, executable modules, MEX and SIL/PIL verification

Implementation task: [J35](#task-j35). Group: Modeling and deployment; integration milestone: M8.

How it works: Generation requires declared types and supported language/model constructs. MEX, standalone libraries and executables have distinct runtimes. SIL executes host code; PIL executes target code through an explicit connectivity configuration.

Flow: Frozen supported source → Typed subset IR → State/layout/code emission → Host or processor build → SIL/PIL and source trace.

Interface design: A Build panel lists entry points, inferred/declared types, output kind and target profile. Diagnostics link unsupported constructs to original source. A generated-code browser maps IR/model elements to emitted lines. Verification has distinct interpreted, native extension, SIL and PIL lanes with captured vectors and numerical differences; a target unavailable state is explicit. Artifact manifests identify compiler, runtime dependencies and relocatable output paths.

Construction:

1. Define a code-generation profile separate from general interactive execution: entry points, scalar/array types, bounded variable sizes, supported control flow, allocation rules and external calls. Analyze original source with precise diagnostics for dynamic features that cannot be emitted safely.
2. Lower the validated subset into a typed Rust IR with explicit column-major indexing, state updates and numerical operations. Use the semantic fixture suite as the shared contract. Begin with source-to-C and model-to-C kernels; avoid embedding a hidden Octave interpreter in supposedly standalone output.
3. Emit self-contained kernel headers, initialization/step/termination functions, build manifests and source maps. Pin compiler flags and target ABI, and track floating-point options. Native extension output uses MathLab's declared API; MATLAB/Octave MEX source recompilation and existing binary compatibility remain separate gates.
4. Create a host SIL harness that serializes test vectors, executes generated code in an isolated process and compares outputs/errors/state with the interpreter. Add PIL transport only for a named compiler, target processor or instruction-set simulator and actual connectivity configuration.
5. Publish an original matrix-transform and discrete-filter workshop producing source, library and executable outputs. Package all runtime dependencies and notices, then rerun the workshop outside the checkout. Add optimization only after correctness and traceability reports survive compiler and platform changes.

API surface to qualify: `codegen`, `coder.config`, `coder.typeof`, `coder.ceval`, `slbuild`, `MEX`, `software-in-the-loop`, `processor-in-the-loop`.

Boundary: Freeze the named toolbox/product, release, supported API variants and backend/target boundaries in this feature's profile; see its detailed implementation card.

<a id="feature-af36"></a>

### AF36 — Fixed-point arithmetic and embedded, HDL and PLC generation

Implementation task: [J36](#task-j36). Group: Modeling and deployment; integration milestone: M8.

How it works: Fixed-point values combine stored integers, scaling and arithmetic policies. Conversion uses observed/derived ranges. HDL adds clocks and pipelines; PLC generation emits scan-oriented code requiring separate target compilation and verification.

Flow: Ranges and quantization → Fixed-point typed IR → Overflow/rounding analysis → C/HDL/PLC target emission → Bit-accurate and target proof.

Interface design: A fixed-point inspector places stored integer, bit pattern, real value and numeric policy side by side. A conversion comparison view overlays float/fixed traces and flags overflow with input context. The target selector exposes C, RTL or PLC settings with distinct reports. An RTL timing view shows pipeline cycles, while a PLC view shows scan ordering. Build, simulation and hardware-validation status remain separate for each exact target.

Construction:

1. Implement a Rust fixed-point value descriptor with signedness, word length, binary-point or declared slope/bias scaling and explicit sum/product settings. Track stored bits independently from displayed real values; serialize arithmetic policy with arrays and model parameters.
2. Add deterministic quantization, rounding, saturation, wrap and widening rules using sufficiently wide integer intermediates. Freeze selected fi-compatible constructor and arithmetic variants independently; inspect pinned Octave capabilities rather than assuming a native integer type establishes fixed-point object compatibility.
3. Create range instrumentation and original static interval analysis for a bounded subset. Proposed widths must record observed coverage or assumptions, quantization error budgets and overflow examples. Keep floating reference, fixed implementation and derived-range evidence visible in the conversion report.
4. Introduce independent embedded-C, RTL and PLC backends over validated IR. RTL contracts include reset polarity, enable, latency and throughput; PLC contracts include scan order and supported Structured Text dialect. Evaluate Verilator/Yosys as candidate tools, while each target gets an explicit build profile.
5. Ship original FIR and tank-controller workshops with generated sources, testbench vectors and traceability. Simulation, synthesis, place-and-route, physical board execution and PLC IDE import each receive their own evidence gate; software tests do not confer hardware certification or general device support.

API surface to qualify: `fi`, `numerictype`, `fimath`, `Fixed-Point Tool`, `makehdl`, `checkhdl`, `plcgeneratecode`, `HDL Workflow Advisor`.

Boundary: Freeze the named toolbox/product, release, supported API variants and backend/target boundaries in this feature's profile; see its detailed implementation card.

<a id="feature-af37"></a>

### AF37 — Real-time target execution, HIL schedules and hardware I/O

Implementation task: [J37](#task-j37). Group: Modeling and deployment; integration milestone: M8.

How it works: Configured target hardware executes real-time applications with timed I/O; the host controls and instruments them. HIL closes a loop with physical controllers. Deadline, overrun and connectivity behavior belongs to the target profile.

Flow: Target and timing profile → Task/IO schedule → Target build and deployment → Measured HIL execution → Deadline/jitter/hardware evidence.

Interface design: A Target panel shows connection state, hardware identity, loaded application hash and supported channels. A deploy preview displays task periods, deadlines and I/O bindings. Start becomes available only for a validated loaded application. The run dashboard separates simulation time from host and target timestamps, plots jitter/overruns and exposes tuning acknowledgements. Disconnection marks live values stale and preserves the target's documented watchdog behavior.

Construction:

1. Define a target manifest covering CPU, OS/RTOS, compiler, driver versions, clock source, task periods, deadlines, watchdog and I/O map. Keep the desktop an orchestration client. Start with recorded virtual-time tests, then qualify one actual hardware/runtime combination for measured wall-clock behavior.
2. Generate deterministic fixed-step target code from the validated model subset, with allocation and blocking policies appropriate to that target. Compile task precedence and rate-transition buffers from the schedule; perform feasibility analysis with stated assumptions before accepting a deployment configuration.
3. Build versioned deployment and control transport for upload, verify-hash, load, start, stop and reconnect. Target status must include application identity and run epoch so late commands cannot operate on another experiment. Parameter updates are typed, range-checked and applied at declared safe boundaries.
4. Implement device adapters with timestamped samples, units, channel calibration and explicit stale/missing-data policy. Add overruns, jitter, task execution time and dropped samples to target telemetry. Keep logging off the critical path through bounded buffers and observable backpressure.
5. Create an original simulated-plant/controller HIL workshop with replay mode, fault injection, safe output policy and a recorded stop sequence. Evaluate Linux deadline scheduling only for a declared Linux target; pin drivers and repeat measurements on the promised hardware before any real-time release claim.

API surface to qualify: `slrealtime`, `load`, `start`, `stop`, `Simulink Real-Time Explorer`, `instrumentation`, `parameter tuning`, `target I/O blocks`.

Boundary: Freeze the named toolbox/product, release, supported API variants and backend/target boundaries in this feature's profile; see its detailed implementation card.

<a id="feature-af38"></a>

### AF38 — Requirements, model tests, structural coverage and formal verification

Implementation task: [J38](#task-j38). Group: Modeling and deployment; integration milestone: M8.

How it works: Requirements link to artifacts; tests compare behavior and coverage measures exercised structure. Formal analysis reasons under assumptions and produces counterexamples. Qualification requires specific tool/project evidence beyond passing tests.

Flow: Requirements and source IDs → Test/coverage/property model → Instrumented or bounded run → Trace/counterexample analysis → Scoped assurance evidence.

Interface design: An Engineering Evidence workspace combines a requirements table, traceability graph and model/test browser. A selected requirement shows linked revisions, test verdicts and stale evidence. Coverage overlays display each metric and denominator with excluded objectives visible. Formal result cards expose assumptions and bounds before the verdict; counterexamples open as replayable traces. Reports distinguish functional results, analysis scope and qualification status without a single misleading green approval badge.

Construction:

1. Create original requirement records with immutable IDs, revisioned text, rationale, verification method and bidirectional source/model/test links. Preserve links through renames and mark evidence stale after changes. Import selected formats without claiming full proprietary requirements-database compatibility.
2. Extend the test runner with reusable model harnesses, parameter sweeps, input sequences, numeric tolerances and explicit verdicts. Save model/profile hashes, actual/expected traces and simulation events. Distinguish requirement-based tests from automatically generated coverage-seeking tests in reports.
3. Instrument decision, condition and selected MC/DC coverage at the model/IR level, with defined short-circuit and statechart semantics. Map counters to source revision and disclose exclusions, unsupported constructs and unattained objectives; aggregate only compatible instrumentation profiles.
4. Add bounded formal checks for the supported finite-state/integer subset through a candidate solver such as CBMC. Encode assumptions, time/loop bounds and arithmetic precisely; return proved-within-scope, counterexample, unknown and unsupported as separate outcomes. Replay counterexamples through the simulator.
5. Integrate candidate static analysis for generated C/C++ and publish original evidence bundles. Tool qualification is a separate specialist program with operational requirements, configuration control, validation cases and independent review; certification and absence-of-error claims must never be inferred from an ordinary smoke suite.

API surface to qualify: `Requirements Toolbox links`, `Simulink Test harnesses`, `Simulink Coverage`, `sldvrun`, `Polyspace Bug Finder`, `Polyspace Code Prover`, `qualification evidence`.

Boundary: Freeze the named toolbox/product, release, supported API variants and backend/target boundaries in this feature's profile; see its detailed implementation card.

<a id="feature-af39"></a>

### AF39 — Econometric models, derivative pricing, portfolio risk and backtesting

Implementation task: [J39](#task-j39). Group: Modeling and deployment; integration milestone: M8.

How it works: Econometric objects separate specifications, estimation and forecasts. Pricing depends on instrument/date conventions; portfolios combine risk models with constraints. Backtests rebalance using trailing information and account for trades, cash and costs.

Flow: Market data/conventions → Model and portfolio contract → Fit/price/risk optimization → Cost-aware backtest → Independent price/risk proof.

Interface design: A Finance workspace presents a data timeline with training, warm-up and test boundaries, plus instruments and model specifications. Estimation panels show convergence and residual diagnostics. Portfolio views expose constraints, expected return/risk and infeasibility. Backtest charts link equity, holdings, trades, costs and available information at each decision time. Export generates the exact original script, data/profile manifest and a ledger suitable for independent audit.

Construction:

1. Define versioned financial data contracts for prices versus returns, adjusted versus raw observations, currency, trading calendar, day counts, missing dates and corporate actions. Keep market-provider adapters optional; author synthetic workshop data and retain provenance rather than bundling restricted vendor datasets.
2. Implement selected ARIMA/GARCH specifications and likelihood/forecast contracts over pinned numerical and optimization services. Record initialization, stationarity constraints, presample values, convergence and parameter uncertainty. Validate each selected API variant independently; a similar model object name does not establish econometric equivalence.
3. Add European option pricing and a selected instrument/calendar layer, evaluating QuantLib as a candidate pricing backend with explicit convention mappings. Build original closed-form and finite-difference fixtures for prices and Greeks; enumerate unsupported exotic instruments and calibration methods before expansion.
4. Create constrained portfolio models with named estimators, covariance treatment and optimizer status. Implement a backtest ledger with point-in-time input windows, rebalancing/execution timing, transaction costs, cash and delisting policy. Never allow future observations to leak into fit or allocation callbacks.
5. Deliver an original forecast/pricing/allocation/backtest workshop with frozen data splits and reproducible reports. Expand scenario risk, CVaR, credit and provider integrations as separate releases; results are computational evidence for declared algorithms, not an implied investment-performance guarantee.

API surface to qualify: `arima`, `garch`, `estimate`, `infer`, `forecast`, `blsprice`, `Portfolio`, `backtestStrategy`, `backtestEngine`, `runBacktest`.

Boundary: Freeze the named toolbox/product, release, supported API variants and backend/target boundaries in this feature's profile; see its detailed implementation card.

<a id="feature-af40"></a>

### AF40 — Bioinformatics pipelines and quantitative biological model fitting

Implementation task: [J40](#task-j40). Group: Modeling and deployment; integration milestone: M8.

How it works: Sequence containers retain identifiers, bases and quality/alignment metadata. Biological models combine species, reactions, doses and events. Fitting maps grouped observations to responses and minimizes a declared likelihood or weighted residual objective.

Flow: Sequence/count/model inputs → Coordinate/quality/units schema → Alignment or kinetic solver → Fit/pipeline execution → Independent biological fixture.

Interface design: The Genomics view provides read/quality tables, reference intervals and a pipeline graph with retained/excluded counts. The biological model editor combines species/reaction tables with a network diagram, dose timeline and units. A Fit panel maps observation columns to responses and shows estimates, bounds, residuals and uncertainty assumptions. Simulation and fit states distinguish convergence failure from success; SBML import displays supported packages and conversion losses before execution.

Construction:

1. Create separate genomics and systems-biology package profiles. For reads, freeze FASTA/FASTQ and selected SAM/BAM/CRAM fields, quality encodings, reference identity and coordinate conventions. Use HTSlib as a candidate adapter, with bounded streaming and explicitly translated indexing at the MATLAB-facing boundary.
2. Implement original read filtering, region retrieval and count summaries with preservation of identifiers and quality scores. Every pipeline stage records parameters, reference hashes, excluded-read counts and tool versions. Alignment, variant calling and differential-expression engines each require independent specialist scope and validation.
3. Design a reaction-network IR with species amount/concentration, compartments, parameters, units, stoichiometry and selected event/dose schedules. Compile supported deterministic models to the validated ODE/DAE service; keep stochastic simulation and mixed-effects fitting as separate future variants.
4. Build grouped parameter estimation using explicit response mappings, transforms, bounds and residual/error models over validated optimization services. Record solver choice, failures and identifiability diagnostics. Add SBML import by exact level/version/package and preserve unsupported semantics with explanatory errors.
5. Publish original tiny-read and one-compartment elimination workshops, with analytic solutions and synthetic observations. Include executable scripts, model/data manifests and numerical checks in the installed package; clinical validity or comprehensive biological-model interchange is outside these functional acceptance claims.

API surface to qualify: `BioRead`, `BioMap`, `sbiosimulate`, `sbiofit`, `groupedData`, `estimatedInfo`, `sbiodose`, `sbmlimport`.

Boundary: Freeze the named toolbox/product, release, supported API variants and backend/target boundaries in this feature's profile; see its detailed implementation card.

<a id="feature-af41"></a>

### AF41 — Geospatial rasters, NetCDF, DICOM and domain-aware visualization

Implementation task: [J41](#task-j41). Group: Modeling and deployment; integration milestone: M8.

How it works: Domain readers preserve coordinates, units and metadata. Geospatial references distinguish geographic/projected cells and postings; NetCDF supports multidimensional subsets, while DICOM pixel decoding depends on transfer syntax and image geometry.

Flow: Raster/NetCDF/DICOM source → CRS/packing/patient geometry → Lossless domain adapter → Transform/render/export → Coordinate and round-trip proof.

Interface design: An import preview lists dimensions, variable types, coordinates, missing indicators and available codecs. A map workspace shows layers, CRS and scale/units; a volume workspace shows linked axial/sagittal/coronal or native planes with image geometry. Data tips report both array indices and physical coordinates. Unsupported metadata remains accessible. Export previews declare resampling, compression and metadata retention; partially read data shows its selected subset instead of appearing complete.

Construction:

1. Split adapters into exact format/version/codec profiles: selected GeoTIFF/vector formats, NetCDF classic/enhanced types, and DICOM transfer syntaxes. Inventory file metadata before loading large arrays. Candidate GDAL, NetCDF-C and DICOM libraries require pinned versions, notices and documented type mappings.
2. Implement bounded hyperslab/tile/frame reads with MATLAB-facing one-based indices and column-major output. Preserve dimension order, fill/scale/offset policy and raw versus interpreted values explicitly. Reject unsupported compression or variable types without returning a misleading partially decoded dataset.
3. Add coordinate models for projected/geographic CRS, cell/posting grids and DICOM image position/orientation/spacing. Store transforms with values, handle axis order and north/south orientation, and require a declared reference system before combining layers. Never infer physical coordinates from array indices alone.
4. Develop map/volume views with tile caching, linked slice planes, metadata inspection and original-value data tips. Basemap/network layers have separate availability/license settings; offline synthetic examples remain executable. A rendering conversion must not overwrite scientific source data or silently resample it.
5. Publish original raster, packed NetCDF and oblique-image workshops that compare independent readers and exact metadata. Expand domain writer/round-trip behavior and remote access separately. DICOM decoding tests establish format behavior; medical interpretation and clinical deployment need their own specialist validation.

API surface to qualify: `readgeoraster`, `georasterinfo`, `ncread`, `ncinfo`, `dicominfo`, `dicomread`, `dicomreadVolume`, `geoscatter`, `geoplot`.

Boundary: Freeze the named toolbox/product, release, supported API variants and backend/target boundaries in this feature's profile; see its detailed implementation card.

<a id="feature-af42"></a>

### AF42 — Automotive, aerospace and UAV scenarios with validated dynamics

Implementation task: [J42](#task-j42). Group: Modeling and deployment; integration milestone: M8.

How it works: Scenarios define environments, actors, trajectories and sensors. Prescribed trajectories update pose; dynamic models integrate forces and state. Coordinate frames, sensor rates and random errors shape synthetic observations.

Flow: Scenario/frame conventions → Vehicle/airframe dynamics → Controls/sensors/environment → Solver and scenario replay → Independent motion/force proof.

Interface design: A Scenario workspace combines an actor tree, 2D road/map editor and 3D viewport. Pose inspectors show reference frame and units; a timeline distinguishes prescribed trajectories, integrated dynamics and sensor samples. Run controls support deterministic replay and frame stepping. Selecting an actor links its state traces and detections. Import previews show supported actions and assets, while fidelity/backend labels accompany sensor and physics settings.

Construction:

1. Define original scene and actor records for road/obstacle geometry, stable actor IDs, poses, units and frame conventions. Separate trajectory playback from force-based dynamics. Start with cuboid/polygon scenes and explicitly name supported interpolations instead of claiming photorealistic or full vehicle-blockset parity.
2. Implement original kinematic vehicle and rigid-body flight kernels through the validated solver service, with mass/inertia, gravity, force/torque conventions and quaternion handling. Evaluate JSBSim as a candidate flight backend; preserve a backend/version declaration and independently verify all selected model parameters.
3. Add scenario-time scheduling for actors and sensor updates, with deterministic noise streams and logged timestamps. Begin with geometric range/ray observations and known detection policies. Camera, radar, lidar and environmental fidelity each need a separate calibrated release and original validation dataset.
4. Create versioned scenario serialization and selected ASAM OpenSCENARIO interchange only after inspecting exact schema/version/actions. Route unsupported road networks, behavior rules and external engines to import diagnostics. Keep proprietary assets and protected dynamics models outside the bundled original examples.
5. Publish straight-road, coordinated-turn and UAV-hover workshops that combine scenes with controller/dynamics traces. Export exact initial conditions, frames, environment, seed and solver profile. Digital-twin claims require measured calibration and field comparison beyond deterministic scenario replay or visually plausible animation.

API surface to qualify: `drivingScenario`, `road`, `vehicle`, `actor`, `trajectory`, `advance`, `uavScenario`, `uavPlatform`, `move`, `6DOF dynamics`.

Boundary: Freeze the named toolbox/product, release, supported API variants and backend/target boundaries in this feature's profile; see its detailed implementation card.

<a id="feature-af43"></a>

### AF43 — Production services and deployable scientific applications

Implementation task: [J43](#task-j43). Group: Systems integration; integration milestone: M8.

How it works: Production Server invokes deployed functions through typed synchronous or asynchronous requests. Outputs and job lifecycle cross a network protocol. Compiler/runtime archives and service APIs have separate product requirements.

Flow: Source/profile package → Typed request validation → Isolated worker job → Typed result/artifact → Versioned deployed service.

Interface design: Proposed Deployment view lists entry points, typed signatures, profile/version, worker status, request progress and errors; a job detail opens its original source/result artifacts. Failed rollout retains the previous version. A web-app view reports its server/model revision.

Construction:

1. Freeze an original MathLab service package: source entry points, allowed inputs/classes/shapes, dependency profile, artifact hashes and requested output arity. Treat vendor CTF/MATLAB Runtime archives as an explicit unsupported/interoperability assessment.
2. Build a Rust HTTP adapter around the session/job actor; use axum as a candidate after version/license review. Allocate isolated worker sessions, validate typed requests and enforce entry-point/resource policy without exposing arbitrary session eval.
3. Implement synchronous and asynchronous request state, typed nonfinite/complex/array results, cancellation, retry/idempotency rules and durable artifact ownership. Preserve source revision and original error frames across workers.
4. Add deployment/profile validation, version routing, health/metrics and rollback using the release contract. Keep credentials and per-user storage separate from source archives and shared workers.
5. Build a service/browser client and deployable example using the same supported app controls where relevant. Test local installed service behavior before testing a declared hosted target.

API surface to qualify: `compiler.build.productionServerArchive`, `webwrite`, `mps.json.encoderequest`, `nargout`.

Boundary: MATLAB Compiler/Compiler SDK, Production Server and Web App Server are distinct commercial workflows. MathLab will implement its own open service/app artifacts and declared protocol adapters; original service behavior is not vendor binary compatibility.

<a id="feature-af44"></a>

### AF44 — Instrument control and streamed data acquisition

Implementation task: [J44](#task-j44). Group: Systems integration; integration milestone: M8.

How it works: Instrument objects manage connections, framing, timeouts and callbacks; acquisition objects add channels, sampling clocks and buffers. Device drivers and support packages determine available hardware and timing behavior.

Flow: Device/interface profile → Framing and channel codec → Buffered I/O worker → Owning-thread callbacks → Live scope and recorded samples.

Interface design: Proposed Device Explorer shows connection state, interface/driver, timeout/framing, channels/units, acquisition rate and trigger. Live Scope shows timestamps, buffer fill and dropped samples; disconnect, timeout and overflow are visible states with recoverable results.

Construction:

1. Define a typed instrument handle with connection generation, timeout, byte order, terminator, units, cancellation and close semantics. Start with TCP loopback and an original simulated instrument rather than an untested device list.
2. Implement native I/O workers and byte/framing codecs; queue callback notifications on the owning engine thread. Reject old connection events after reconnect/restart and close resources on error or handle deletion.
3. Add SCPI command/readback support and serial/VISA adapters as independently tested capabilities. Record each target driver/SDK and source/license/platform constraints.
4. Implement acquisition channel metadata, sample clock, buffer ownership, timestamps, trigger/pretrigger policy and dropped-sample accounting. Device-clock timing is separate from UI refresh or general-purpose worker timing.
5. Add a connection/channel configuration view and bounded live scope. Verify loopback/mocked behavior, then validate the declared hardware subset using actual devices and installed-package checks.

API surface to qualify: `tcpclient`, `serialport`, `visadev`, `configureTerminator`, `configureCallback`, `daq`.

Boundary: Base MATLAB provides the TCP client and serial-port APIs. VISA instrument communication belongs to Instrument Control Toolbox and depends on a compatible VISA driver. Sampled acquisition channels, timing and device drivers are a separate Data Acquisition Toolbox contract. MathLab profiles must enumerate supported transports, framing, drivers and actual device models separately.

<a id="feature-af45"></a>

### AF45 — Transactional database queries and typed result streaming

Implementation task: [J45](#task-j45). Group: Systems integration; integration milestone: M8.

How it works: Database connectors bind typed parameters to prepared queries and return tabular results. Driver capabilities control transactions, schema types and streaming; SQL NULL and MATLAB missing values need an explicit mapping.

Flow: Connection and schema profile → Prepared typed parameters → Transaction/cursor → Bounded typed batches → Table or datastore result.

Interface design: Proposed Data Connections view includes connector profile, schema explorer, parameter fields and bounded query result grid. It displays transaction state, explicit NULL markers, batch progress and conversion failures; committing is a distinct action from fetching.

Construction:

1. Freeze a connector/driver subset and schema mapping for signed/unsigned integers, text/encoding, timezones, decimals, blobs and missing values. Track SQL NULL separately from floating NaN and empty string.
2. Implement connection/statement/result handles with generation and transaction lifetime. Use SQLite native binding as a first local candidate; JDBC/ODBC and remote databases remain separate backend profiles.
3. Bind values through native parameter APIs rather than SQL text interpolation. Specify commit/rollback, cancellation, cursor/batch size and driver-error mapping without logging connection secrets.
4. Stream bounded result batches through the typed data service and integrate tables/datastore plans; record ordering, snapshot/isolation and cross-batch consistency. Closing the cursor releases backend resources.
5. Add schema/query preview and result-grid views with transaction state, rows fetched, pending cancellation and failed conversion. Test both installed local fixture and each advertised external connector.

API surface to qualify: `database`, `databasePreparedStatement`, `bindParamValues`, `fetch`, `sqlwrite`.

Boundary: Database Toolbox connector/JDBC/ODBC options differ by release/driver. MathLab starts with a named native connector; similarly named calls do not establish universal driver or MATLAB object support.

<a id="feature-af46"></a>

### AF46 — Graph and network algorithms with inspectable results

Implementation task: [J46](#task-j46). Group: Systems integration; integration milestone: M8.

How it works: Graph objects retain nodes, edges, attributes and direction. Algorithms operate on declared weights/capacities; shortest paths and flows have different input contracts, and plotted coordinates do not change network connectivity.

Flow: Typed node/edge graph → Algorithm and weight selection → Sparse computation → Path/flow invariants → Linked graph and attribute view.

Interface design: Proposed Network Explorer combines node/edge table, graph canvas and algorithm inspector. Selected start/end nodes, weight/capacity property, path cost/flow and conservation diagnostics remain visible; stale structural results are invalidated after mutation.

Construction:

1. Define graph/digraph object schema with stable node/edge IDs, name/attribute tables, multiedges and self-loop policy. Preserve IDs when an algorithm or layout changes display order.
2. Implement sparse adjacency and mutation operations with versioned structural caches. Validate weight/capacity dimensions and supported directed/undirected variants before executing algorithms.
3. Add named algorithms incrementally: connectivity, unweighted/positive-weight shortest path, declared negative-edge method, max flow and centrality. Use exact small-network oracles and structural conservation checks.
4. Add graph layout/picking tied to original node/edge IDs, path/flow overlays and linked attribute grid. Preserve algorithm result independently from camera/layout so visual arrangement cannot alter numerical outputs.
5. Round-trip supported graphs in the declared file schema and run installed large/sparse resource tests; disclose nonunique path/layout conventions and unsupported variants.

API surface to qualify: `graph`, `digraph`, `shortestpath`, `maxflow`, `conncomp`, `centrality`.

Boundary: Graph/digraph APIs are MATLAB functionality; backend algorithms, attribute storage and plotting each need independent API/type/lifecycle tests.

<a id="feature-af47"></a>

### AF47 — Custom simulation blocks, callback lifecycles and masked libraries

Implementation task: [J47](#task-j47). Group: Advanced ecosystem; integration milestone: M7.

How it works: Custom blocks declare ports, parameters, state and sample-time behavior. The simulation engine invokes registered callbacks at lifecycle phases; masks expose parameters while reusable libraries retain block-definition identity.

Flow: Block source and manifest → Ports/sample-time/state ABI → Compile and lifecycle callbacks → Owned update/output dispatch → Mask/library and state proof.

Interface design: The block inspector exposes callback methods, port shapes, sample time, state and parameter tunability. A lifecycle debugger highlights the current phase and original source; state inspection distinguishes pre-update from post-update values. A mask designer previews labels and validation without running arbitrary initialization. Library instances show definition revision and overrides, and an update preview identifies changed ports before reconnecting or invalidating previous simulation results.

Construction:

1. Define three separate capability variants: a restricted pure MATLAB Function block, selected discrete Level-2 callbacks, and optional MATLAB System-object blocks. Freeze supported classes/shapes, fixed rates, direct-feedthrough flags and tunable parameters; continuous states and C MEX blocks remain separate gated variants.
2. Extend the Rust model compiler with per-instance callback registries and typed runtime-block proxies. Execute supported source through the pinned Octave worker, retaining original file/revision and instance scope. Validate runtime property reads/writes rather than forwarding arbitrary model-object mutation into the scheduler.
3. Implement discrete state allocation, initialization, outputs and updates as distinct phases under the validated schedule. Keep state independent across instances and runs; invalidate old callback epochs after restart. Initial support must reject unsupported minor-step or algebraic-loop callbacks rather than approximate their execution order.
4. Create original masked parameter forms, validation and labels with transactional edits. Store library definition revision, instance overrides and explicit update policy. Passive model preview inventories callbacks without executing initialization code; running callbacks uses the normal source, cancellation and diagnostics contracts.
5. Build original saturation and one-sample-delay workshops with callback logs and source-generated block definitions. Add System-object state/reset integration only with its validated runtime contract; add C/source code generation and platform binary interfaces separately, preserving unsupported blocks and proprietary artifacts in migration reports.

API surface to qualify: `MATLAB Function block`, `MATLAB System block`, `Level-2 MATLAB S-Function`, `Simulink.MSFcnRunTimeBlock`, `RegBlockMethod`, `PostPropagationSetup`, `InitializeConditions`, `Outputs`, `Update`, `Dwork`, `DirectFeedthrough`, `block masks`.

Boundary: Freeze the named toolbox/product, release, supported API variants and backend/target boundaries in this feature's profile; see its detailed implementation card.

<a id="feature-af48"></a>

### AF48 — Architecture components, typed interfaces, allocations and engineering views

Implementation task: [J48](#task-j48). Group: Advanced ecosystem; integration milestone: M8.

How it works: Architecture models describe components, ports, interfaces and relationships independently of executable behavior. Stereotypes add typed metadata; requirement links, allocations and filtered views support analysis and traceability across model revisions.

Flow: Components and interfaces → Dictionary and stereotypes → Connections/allocations/views → Constraint/latency analysis → Architecture traceability report.

Interface design: An Architecture workspace contains a component tree, typed connector canvas, interface dictionary and property table. Users switch between structural, allocation, requirement and filtered views without changing underlying connectivity. An analysis pane states aggregation rules, units and model revision. Selecting a port reveals its full interface and incompatible connection reasons. Requirement/behavior links show stale or missing targets explicitly, and import previews report unsupported architecture constructs before conversion.

Construction:

1. Design an original architecture schema with stable component/port/connector IDs, hierarchy, interface dictionaries and explicit ownership. Keep architecture models distinct from executable simulation graphs, even when they reference the same component; native versioned storage precedes any selected proprietary-file import assessment.
2. Implement a selected Octave-facing object API over the Rust architecture service for create, add, connect, inspect and save. Type-check port direction, element names, dimensions and units. Every transaction updates revisioned references atomically so renames do not detach connectors or silently rebind requirements.
3. Add typed profile/stereotype definitions, defaults, units and constrained values. Implement explicit analysis functions over immutable model snapshots, with declared aggregation rules and source-linked results; a computed cost or latency report is an analysis under assumptions, not proof of executable target timing.
4. Integrate requirement records and functional-to-implementation allocation relationships with the engineering evidence service. Record model/evidence hashes and stale-link status. Behavior links, variants, generated code, physical-domain interfaces and external interchange each require their own prerequisite contracts and supported subset.
5. Build original sensor/controller/actuator architecture and allocation workshops, including component views, interface-control reports and reviewable table import/export. Preserve unsupported stereotypes and foreign metadata as inventory items rather than discarding them or claiming full System Composer model restoration.

API surface to qualify: `systemcomposer.createModel`, `systemcomposer.createDictionary`, `addComponent`, `addPort`, `connect`, `addInterface`, `addElement`, `setInterface`, `createProfile`, `addStereotype`, `applyStereotype`, `requirement links`, `allocation sets`.

Boundary: Freeze the named toolbox/product, release, supported API variants and backend/target boundaries in this feature's profile; see its detailed implementation card.

<a id="feature-af49"></a>

### AF49 — Text tokenization, sparse document features and topic-model workflows

Implementation task: [J49](#task-j49). Group: Advanced ecosystem; integration milestone: M8.

How it works: Tokenized documents preserve text-derived units and metadata; sparse document-term models count vocabulary occurrences. Topic models infer probabilistic document mixtures and word distributions, while preprocessing, language and solver choices affect interpretation.

Flow: Original text and tokenizer → Vocabulary/count matrix → Topic or embedding backend → Document results and state → Count/posterior/Unicode proof.

Interface design: A Text Analysis workspace places original documents beside token/preprocessing changes and a document-term grid. A pipeline inspector shows language, vocabulary, counts and excluded rows. Topic views expose top terms and probability mixtures, with word clouds paired with numeric tables rather than used as correctness evidence. A held-out partition marker and model/profile hash remain visible; reopening a model restores preprocessing and flags missing resources.

Construction:

1. Freeze a selected text profile for Unicode normalization, token boundaries, case, punctuation, language, stopwords and empty documents. Retain original document IDs and offsets so every displayed token or removed term can be traced back to the source text.
2. Implement bounded token streams and sparse document-term storage in Rust, exposing versioned Octave-facing document/model objects. Specify vocabulary order, duplicate/n-gram counting, training-only vocabulary construction and unknown-term behavior; no hidden dense conversion is permitted for large corpora.
3. Add one declared LDA inference profile with explicit solver, word/topic priors, convergence, RNG and topic-order policy. Evaluate Gensim as an optional candidate, translating prior parameterization and dtype precisely; similar topic labels or default settings do not establish MATLAB object/solver compatibility.
4. Implement source-generating preprocessing and model inspection with saved data splits and reversible pipeline metadata. Validate held-out transforms without fitting vocabulary or preprocessing on held-out documents. Supervised text models, embeddings, multilingual tokenizers and transformer/GPU backends expand through separately gated operator/data profiles.
5. Publish original document-count and one-topic workshops with exact numerical fixtures, then add corpus-scale resource and installed rerun checks. Bundle original/licensed teaching text; protect provenance and report unsupported languages or extraction formats instead of silently substituting a different tokenizer.

API surface to qualify: `tokenizedDocument`, `removeStopWords`, `erasePunctuation`, `bagOfWords`, `bagOfNgrams`, `fitlda`, `ldaModel`, `transform`, `predict`, `wordcloud`, `textscatter`.

Boundary: Freeze the named toolbox/product, release, supported API variants and backend/target boundaries in this feature's profile; see its detailed implementation card.

<a id="feature-af50"></a>

### AF50 — Satellite orbits, ground-station access and reproducible link budgets

Implementation task: [J50](#task-j50). Group: Advanced ecosystem; integration milestone: M8.

How it works: Satellite scenarios propagate objects, transform coordinate frames and evaluate visibility over time. Access depends on obstruction and elevation rules; communication links additionally combine propagation loss, antenna gains, receiver noise and rate requirements.

Flow: Orbit and frame conventions → Propagation/time sampling → Access and occultation geometry → Antenna/channel/link analysis → Geometry and link-budget proof.

Interface design: A Satellite workspace combines an object tree, globe/orbit view and access timeline. Inspectors show epoch, frame, units, propagator and central-body assumptions. Selecting a station/satellite pair reveals range, elevation and obstruction separately from link status. A link-budget table shows gains, losses, noise and bit rate with numeric margins. Timeline sampling and refined interval boundaries are labeled; unsupported TLE, terrain or radio models remain visible prerequisites.

Construction:

1. Define selected scenario records for epochs/time scales, orbital elements or Cartesian states, frame identities, central-body shape/gravity and ground stations. Begin with an explicitly named spherical-Earth, two-body profile; TLE/SGP4, perturbations, terrain and non-Earth bodies remain separate validated variants.
2. Implement analytical circular/Kepler propagation and declared inertial/Earth-fixed transforms in Rust with original Octave-facing objects. Evaluate NASA SPICE for later frame/time services; pin kernels and conventions before use. Do not mix inertial state, Earth-fixed station coordinates or UTC samples implicitly.
3. Compute line-of-sight occultation, local elevation masks and access intervals using declared sample grids or refined boundary roots. Specify interval closure and endpoint policy, retain object IDs, and distinguish geometric visibility from radio-link closure or a successfully rendered ground track.
4. Add selected far-field free-space link budgets with explicit dBW/dBm, dBi, hertz, bits/second, noise temperature and losses. Expose every term and receiver threshold; waveform, coding, atmospheric fading, antenna electromagnetic design and standard compliance require separate domain profiles.
5. Publish original overhead/occulted geometry, circular-orbit and loss-budget workshops with independent formulas. Export scenario inputs, time/frame profile and reports. Optional platform dynamics, terrain layers, numerical propagation and standards-based radios must close their own AF and external conformance gates before advertising those variants.

API surface to qualify: `satelliteScenario`, `satellite`, `groundStation`, `access`, `accessIntervals`, `transmitter`, `receiver`, `link`, `ebno`, `fspl`, `satelliteScenarioViewer`.

Boundary: Freeze the named toolbox/product, release, supported API variants and backend/target boundaries in this feature's profile; see its detailed implementation card.

<a id="learning-sources"></a>

## Primary learning references

Source IDs have their own namespace: source D01 is distinct from engineering task D01. Recorded page-check dates describe earlier source selection; this normalization does not recheck every external URL. Linked upstream illustrations remain on their original sites.

<a id="source-d01"></a>

### D01 — Desktop Basics

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/matlab/learn_matlab/desktop.html); [Related reference 1](https://www.mathworks.com/help/matlab/learn_matlab/desktop.png).

<a id="source-d02"></a>

### D02 — Configure the Desktop

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/matlab/matlab_env/change-the-desktop-layout.html); [Related reference 1](https://www.mathworks.com/help/matlab/matlab_env/desktop_files_panel_selected.png); [Related reference 2](https://www.mathworks.com/help/matlab/matlab_env/desktop_leftright_tile.png).

<a id="source-d03"></a>

### D03 — Variables Editor

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/matlab/ref/variableseditor.html); [Related reference 1](https://www.mathworks.com/help/matlab/ref/variables_editor_replaceelem.png); [Related reference 2](https://www.mathworks.com/help/matlab/ref/variables_editor_statssparklines.png).

<a id="source-d04"></a>

### D04 — Debug MATLAB Code Files

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/matlab/matlab_prog/debugging-process-and-features.html); [Related reference 1](https://www.mathworks.com/help/matlab/matlab_prog/debugging_runtohere_paused.png).

<a id="source-d05"></a>

### D05 — Property Inspector

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/matlab/ref/propertyinspector.html); [Related reference 1](https://www.mathworks.com/help/matlab/ref/inspector_expandhierarchy.png).

<a id="source-d06"></a>

### D06 — Interactively Explore and Analyze Plotted Data

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/matlab/creating_plots/interactively-explore-plotted-data.html); [Related reference 1](https://www.mathworks.com/help/matlab/creating_plots/figtoolstrip_datatipsadded.png); [Related reference 2](https://www.mathworks.com/help/matlab/creating_plots/figtoolstrip_brushed.png); [Related reference 3](https://www.mathworks.com/help/matlab/creating_plots/figtoolstrip_stats.png).

<a id="source-d07"></a>

### D07 — Create Callbacks for Graphics Objects

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/matlab/creating_plots/create-callbacks-for-graphics-objects.html).

<a id="source-d08"></a>

### D08 — Create Live Scripts

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/matlab/matlab_prog/create-live-scripts.html); [Related reference 1](https://www.mathworks.com/help/matlab/matlab_prog/live_editor_output_right.png).

<a id="source-d09"></a>

### D09 — Create and Run a Simple App Using App Designer

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/matlab/creating_guis/create-a-simple-app-or-gui-using-app-designer.html); [Related reference 1](https://www.mathworks.com/help/matlab/creating_guis/appd-tutorial-canvas.png).

<a id="source-d10"></a>

### D10 — Lay Out Apps in App Designer Design View

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/matlab/creating_guis/lay-out-apps-in-app-designer.html); [Related reference 1](https://www.mathworks.com/help/matlab/creating_guis/appdlayout_quick_insert_menu.png); [Related reference 2](https://www.mathworks.com/help/matlab/creating_guis/appdlayout_alignment_hints.png).

<a id="source-d11"></a>

### D11 — Manage Code in App Designer Code View

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/matlab/creating_guis/app-designer-code-generation.html); [Related reference 1](https://www.mathworks.com/help/matlab/creating_guis/codeview_editable_sections19a.png); [Related reference 2](https://www.mathworks.com/help/matlab/creating_guis/code_browser_appearance.png).

<a id="source-d12"></a>

### D12 — Create Projects

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/matlab/matlab_prog/create-projects.html); [Related reference 1](https://www.mathworks.com/help/matlab/matlab_prog/projects_newproject.png).

<a id="source-d13"></a>

### D13 — Package Apps in App Designer

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/matlab/creating_guis/package-apps-in-app-designer.html); [Related reference 1](https://www.mathworks.com/help/matlab/creating_guis/app-sharing-dialog-box.png).

<a id="source-d14"></a>

### D14 — egui Context 0.33.3

Previously checked: 2026-10-14. [Primary reference](https://docs.rs/egui/0.33.3/egui/struct.Context.html).

<a id="source-d15"></a>

### D15 — eframe App 0.33.3

Previously checked: 2026-10-14. [Primary reference](https://docs.rs/eframe/0.33.3/eframe/trait.App.html).

<a id="source-d16"></a>

### D16 — GNU Octave Handle Functions

Previously checked: 2026-10-14. [Primary reference](https://docs.octave.org/interpreter/Handle-Functions.html).

<a id="source-d17"></a>

### D17 — GNU Octave Breakpoints

Previously checked: 2026-10-14. [Primary reference](https://docs.octave.org/interpreter/Breakpoints.html).

<a id="source-d18"></a>

### D18 — GNU Octave Callbacks

Previously checked: 2026-10-14. [Primary reference](https://docs.octave.org/interpreter/Callbacks.html).

<a id="source-s01"></a>

### S01 — Data types and their frontend representation

Previously checked: 2026-10-14. [Primary reference](https://docs.octave.org/latest/Data-Types.html); [Related reference 1](https://www.mathworks.com/help/matlab/data-types.html).

<a id="source-s02"></a>

### S02 — Runnable numerical tests and result inspection

Previously checked: 2026-10-14. [Primary reference](https://docs.octave.org/latest/Test-Functions.html); [Related reference 1](https://www.mathworks.com/help/matlab/matlab_prog/function-based-unit-tests.html).

<a id="source-s03"></a>

### S03 — Import preview that produces reusable code

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/matlab/ref/importtool.html).

<a id="source-s04"></a>

### S04 — Heterogeneous tables with named variables

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/matlab/matlab_prog/create-a-table.html).

<a id="source-s05"></a>

### S05 — Synchronize and resample timestamped measurements

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/matlab/ref/timetable.synchronize.html).

<a id="source-s06"></a>

### S06 — Figures as inspectable graphics objects

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/matlab/creating_plots/using-high-level-plotting-functions.html).

<a id="source-s07"></a>

### S07 — Design, inspect and apply digital filters

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/signal/ug/practical-introduction-to-digital-filter-design.html); [Related reference 1](https://octave.sourceforge.io/signal/function/fir1.html).

<a id="source-s08"></a>

### S08 — Peak detection with adjustable constraints

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/signal/ug/peak-analysis.html).

<a id="source-s09"></a>

### S09 — Control-system response objects and linked plots

Previously checked: 2026-10-14. [Primary reference](https://python-control.readthedocs.io/en/latest/examples/python-control_tutorial.html).

<a id="source-s10"></a>

### S10 — Statistical summaries and hypothesis-test conventions

Previously checked: 2026-10-14. [Primary reference](https://docs.scipy.org/doc/scipy/tutorial/stats/analysing_one_sample.html).

<a id="source-s11"></a>

### S11 — Optimization with convergence and failure diagnostics

Previously checked: 2026-10-14. [Primary reference](https://docs.scipy.org/doc/scipy/tutorial/optimize.html).

<a id="source-s12"></a>

### S12 — Image-processing pipelines with intermediate results

Previously checked: 2026-10-14. [Primary reference](https://scikit-image.org/docs/stable/auto_examples/applications/plot_coins_segmentation.html).

<a id="source-s13"></a>

### S13 — Exact symbolic results alongside numeric evaluation

Previously checked: 2026-10-14. [Primary reference](https://docs.sympy.org/latest/tutorials/intro-tutorial/calculus.html).

<a id="source-s14"></a>

### S14 — Explicit parallel jobs with reproducible inputs

Previously checked: 2026-10-14. [Primary reference](https://octave.sourceforge.io/parallel/4.0.2/package_doc/parcellfun.html).

<a id="source-s15"></a>

### S15 — GPU array residency and capability diagnostics

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/parallel-computing/run-matlab-functions-on-a-gpu.html).

<a id="source-s16"></a>

### S16 — Block creation through executable results

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/simulink/gs/create-a-simple-model.html).

<a id="source-s17"></a>

### S17 — Open-source block simulator workflow

Previously checked: 2026-10-14. [Primary reference](https://www.scilab.org/xcos-for-beginners-tutorial); [Related reference 1](https://www.scilab.org/sites/default/files/Xcos_beginners_0.pdf).

<a id="source-s18"></a>

### S18 — Observable hierarchical statecharts

Previously checked: 2026-10-14. [Primary reference](https://sismic.readthedocs.io/en/latest/integration.html).

<a id="source-s19"></a>

### S19 — Whole-ecosystem inventory, not a runnable tutorial

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/products/alphabetical.html).

<a id="source-s20"></a>

### S20 — Open-source package inventory, not runnable coverage evidence

Previously checked: 2026-10-14. [Primary reference](https://packages.octave.org/); [Related reference 1](https://pr0m1th3as.github.io/datatypes/).

<a id="source-s21"></a>

### S21 — Core dense and sparse numerical algebra

Previously checked: 2026-10-14. [Primary reference](https://docs.octave.org/latest/Linear-Algebra.html); [Related reference 1](https://docs.octave.org/latest/Matrix-Factorizations.html).

<a id="source-s22"></a>

### S22 — Integration interpolation and differential equations

Previously checked: 2026-10-14. [Primary reference](https://docs.octave.org/latest/Functions-of-One-Variable.html); [Related reference 1](https://docs.octave.org/latest/One_002ddimensional-Interpolation.html); [Related reference 2](https://docs.octave.org/latest/Matlab_002dcompatible-solvers.html).

<a id="source-s23"></a>

### S23 — Measured function profiling

Previously checked: 2026-10-14. [Primary reference](https://docs.octave.org/latest/Profiling.html); [Related reference 1](https://docs.octave.org/latest/Profiler-Example.html).

<a id="source-er01"></a>

### ER01 — Language services

Previously checked: 2026-10-14. [Primary reference](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/); [Related reference 1](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.17/specification.md).

<a id="source-er02"></a>

### ER02 — Rust and native boundaries

Previously checked: 2026-10-14. [Primary reference](https://doc.rust-lang.org/nomicon/ffi.html); [Related reference 1](https://www.mathworks.com/help/matlab/matlab-data-array.html); [Related reference 2](https://www.mathworks.com/help/matlab/apiref/matlab.engine.matlabengine_cpp.html); [Related reference 3](https://www.mathworks.com/help/matlab/ref/which.html); [Related reference 4](https://www.mathworks.com/help/matlab/ref/type.html).

<a id="source-er03"></a>

### ER03 — Future parser and execution backend

Previously checked: 2026-10-14. [Primary reference](https://llvm.org/docs/tutorial/MyFirstLanguageFrontend/); [Related reference 1](https://blogs.mathworks.com/loren/2016/02/12/run-code-faster-with-the-new-matlab-execution-engine/); [Related reference 2](https://www.mathworks.com/help/matlab/matlab_prog/avoid-unnecessary-copies-of-data.html); [Related reference 3](https://www.mathworks.com/help/simulink/sfg/how-the-simulink-engine-interacts-with-c-s-functions.html); [Related reference 4](https://www.mathworks.com/help/simulink/ug/choose-a-solver.html); [Related reference 5](https://www.mathworks.com/help/install/ug/download-without-installing.html); [Related reference 6](https://www.mathworks.com/help/install/ug/install-documentation-using-internet-connection.html).

<a id="source-rp01"></a>

### RP01 — Build release binaries with locked dependencies

Previously checked: 2026-10-14. [Primary reference](https://doc.rust-lang.org/cargo/commands/cargo-build.html).

<a id="source-rp02"></a>

### RP02 — Assemble a rebuildable source distribution

Previously checked: 2026-10-14. [Primary reference](https://doc.rust-lang.org/cargo/commands/cargo-vendor.html).

<a id="source-rp03"></a>

### RP03 — Build and retain a platform/profile CI matrix

Previously checked: 2026-10-14. [Primary reference](https://docs.github.com/en/actions/tutorials/build-and-test-code/rust); [Related reference 1](https://docs.github.com/en/actions/how-tos/write-workflows/choose-what-workflows-do/run-job-variations); [Related reference 2](https://docs.github.com/en/actions/tutorials/store-and-share-data).

<a id="source-rp04"></a>

### RP04 — Sign and notarize the macOS distribution

Previously checked: 2026-10-14. [Primary reference](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution); [Related reference 1](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow).

<a id="source-rp05"></a>

### RP05 — Create a Windows MSIX package

Previously checked: 2026-10-14. [Primary reference](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-manual-conversion); [Related reference 1](https://learn.microsoft.com/en-us/windows/msix/package/create-app-package-with-makeappx-tool).

<a id="source-rp06"></a>

### RP06 — Sign and validate Windows installation

Previously checked: 2026-10-14. [Primary reference](https://learn.microsoft.com/en-us/windows/msix/package/sign-msix-package-guide); [Related reference 1](https://learn.microsoft.com/en-us/windows/msix/package/sign-app-package-using-signtool).

<a id="source-rp07"></a>

### RP07 — Build, export, and test a Linux Flatpak

Previously checked: 2026-10-14. [Primary reference](https://docs.flatpak.org/en/latest/first-build.html).

<a id="source-rp08"></a>

### RP08 — Pin and verify the Octave runtime/package profile

Previously checked: 2026-10-14. [Primary reference](https://octave.org/download); [Related reference 1](https://docs.octave.org/latest/Installing-and-Removing-Packages.html).

<a id="source-at01"></a>

### AT01 — Comparison of Handle and Value Classes

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/matlab/matlab_oop/comparing-handle-and-value-classes.html); [Related reference 1](https://www.mathworks.com/help/matlab/matlab_oop/property-attributes.html); [Related reference 2](https://www.mathworks.com/help/matlab/handle-classes.html); [Related reference 3](https://docs.octave.org/latest/classdef-Classes.html).

<a id="source-at02"></a>

### AT02 — Validate Name-Value Arguments

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/matlab/matlab_prog/validate-name-value-arguments.html); [Related reference 1](https://www.mathworks.com/help/matlab/ref/arguments.html).

<a id="source-at03"></a>

### AT03 — onCleanup examples and lifetime rules

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/matlab/ref/oncleanup.html); [Related reference 1](https://www.mathworks.com/help/matlab/ref/timer.html); [Related reference 2](https://www.mathworks.com/help/matlab/ref/handle-class.html).

<a id="source-at04"></a>

### AT04 — Customize Object Indexing

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/matlab/customize-object-indexing.html); [Related reference 1](https://www.mathworks.com/help/matlab/ref/metaclass.html).

<a id="source-at05"></a>

### AT05 — Resample and Aggregate Data in Timetable

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/matlab/matlab_prog/resample-and-aggregate-data-in-timetable.html); [Related reference 1](https://www.mathworks.com/help/matlab/matlab_prog/specify-time-zones.html); [Related reference 2](https://www.mathworks.com/help/matlab/ref/timetable.synchronize.html); [Related reference 3](https://www.mathworks.com/help/matlab/ref/table.innerjoin.html); [Related reference 4](https://www.mathworks.com/help/matlab/ref/double.groupsummary.html).

<a id="source-at06"></a>

### AT06 — Tall Arrays for Out-of-Memory Data

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/matlab/import_export/tall-arrays.html); [Related reference 1](https://www.mathworks.com/help/matlab/datastore.html); [Related reference 2](https://www.mathworks.com/help/matlab/mapreduce.html).

<a id="source-at07"></a>

### AT07 — MatFile partial access examples and limitations

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/matlab/ref/matlab.io.matfile.html); [Related reference 1](https://www.mathworks.com/help/matlab/import_export/mat-file-versions.html); [Related reference 2](https://docs.h5py.org/en/stable/high/dataset.html); [Related reference 3](https://docs.octave.org/latest/Simple-File-I_002fO.html).

<a id="source-at08"></a>

### AT08 — Write C Functions Callable from MATLAB

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/matlab/call-mex-files-1.html); [Related reference 1](https://www.mathworks.com/help/matlab/matlab_external/cpp-mex-api.html); [Related reference 2](https://docs.octave.org/latest/Mex_002dFiles.html); [Related reference 3](https://docs.octave.org/latest/Standalone-Programs.html).

<a id="source-at09"></a>

### AT09 — pyenv environment setup and out-of-process examples

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/matlab/ref/pyenv.html); [Related reference 1](https://www.mathworks.com/help/matlab/ref/javamethod.html); [Related reference 2](https://www.mathworks.com/help/matlab/matlab_external/building-a-net-application-for-matlab-examples.html); [Related reference 3](https://docs.octave.org/latest/Java-Interface.html).

<a id="source-at10"></a>

### AT10 — Create Advanced Parameterized Tests

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/matlab/matlab_prog/create-advanced-parameterized-tests.html); [Related reference 1](https://www.mathworks.com/help/matlab/ref/matlab.mock.testcase-class.html); [Related reference 2](https://www.mathworks.com/help/matlab/ref/matlab.unittest.plugins.codecoverageplugin-class.html); [Related reference 3](https://www.mathworks.com/help/matlab/ref/profile.html).

<a id="source-at11"></a>

### AT11 — Create and Run Tasks Using Build Tool

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/matlab/matlab_prog/create-and-run-tasks-using-build-tool.html); [Related reference 1](https://www.mathworks.com/help/matlab/matlab_prog/overview-of-matlab-build-tool.html); [Related reference 2](https://www.mathworks.com/help/matlab/ref/matlab.addons.toolbox.packagetoolbox.html); [Related reference 3](https://www.mathworks.com/help/matlab/ref/matlab.project.project.html).

<a id="source-at12"></a>

### AT12 — Use Assumptions on Symbolic Variables

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/symbolic/assumptions-for-symbolic-objects.html); [Related reference 1](https://www.mathworks.com/help/symbolic/sym.vpa.html); [Related reference 2](https://www.mathworks.com/help/symbolic/sym.matlabfunction.html); [Related reference 3](https://docs.sympy.org/latest/guides/assumptions.html); [Related reference 4](https://docs.sympy.org/latest/modules/utilities/codegen.html).

<a id="source-at13"></a>

### AT13 — isosurface extraction and plotting examples

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/matlab/ref/isosurface.html); [Related reference 1](https://www.mathworks.com/help/matlab/volume-visualization.html).

<a id="source-at14"></a>

### AT14 — Develop Custom UI Components Programmatically

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/matlab/creating_guis/develop-classes-of-ui-component-objects.html); [Related reference 1](https://www.mathworks.com/help/matlab/develop-live-editor-tasks.html); [Related reference 2](https://www.mathworks.com/help/matlab/creating_guis/create-a-simple-app-or-gui-using-app-designer.html); [Related reference 3](https://www.mathworks.com/help/matlab/creating_guis/app-designer-code-generation.html); [Related reference 4](https://www.mathworks.com/help/matlab/ref/matlab.ui.componentcontainer.componentcontainer-class.html).

<a id="source-at15"></a>

### AT15 — decomposition: worked examples for repeated linear solves

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/matlab/ref/decomposition.html); [Related reference 1](https://www.mathworks.com/help/matlab/ref/eig.html); [Related reference 2](https://docs.octave.org/latest/Iterative-Techniques.html).

<a id="source-at16"></a>

### AT16 — Tutorial for Optimization Toolbox

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/optim/ug/optimization-toolbox-tutorial.html); [Related reference 1](https://www.mathworks.com/help/optim/ug/intlinprog.html); [Related reference 2](https://www.mathworks.com/help/optim/ug/nonlinear-constraints.html); [Related reference 3](https://ergo-code.github.io/HiGHS/dev/); [Related reference 4](https://nlopt.readthedocs.io/en/stable/NLopt_Tutorial/).

<a id="source-at17"></a>

### AT17 — fitcensemble: training, generalization, and optimization examples

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/stats/fitcensemble.html); [Related reference 1](https://www.mathworks.com/help/stats/fitglm.html); [Related reference 2](https://www.mathworks.com/help/stats/bayesopt.html); [Related reference 3](https://www.mathworks.com/help/stats/mhsample.html); [Related reference 4](https://www.statsmodels.org/stable/glm.html).

<a id="source-at18"></a>

### AT18 — Introduction to Streaming Signal Processing in MATLAB

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/dsp/ug/introduction-to-streaming-signal-processing-in-matlab.html); [Related reference 1](https://www.mathworks.com/help/wavelet/ug/cwt-based-time-frequency-analysis.html); [Related reference 2](https://pywavelets.readthedocs.io/en/latest/ref/nd-dwt-and-idwt.html).

<a id="source-at19"></a>

### AT19 — Model Predictive Control of Multi-Input Single-Output Plant

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/mpc/gs/control-of-a-multi-input-single-output-plant.html); [Related reference 1](https://www.mathworks.com/help/ident/ref/arx.html); [Related reference 2](https://www.mathworks.com/help/robust/ref/dynamicsystem.mixsyn.html); [Related reference 3](https://python-control.readthedocs.io/en/latest/).

<a id="source-at20"></a>

### AT20 — Distribute Arrays and Run SPMD

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/parallel-computing/distribute-arrays-and-run-spmd.html); [Related reference 1](https://www.mathworks.com/help/parallel-computing/sliced-variable.html); [Related reference 2](https://docs.open-mpi.org/en/main/); [Related reference 3](https://docs.rs/rayon/latest/rayon/).

<a id="source-at21"></a>

### AT21 — parfeval: run functions in the background and retrieve futures

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/matlab/ref/parfeval.html); [Related reference 1](https://www.mathworks.com/help/parallel-computing/parallel.pool.dataqueue.html); [Related reference 2](https://www.mathworks.com/help/parallel-computing/run-a-batch-job.html); [Related reference 3](https://www.mathworks.com/help/matlab-parallel-server/index.html); [Related reference 4](https://docs.rs/tokio/latest/tokio/).

<a id="source-at22"></a>

### AT22 — Run CUDA or PTX Code on GPU

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/parallel-computing/run-cuda-or-ptx-code-on-gpu.html); [Related reference 1](https://www.mathworks.com/help/parallel-computing/gpuarray.html); [Related reference 2](https://www.mathworks.com/help/gpucoder/gs/gpu-code-generation-using-cli.html); [Related reference 3](https://arrayfire.org/docs/index.htm); [Related reference 4](https://docs.nvidia.com/cuda/cublas/).

<a id="source-at23"></a>

### AT23 — Train Network Using Custom Training Loop

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/deeplearning/ug/train-network-using-custom-training-loop.html); [Related reference 1](https://www.mathworks.com/help/deeplearning/ref/dlarray.html); [Related reference 2](https://www.mathworks.com/help/deeplearning/ref/importnetworkfromonnx.html); [Related reference 3](https://www.mathworks.com/help/deeplearning/ref/exportonnxnetwork.html); [Related reference 4](https://docs.pytorch.org/tutorials/beginner/basics/autogradqs_tutorial.html); [Related reference 5](https://onnxruntime.ai/docs/get-started/).

<a id="source-at24"></a>

### AT24 — Create Custom Environment Using Step and Reset Functions

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/reinforcement-learning/ug/create-matlab-environments-using-custom-functions.html); [Related reference 1](https://www.mathworks.com/help/reinforcement-learning/ref/rl.env.rlfunctionenv.html); [Related reference 2](https://gymnasium.farama.org/introduction/create_custom_env/); [Related reference 3](https://stable-baselines3.readthedocs.io/en/master/guide/custom_env.html).

<a id="source-at25"></a>

### AT25 — Heat Transfer Problem with Temperature-Dependent Properties

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/pde/ug/heat-transfer-problem-with-temperature-dependent-properties.html); [Related reference 1](https://www.mathworks.com/help/pde/ug/femodel.html); [Related reference 2](https://mfem.org/examples/).

<a id="source-at26"></a>

### AT26 — Label 3-D Medical Image Using Medical Image Labeler

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/medical-imaging/ug/Label-3-d-medical-image-using-medical-image-labeler.html); [Related reference 1](https://www.mathworks.com/help/images/ref/imregtform.html); [Related reference 2](https://www.mathworks.com/help/vision/ug/camera-calibration.html); [Related reference 3](https://www.mathworks.com/help/fusion/ref/trackergnn-system-object.html); [Related reference 4](https://docs.opencv.org/4.x/d9/d0c/group__calib3d.html); [Related reference 5](https://simpleitk.readthedocs.io/en/master/registrationOverview.html).

<a id="source-at27"></a>

### AT27 — Build Basic Rigid Body Tree Models

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/robotics/ug/build-basic-rigid-body-tree-models.html); [Related reference 1](https://ch.mathworks.com/help/ros/ug/get-started-with-ros-2.html); [Related reference 2](https://www.mathworks.com/help/nav/ref/insfiltererrorstate.html); [Related reference 3](https://github.com/stack-of-tasks/pinocchio); [Related reference 4](https://ompl.kavrakilab.org/); [Related reference 5](https://github.com/ros2/ros2).

<a id="source-at28"></a>

### AT28 — 5G NR Downlink Vector Waveform Generation

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/5g/ug/downlink-carrier-waveform-generation.html); [Related reference 1](https://www.mathworks.com/help/comm/ref/qammod.html); [Related reference 2](https://www.mathworks.com/help/phased/ref/phased.steeringvector-system-object.htm); [Related reference 3](https://docs.srsran.com/projects/project/en/latest/); [Related reference 4](https://scikit-rf.readthedocs.io/en/latest/tutorials/Networks.html); [Related reference 5](https://www.etsi.org/deliver/etsi_TS/138200_138299/138211/18.08.00_60/ts_138211v180800p.pdf).

<a id="source-at29"></a>

### AT29 — Model Reference Variants

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/simulink/var/model-reference-variants.html); [Related reference 1](https://www.mathworks.com/help/simulink/slref/ratetransition.html); [Related reference 2](https://www.mathworks.com/help/simulink/ug/create-bus-objects.html); [Related reference 3](https://www.mathworks.com/help/simulink/slref/simulink.variantcontrol.html).

<a id="source-at30"></a>

### AT30 — Solve Differential Algebraic Equations (DAEs)

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/matlab/math/solve-differential-algebraic-equations-daes.html); [Related reference 1](https://www.mathworks.com/help/matlab/ref/ode15i.html); [Related reference 2](https://docs.octave.org/latest/Matlab_002dcompatible-solvers.html); [Related reference 3](https://sundials.readthedocs.io/en/latest/ida/Introduction_link.html).

<a id="source-at31"></a>

### AT31 — Implement Parallel States

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/stateflow/ug/utilize-parallel-state-execution.html); [Related reference 1](https://www.mathworks.com/help/stateflow/ug/what-do-semantics-mean-for-stateflow-charts.html); [Related reference 2](https://www.mathworks.com/help/stateflow/ug/execution-order-for-parallel-states.html).

<a id="source-at32"></a>

### AT32 — How Simscape Models Represent Physical Systems

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/simscape/ug/how-simscape-models-represent-physical-systems.html); [Related reference 1](https://www.mathworks.com/help/simscape/lang/defining-component-equations.html); [Related reference 2](https://www.mathworks.com/help/simscape/lang/initial-equations.html); [Related reference 3](https://openmodelica.org/doc/OpenModelicaUsersGuide/latest/fmitlm.html).

<a id="source-at33"></a>

### AT33 — Create a Discrete-Event Model

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/simevents/gs/create-a-discrete-event-model.html); [Related reference 1](https://www.mathworks.com/help/simevents/gs/role-of-queues-in-simevents-models.html); [Related reference 2](https://www.mathworks.com/help/simevents/ug/statistics-for-data-analysis.html).

<a id="source-at34"></a>

### AT34 — FMI 3.0.2 specification: interface concepts, state machines and code examples

Previously checked: 2026-10-14. [Primary reference](https://fmi-standard.org/docs/3.0.2/); [Related reference 1](https://fmi-standard.org/); [Related reference 2](https://openmodelica.org/doc/OpenModelicaUsersGuide/latest/fmitlm.html).

<a id="source-at35"></a>

### AT35 — Generate C/C++ Code

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/coder/ug/generate-cc-code.html); [Related reference 1](https://www.mathworks.com/help/coder/ug/build-setting-configuration.html); [Related reference 2](https://www.mathworks.com/help/ecoder/ug/code-verification-through-software-in-the-loop-and-processor-in-the-loop-execution.html); [Related reference 3](https://www.mathworks.com/help/ecoder/ref/createblock.html).

<a id="source-at36"></a>

### AT36 — Fixed-Point Type Conversion and Derived Ranges

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/hdlcoder/ug/fixed-point-type-conversion-and-derived-ranges.html); [Related reference 1](https://www.mathworks.com/help/fixedpoint/ref/embedded.fi.html); [Related reference 2](https://www.mathworks.com/help/fixedpoint/ref/embedded.fimath.html); [Related reference 3](https://www.mathworks.com/help/hdlcoder/ref/makehdl.html); [Related reference 4](https://www.mathworks.com/help/plccoder/code-generation-for-plcs.html); [Related reference 5](https://verilator.org/guide/latest/); [Related reference 6](https://yosyshq.readthedocs.io/projects/yosys/en/latest/).

<a id="source-at37"></a>

### AT37 — Start a loaded real-time application on target hardware

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/slrealtime/api/slrealtime.target.start.html); [Related reference 1](https://www.mathworks.com/help/slrealtime/getting-started-with-slrealtime.html); [Related reference 2](https://docs.kernel.org/scheduler/sched-deadline.html).

<a id="source-at38"></a>

### AT38 — Verify and Validate a Model and Code

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/sldv/ug/verify-and-validate-a-model-and-code.html); [Related reference 1](https://www.mathworks.com/help/sldv/index.html); [Related reference 2](https://www.mathworks.com/help/sldv/ref/sldvrun.html); [Related reference 3](https://www.mathworks.com/help/overview/code-verification.html); [Related reference 4](https://www.mathworks.com/help/qualkitdo/ug/products-supported-by-do-qualification-kit.html); [Related reference 5](https://www.cprover.org/cbmc/); [Related reference 6](https://www.mathworks.com/help/matlab/matlab_prog/integers.html).

<a id="source-at39"></a>

### AT39 — Backtest Investment Strategies Using Financial Toolbox

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/finance/backtest-investment-strategies.html); [Related reference 1](https://www.mathworks.com/help/econ/arima.html); [Related reference 2](https://www.mathworks.com/help/econ/garch.html); [Related reference 3](https://www.mathworks.com/help/finance/blsprice.html); [Related reference 4](https://www.quantlib.org/docs.shtml).

<a id="source-at40"></a>

### AT40 — Fit a one-compartment biological model using sbiofit

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/simbio/ref/sbiofit.html); [Related reference 1](https://www.mathworks.com/help/bioinfo/ref/bioread.html); [Related reference 2](https://www.mathworks.com/help/simbio/gs/simbiology-command-line-tutorial.html); [Related reference 3](https://www.mathworks.com/help/simbio/ref/sbmlimport.html); [Related reference 4](https://sbml.org/documents/specifications/); [Related reference 5](https://www.htslib.org/doc/samtools.html).

<a id="source-at41"></a>

### AT41 — Read geospatial raster data with metadata-aware references

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/map/ref/readgeoraster.html); [Related reference 1](https://www.mathworks.com/help/matlab/ref/ncread.html); [Related reference 2](https://www.mathworks.com/help/images/ref/dicomread.html); [Related reference 3](https://www.mathworks.com/help/images/read-metadata-from-dicom-files.html); [Related reference 4](https://www.mathworks.com/help/matlab/ref/geoscatter.html); [Related reference 5](https://gdal.org/en/stable/tutorials/index.html); [Related reference 6](https://docs.unidata.ucar.edu/netcdf-c/current/tutorial_8dox.html); [Related reference 7](https://dicom.nema.org/medical/dicom/current/output/html/part05.html); [Related reference 8](https://dicom.nema.org/medical/dicom/current/output/chtml/part03/sect_C.7.6.2.html).

<a id="source-at42"></a>

### AT42 — UAV Scenario Tutorial

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/uav/ug/uav-scenario-tutorial.html); [Related reference 1](https://www.mathworks.com/help/driving/ug/create-actor-and-vehicle-trajectories.html); [Related reference 2](https://www.mathworks.com/help/driving/ref/drivingscenario.html); [Related reference 3](https://www.mathworks.com/help/aeroblks/model-package-delivery-with-custom-variable-mass-6dof-block.html); [Related reference 4](https://www.asam.net/standards/detail/openscenario/); [Related reference 5](https://jsbsim-team.github.io/jsbsim-reference-manual/).

<a id="source-at43"></a>

### AT43 — RESTful API for MATLAB Function Execution

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/mps/restfuljson/restful-api.html); [Related reference 1](https://www.mathworks.com/help/mps/restfuljson/restful-api-for-deployable-archive-upload.html); [Related reference 2](https://docs.rs/axum/latest/axum/).

<a id="source-at44"></a>

### AT44 — Write and Read Data with Instruments

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/instrument/write-and-read-data.html); [Related reference 1](https://www.mathworks.com/help/instrument/tcpclient.configureterminator.html); [Related reference 2](https://www.mathworks.com/help/daq/analog-data-acquisition.html); [Related reference 3](https://www.mathworks.com/help/matlab/ref/tcpclient.html); [Related reference 4](https://www.mathworks.com/help/matlab/ref/serialport.html); [Related reference 5](https://www.mathworks.com/help/instrument/visadev.html).

<a id="source-at45"></a>

### AT45 — Import Data Using an SQL Prepared Statement with Multiple Parameter Values

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/database/ug/import-data-using-sql-prepared-statement-with-multiple-parameter-values.html); [Related reference 1](https://sqlite.org/c3ref/bind_blob.html).

<a id="source-at46"></a>

### AT46 — Shortest Path Between Two Nodes

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/matlab/ref/graph.shortestpath.html); [Related reference 1](https://www.mathworks.com/help/matlab/ref/graph.maxflow.html); [Related reference 2](https://www.mathworks.com/help/matlab/ref/digraph.html).

<a id="source-at47"></a>

### AT47 — Design and Create a Custom Block

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/simulink/ug/tutorial-creating-a-custom-block.html); [Related reference 1](https://www.mathworks.com/help/simulink/sfg/writing-level-2-matlab-s-functions.html); [Related reference 2](https://www.mathworks.com/help/simulink/ug/what-is-matlab-system-block.html); [Related reference 3](https://www.mathworks.com/help/simulink/ug/create-your-own-simulink-block.html).

<a id="source-at48"></a>

### AT48 — Create Architecture Model with Interfaces and Requirement Links

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/systemcomposer/gs/compose-an-architecture-model.html); [Related reference 1](https://www.mathworks.com/help/systemcomposer/ug/build-architecture-command-line.html); [Related reference 2](https://www.mathworks.com/help/systemcomposer/ref/systemcomposer.arch.architecture.addport.html); [Related reference 3](https://www.mathworks.com/help/systemcomposer/ref/systemcomposer.arch.architecture.connect.html); [Related reference 4](https://www.mathworks.com/help/systemcomposer/ug/import-and-export-architectures.html).

<a id="source-at49"></a>

### AT49 — Get Started with Topic Modeling

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/textanalytics/gs/getting-started-with-topic-modeling.html); [Related reference 1](https://www.mathworks.com/help/textanalytics/ref/fitlda.html); [Related reference 2](https://www.mathworks.com/help/textanalytics/ref/ldamodel.html); [Related reference 3](https://radimrehurek.com/gensim/models/ldamodel.html); [Related reference 4](https://www.mathworks.com/help/textanalytics/text-data-modeling.html).

<a id="source-at50"></a>

### AT50 — Satellite Scenario Basics

Previously checked: 2026-10-14. [Primary reference](https://www.mathworks.com/help/satcom/gs/satellite-scenario-overview.html); [Related reference 1](https://www.mathworks.com/help/satcom/ref/satellitescenario.html); [Related reference 2](https://www.mathworks.com/help/satcom/ref/matlabshared.satellitescenario.access.accessintervals.html); [Related reference 3](https://www.mathworks.com/help/satcom/ref/satcom.satellitescenario.transmitter.link.html); [Related reference 4](https://www.mathworks.com/help/satcom/ref/satcom.satellitescenario.satellite.transmitter.link.ebno.html); [Related reference 5](https://www.mathworks.com/help/phased/ref/fspl.html); [Related reference 6](https://naif.jpl.nasa.gov/pub/naif/toolkit_docs/C/req/frames.html).
