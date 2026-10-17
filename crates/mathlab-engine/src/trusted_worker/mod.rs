// SPDX-License-Identifier: GPL-3.0-or-later
//! Pinned worker qualification selected only by explicit qualified startup.
//! The complete reviewed program is pinned, rather than heuristically rewriting
//! arbitrary M source at runtime. Any owned-source drift requires a new review.

use crate::{Error, Result};
use sha2::{Digest, Sha256};

const WORKER: &str = include_str!("inputs/mathlab_worker.m");
const VALUE_PAGE: &str = include_str!("inputs/mathlab_value_page.m");
const VALUE_EDIT: &str = include_str!("inputs/mathlab_value_edit.m");
const READY_JSON: &str = include_str!("inputs/mathlab_ready_json.m");
const CORE: &str = include_str!("core_worker.m");
const INTRINSICS: &str = include_str!("inputs/mathlab_intrinsics.m");
const INTRINSIC_KEEPER_END: &str =
    "\n## Carrier values are produced by the private native provider, never requests.";
const GRAPHICS_WORKER: &str = include_str!("inputs/mathlab_worker_graphics.m");
const GRAPHICS_SOURCE: &str = include_str!("inputs/mathlab_graphics_service.m");
const REGISTRY: &str = include_str!("qualified_registry.m");

include!("required_primitives.rs");
include!("required_graphics_primitives.rs");

const GRAPHICS_FRAGMENT: &str = include_str!("graphics_service.m");
const INTRINSICS_MARKER: &str = r#"## SPDX-License-Identifier: GPL-3.0-or-later
## Private V2 source: append once to the exact owned worker closure.
"#;

// SHA-256 of the complete reviewed graphics assembly, including direct caller locking.
const GRAPHICS_SHA256: [u8; 32] = [
    0xf5, 0x7c, 0x35, 0xa0, 0xf9, 0x5a, 0xa3, 0x61, 0x59, 0x81, 0x12, 0x9f, 0x75, 0x2d, 0x0f, 0x37,
    0xfb, 0xf9, 0xea, 0xf9, 0xf2, 0x34, 0xb8, 0x80, 0xbd, 0x82, 0x04, 0xda, 0x3b, 0xbf, 0x96, 0xc9,
];

fn graphics_program() -> Result<String> {
    // Only this fixed reviewed core is an input. These seven substitutions are
    // graphics hooks, not a patch format or transformation of admitted M files.
    // The final digest rejects every missing/extra hook or shared-core drift.
    let program = CORE
        .replacen(
            r#"  value_bridge = [];
"#,
            r#"  value_bridge = [];
  graphics_dispatch = [];
  graphics_bridge_path = '';
"#,
            1,
        )
        .replacen(
            r#"      value_bridge = mathlab_value_bridge_open (value_bridge_path);
"#,
            r#"      value_bridge = mathlab_value_bridge_open (value_bridge_path);
      [graphics_dispatch, graphics_bridge_path] = mathlab_graphics_restore (session_directory, response_prefix, generation, invocation);
"#,
            1,
        )
        .replacen(
            r#"mathlab_ready (response_prefix, generation, invocation, binding_revision, value_bridge)"#,
            r#"mathlab_ready (response_prefix, generation, invocation, binding_revision, value_bridge, ! builtin('isempty', graphics_dispatch))"#,
            3,
        )
        .replacen(
            r#"        execution_request = builtin('any', builtin('strcmp', request.command, {'execute', 'run_file'}));"#,
            r#"        execution_request = builtin('any', builtin('strcmp', request.command, {'execute', 'run_file', 'graphics_dispatch_callback'}));"#,
            1,
        )
        .replacen(
            r#"          case 'ping'
"#,
            r#"          case 'enable_graphics'
            if (builtin('isempty', graphics_dispatch))
              [graphics_dispatch, graphics_bridge_path] = mathlab_graphics_enable (session_directory, request.bridge_path, response_prefix, generation, invocation);
            elseif (! builtin('ischar', request.bridge_path) || ...
                    ! builtin('strcmp', builtin('canonicalize_file_name', request.bridge_path), graphics_bridge_path))
              builtin('error', 'MathLab:GraphicsUnsupported', 'A graphics bridge is already retained; restart before selecting another module.');
            endif
            ## Re-enabling the same module preserves object IDs/lifetimes.
            result = mathlab_ready (response_prefix, generation, invocation, binding_revision, value_bridge, (1 == 1));
            result_is_ready = (1 == 1);
          case {'graphics_snapshot', 'graphics_query', 'graphics_mutate', ...
                'graphics_enqueue_callback', 'graphics_cancel_callback', 'graphics_dispatch_callback'}
            if (builtin('isempty', graphics_dispatch))
              builtin('error', 'MathLab:GraphicsUnsupported', 'Enable the qualified native graphics lifetime bridge before graphics requests.');
            endif
            ## Every graphics command retires CP03 history before dispatch,
            ## including reads, errors and Stop. Keep the pure whitelist intact.
            context = mathlab_binding_context (response_prefix, generation, invocation, binding_revision);
            result = builtin('feval', graphics_dispatch, request, context);
          case 'ping'
"#,
            1,
        )
        .replacen(
            r#"function result = mathlab_ready (worker_id, generation, invocation, revision, value_bridge = [])"#,
            r#"function result = mathlab_ready (worker_id, generation, invocation, revision, value_bridge = [], graphics_enabled = (1 == 0))"#,
            1,
        )
        .replacen(
            r#"  ## Retain exact native integers here. The explicit Ready producer encodes
"#,
            r#"  if (graphics_enabled)
    capabilities{end + 1} = 'graphics_registry.v1';
  endif
  ## Retain exact native integers here. The explicit Ready producer encodes
"#,
            1,
        );
    let (body, intrinsics) = program.split_once(INTRINSICS_MARKER).ok_or_else(|| {
        Error::Protocol("The reviewed graphics insertion boundary changed.".into())
    })?;
    let source = format!("{body}{GRAPHICS_FRAGMENT}{INTRINSICS_MARKER}{intrinsics}");
    if <[u8; 32]>::from(Sha256::digest(source.as_bytes())) != GRAPHICS_SHA256 {
        return Err(Error::Protocol(
            "The reviewed graphics assembly changed; review the complete qualified program.".into(),
        ));
    }
    Ok(source)
}

