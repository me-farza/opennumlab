# Local evidence citation owner handoff

Ten proposed owner patches convert 50 hyperlinks to 32 deliberately excluded
local evidence paths into labeled path citations. The foreign documents remain
unchanged; these fragments are preparation for a future source snapshot.

The inventory reads committed source `9a317b1dfbbe1f25b4b6d4a8c7af334ac5444bd3`,
tree `baaf2adb34722f7dc76f787e2437a57219a1e11f`, using its unchanged source
selection policy. This pin selects 224 source files, 226 public-tree files and
47 Markdown documents. These are selection counts, not newly generated exports.

Of 239 relative Markdown hyperlinks, 189 resolve to 75 selected file or
directory targets. The remaining 50 name excluded `artifacts/` records. No
current code or product-document target is missing. All existing release-owned
document links resolve, so no existing `docs/release` document needs conversion.

| Document | Proposed conversions |
| --- | ---: |
| [Development plan](../../DEVELOPMENT_PLAN.md) | 1 |
| [Engine README](../../crates/mathlab-engine/README.md) | 3 |
| [Console protocol](../CONSOLE_PROTOCOL.md) | 18 |
| [Kernel readiness](../KERNEL_READINESS.md) | 3 |
| [Value edit protocol](../VALUE_EDIT_PROTOCOL.md) | 12 |
| [Value page protocol](../VALUE_PAGE_PROTOCOL.md) | 1 |
| [Workspace protocol](../WORKSPACE_PROTOCOL.md) | 1 |
| [Native input bridge](../../native/INPUT_BRIDGE.md) | 4 |
| [Native README](../../native/README.md) | 4 |
| [Native value bridge](../../native/VALUE_BRIDGE.md) | 3 |

Each replacement retains the exact visible label and original relative path:

```markdown
validation evidence (local evidence unavailable in source exports: `artifacts/VALIDATION.md`)
```

Multiline labels retain their line breaks. Existing plain path citations are
outside this conversion. No path is redirected to an invented exported file,
and evidence availability does not change any recorded result or its scope.

## Review material

Ignored preparation directory:
`dist/opennumlab-local-evidence-link-preparation-20261015/`.

- `current-9a317b1-markdown-link-inventory.json` records the pinned selection and
  complete link spans, labels and destinations. SHA-256:
  `e7d48801f44c946be45086eeaf7efa0319f3c82dc9e2caf5dbbf4bef4ba5c4e0`.
- `patches/<document>.patch` contains one fragment per foreign document;
  `proposed/<document>` contains its proposed text. No fragment was applied.
- `all-owners-proposed.patch` contains all ten fragments. SHA-256:
  `6598cfb257e1da8bc287c0e13cb93053248571237630d51dcaa1d0e6a1f4356d`.
- `proposal-preservation-report.json` records each base blob, preimage and
  proposed SHA-256, replacement ledger and static checks. SHA-256:
  `9778429de99c96153e6ffbe4531884242b66ce034b8ce3548a48d3e117919a77`.
- `independent-native-ci-review.json` independently confirms the exact 50
  substitutions against raw pinned blobs, including all 17 fenced blocks and
  three multiline labels. SHA-256:
  `4dc2ae95ddbbee92226b60bd3e0c66cb95887a7d54a50278fd327e047d43acf1`.

Static comparisons reconstruct every original document byte for byte by
reversing only the recorded replacements. All labels, exact target paths,
other hyperlinks, code fences, identity hashes, line breaks and table
separators are preserved. Normative JSON and surrounding results prose remain
unchanged. The valid `crates/mathlab-desktop/` directory hyperlink is retained.

The current pinned documents still contain all 50 unavailable evidence
hyperlinks. Applying all ten reviewed proposals would replace them with 50
explicit local-evidence citations, leave zero unavailable relative hyperlinks,
and retain the same 189 resolving relative links to 75 targets. Owners must
compare each preimage with their current file before integrating its fragment;
later edits require a refreshed review.

This preparation used static text and hash checks only. It did not run builds,
tests, runtimes, GUI operations or network requests, widen selection, include
raw evidence, or change a schema or proof claim. It did not modify the immutable
`5fa9b5c` artifacts or the prepared `ccb4a01` private-update checkout. Their
review and remote-approval status remain separate from this future cleanup.