pub(crate) struct OwnedWorkerParts<'a> {
    pub worker: &'a str,
    pub value_page: &'a str,
    pub value_edit: &'a str,
    pub ready_json: &'a str,
}

pub(crate) struct QualifiedWorker {
    pub source: String,
    pub registry_source: &'static str,
    pub required_primitives: &'static [&'static str],
}

/// Qualify one complete, exact reviewed program with one retained guard.
/// Graphics selects its separately pinned worker hooks and complete service;
/// the graphics-disabled core source and primitive profile remain unchanged.
/// Native bootstrap must admit every returned primitive before project entry.
/// This is a source admission boundary, not an ABI or purity proof. Legacy
/// worker text is intentionally rejected: the callback-routing seam is required.
pub(crate) fn qualify_worker_source(
    parts: OwnedWorkerParts<'_>,
    graphics_source: Option<&str>,
) -> Result<QualifiedWorker> {
    // The other helper functions include reviewed generated rewrites. The
    // keeper itself must be emitted byte for byte from its shared input.
    let Some((keeper, _)) = INTRINSICS.split_once(INTRINSIC_KEEPER_END) else {
        return Err(Error::Protocol(
            "The intrinsic keeper boundary changed; review the complete qualified program.".into(),
        ));
    };
    if CORE.matches(keeper).count() != 1 {
        return Err(Error::Protocol(
            "The embedded intrinsic helper changed; review the complete qualified program.".into(),
        ));
    }
    let (reviewed_worker, required_primitives) = match graphics_source {
        None => (WORKER, REQUIRED_PRIMITIVES),
        Some(actual) if actual == GRAPHICS_SOURCE => {
            (GRAPHICS_WORKER, GRAPHICS_REQUIRED_PRIMITIVES)
        }
        Some(_) => {
            return Err(Error::Protocol(
                "The owned graphics source changed; review the complete qualified program.".into(),
            ));
        }
    };
    for (name, actual, reviewed) in [
        ("worker", parts.worker, reviewed_worker),
        ("value page", parts.value_page, VALUE_PAGE),
        ("value edit", parts.value_edit, VALUE_EDIT),
        ("Ready JSON", parts.ready_json, READY_JSON),
    ] {
        if actual != reviewed {
            return Err(Error::Protocol(format!(
                "The owned {name} source changed; review the complete qualified program."
            )));
        }
    }
    Ok(QualifiedWorker {
        source: if graphics_source.is_some() {
            graphics_program()?
        } else {
            CORE.to_owned()
        },
        registry_source: REGISTRY,
        required_primitives,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts<'a>() -> OwnedWorkerParts<'a> {
        OwnedWorkerParts {
            worker: WORKER,
            value_page: VALUE_PAGE,
            value_edit: VALUE_EDIT,
            ready_json: READY_JSON,
        }
    }

    #[test]
    fn all_owned_functions_share_one_guard_including_current_ready() {
        let program = qualify_worker_source(parts(), None).unwrap();
        assert_eq!(
            program
                .source
                .matches("function [dispatcher, bridge, adapters] = mathlab_intrinsics")
                .count(),
            1
        );
        assert_eq!(
            program
                .source
                .matches("mathlab_intrinsics (1, intrinsic_bindings);")
                .count(),
            1
        );
        assert!(
            program
                .source
                .contains("function text = mathlab_ready_json (")
        );
        assert!(
            program
                .source
                .contains("result.intrinsics = mathlab_intrinsic_status ();")
        );
        assert!(
            program
                .source
                .contains("result = bridge('intrinsic_status');")
        );
        assert!(
            program
                .source
                .contains("function [candidate, receipt] = mathlab_edit_history_accept_restore (")
        );
        assert!(program.required_primitives.contains(&"isobject"));
        assert!(program.required_primitives.contains(&"signbit"));
        assert!(
            program
                .required_primitives
                .windows(2)
                .all(|pair| pair[0] < pair[1])
        );
        assert!(program.registry_source.contains("__ml_registry_provider__"));
        assert!(!program.source.contains("(@builtin)"));
        assert!(!program.source.contains("builtin(type,"));
    }

    #[test]
    fn both_emitted_profiles_embed_the_exact_reviewed_intrinsic_keeper() {
        let (keeper, _) = INTRINSICS.split_once(INTRINSIC_KEEPER_END).unwrap();
        for source in [CORE.to_owned(), graphics_program().unwrap()] {
            assert_eq!(source.matches(keeper).count(), 1);
            assert!(!source.contains("dispatcher('mlock')"));
        }
    }

    #[test]
    fn changed_ready_producer_or_new_primitive_cannot_escape_the_pin() {
        let changed = READY_JSON.replace("'signbit'", "'unknown_primitive'");
        let mut input = parts();
        input.ready_json = &changed;
        assert!(qualify_worker_source(input, None).is_err());
        let mut input = parts();
        input.ready_json = "";
        assert!(qualify_worker_source(input, None).is_err());
    }

    #[test]
    fn duplicate_guard_and_callback_signature_drift_fail_admission() {
        let duplicate =
            format!("{WORKER}\nfunction builtin = mathlab_intrinsics ()\nendfunction\n");
        let mut input = parts();
        input.worker = &duplicate;
        assert!(qualify_worker_source(input, None).is_err());
        let changed = WORKER.replace("intrinsic_bindings = []", "other_bindings = []");
        let mut input = parts();
        input.worker = &changed;
        assert!(qualify_worker_source(input, None).is_err());
    }

    #[test]
    fn unreviewed_graphics_is_never_silently_omitted() {
        assert!(qualify_worker_source(parts(), Some("")).is_err());
        assert!(
            qualify_worker_source(parts(), Some("function graphics ()\nendfunction\n")).is_err()
        );
    }

    fn graphics_parts<'a>() -> OwnedWorkerParts<'a> {
        OwnedWorkerParts {
            worker: GRAPHICS_WORKER,
            ..parts()
        }
    }

    #[test]
    fn graphics_disabled_result_preserves_the_reviewed_core_bytes_and_profile() {
        let core = qualify_worker_source(parts(), None).unwrap();
        assert_eq!(core.source.as_bytes(), CORE.as_bytes());
        assert_eq!(core.registry_source, REGISTRY);
        assert_eq!(core.required_primitives, REQUIRED_PRIMITIVES);
        assert_eq!(core.required_primitives.len(), 115);
        assert!(
            !core
                .source
                .contains("function dispatch = mathlab_graphics_service")
        );
        assert!(qualify_worker_source(graphics_parts(), None).is_err());
    }

    #[test]
    fn exact_graphics_closure_shares_one_guard_and_retains_every_nested_function() {
        let program = qualify_worker_source(graphics_parts(), Some(GRAPHICS_SOURCE)).unwrap();
        assert_eq!(
            <[u8; 32]>::from(Sha256::digest(program.source.as_bytes())),
            GRAPHICS_SHA256
        );
        assert_eq!(program.registry_source, REGISTRY);
        assert_eq!(
            program
                .source
                .matches("function [dispatcher, bridge, adapters] = mathlab_intrinsics")
                .count(),
            1
        );
        assert_eq!(
            program
                .source
                .matches("mathlab_intrinsics (1, intrinsic_bindings);")
                .count(),
            1
        );
        let headers: Vec<_> = GRAPHICS_SOURCE
            .lines()
            .filter(|line| line.trim_start().starts_with("function "))
            .collect();
        assert_eq!(headers.len(), 35);
        assert_eq!(
            headers
                .iter()
                .filter(|line| line.starts_with("  function "))
                .count(),
            19
        );
        for header in headers {
            let indent = &header[..header.len() - header.trim_start().len()];
            let guarded_header = format!("{header}\n{indent}  builtin = mathlab_intrinsics ();\n");
            assert_eq!(
                program.source.matches(guarded_header.as_str()).count(),
                1,
                "{header}"
            );
        }
        let function_count = |source: &str| {
            source
                .lines()
                .filter(|line| line.trim_start().starts_with("function "))
                .count()
        };
        assert_eq!(function_count(&program.source), function_count(CORE) + 35);
        assert!(
            program
                .source
                .contains("[bridge, expected] = mathlab_native_adapter ('graphics', expected);")
        );
        assert!(!program.source.contains("mathlab_graphics_primitives"));
        assert!(!program.source.contains("= @builtin"));
        assert!(!program.source.contains("(@builtin)"));
    }

    #[test]
    fn graphics_profile_adds_exactly_the_five_required_genuine_targets() {
        use std::collections::BTreeSet;

        let program = qualify_worker_source(graphics_parts(), Some(GRAPHICS_SOURCE)).unwrap();
        assert_eq!(program.required_primitives, GRAPHICS_REQUIRED_PRIMITIVES);
        assert_eq!(program.required_primitives.len(), 120);
        assert!(
            program
                .required_primitives
                .windows(2)
                .all(|pair| pair[0] < pair[1])
        );
        let core: BTreeSet<_> = REQUIRED_PRIMITIVES.iter().copied().collect();
        let graphics: BTreeSet<_> = program.required_primitives.iter().copied().collect();
        assert!(core.is_subset(&graphics));
        assert_eq!(
            graphics.difference(&core).copied().collect::<Vec<_>>(),
            ["all", "fprintf", "fread", "functions", "rethrow"]
        );
    }

    #[test]
    fn changed_top_level_or_nested_graphics_code_cannot_escape_the_pin() {
        for changed in [
            GRAPHICS_SOURCE.replace(
                "function dispatch = mathlab_graphics_service (",
                "function dispatch = changed_graphics_service (",
            ),
            GRAPHICS_SOURCE.replace(
                "  function result = handle_request (",
                "  function result = changed_handle_request (",
            ),
            GRAPHICS_SOURCE.replace(
                "builtin('rethrow', caught)",
                "builtin('unknown_primitive', caught)",
            ),
            format!("{GRAPHICS_SOURCE}\nfunction injected_graphics ()\nendfunction\n"),
        ] {
            assert_ne!(changed, GRAPHICS_SOURCE);
            assert!(qualify_worker_source(graphics_parts(), Some(&changed)).is_err());
        }
        assert!(qualify_worker_source(graphics_parts(), Some("")).is_err());
        assert!(qualify_worker_source(parts(), Some(GRAPHICS_SOURCE)).is_err());
    }

    #[test]
    fn graphics_requires_the_exact_worker_hooks_and_all_three_other_core_inputs() {
        let changed_worker =
            GRAPHICS_WORKER.replace("case 'enable_graphics'", "case 'different_graphics'");
        assert_ne!(changed_worker, GRAPHICS_WORKER);
        let mut input = graphics_parts();
        input.worker = &changed_worker;
        assert!(qualify_worker_source(input, Some(GRAPHICS_SOURCE)).is_err());
        let mut input = graphics_parts();
        input.value_page = "";
        assert!(qualify_worker_source(input, Some(GRAPHICS_SOURCE)).is_err());
        let mut input = graphics_parts();
        input.value_edit = "";
        assert!(qualify_worker_source(input, Some(GRAPHICS_SOURCE)).is_err());
        let mut input = graphics_parts();
        input.ready_json = "";
        assert!(qualify_worker_source(input, Some(GRAPHICS_SOURCE)).is_err());
    }

    #[test]
    fn graphics_ready_and_history_still_use_the_shared_worker_boundaries() {
        let program = qualify_worker_source(graphics_parts(), Some(GRAPHICS_SOURCE)).unwrap();
        assert_eq!(
            program
                .source
                .matches("result.intrinsics = mathlab_intrinsic_status ();")
                .count(),
            1
        );
        assert_eq!(
            program
                .source
                .matches("function text = mathlab_ready_json (")
                .count(),
            1
        );
        assert!(
            program
                .source
                .contains("value_bridge = [], graphics_enabled = (1 == 0)")
        );
        assert!(
            program
                .source
                .contains("capabilities{end + 1} = 'graphics_registry.v1';")
        );
        assert!(program.source.contains("'graphics_dispatch_callback'}));"));
        assert!(
            program
                .source
                .contains("Every graphics command retires CP03 history before dispatch,")
        );
        let whitelist = "{'ping', 'workspace_snapshot', ...\n                                            'read_variable_checked', 'set_variable_element_checked', ...\n                                            'read_value_page_checked', 'write_value_range_checked', ...\n                                            'undo_value_edit_checked', 'redo_value_edit_checked'}";
        assert!(CORE.contains(whitelist));
        assert!(program.source.contains(whitelist));
    }
}
