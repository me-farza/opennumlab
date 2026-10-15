// SPDX-License-Identifier: GPL-3.0-or-later
use mathlab_language::{SourceKind, prepare_script};
use std::path::Path;

fn prepare(source: &str) -> mathlab_language::PreparedScript {
    prepare_script(Path::new("example.m"), source).unwrap()
}

#[test]
fn extracts_forward_functions_without_changing_script_offsets() {
    let source = "result = twice(3);\nfunction y = twice(x)\ny = x * 2;\nend\n";
    let prepared = prepare(source);
    assert!(prepared.is_transformed());
    assert!(prepared.source.starts_with("result = twice(3);\n"));
    assert_eq!(prepared.source.len(), source.len());
    assert_eq!(prepared.source.lines().count(), source.lines().count());
    let function = &prepared.local_functions[0];
    assert_eq!(function.name, "twice");
    assert_eq!(function.span.start_line, 2);
    assert_eq!(function.span.end_line, 4);
    assert!(
        function
            .source
            .lines()
            .nth(1)
            .unwrap()
            .starts_with("function")
    );
    assert_eq!(
        &source[function.span.start..function.span.end],
        "function y = twice(x)\ny = x * 2;\nend"
    );
}

#[test]
fn comments_strings_and_transposes_do_not_create_blocks() {
    let source = r#"% function fake()
text = 'function y = fake(); end';
other = "function end";
escaped = 'isn''t function';
mixed = [other 'function; end'];
row = [1 2 3]; col = row'; plain = col.';
disp 'function; end'
%{
function not_real()
%{
end
%}
%}
answer = actual(row(end));
function y = actual(x)
    values = [x x; x x];
    y = values(end,end);
end
"#;
    let prepared = prepare(source);
    assert_eq!(prepared.local_functions.len(), 1);
    assert_eq!(prepared.local_functions[0].name, "actual");
    assert!(prepared.source.contains("disp 'function; end'"));
}

#[test]
fn continuations_multi_outputs_nested_functions_and_control_blocks() {
    let source = "[one,two] = pair(4);\nfunction [a, ... % end function\n b] = pair(x)\n if x > 0\n  for i=1:2\n   x = x + i;\n  end\n elseif x < 0\n  x = -x;\n else\n  x = 0;\n end\n a = inner(x); b = sibling(x);\n function y = inner(z)\n  y = z + 1;\n end\nend\nfunction y = sibling(x)\n try\n  switch x\n   case 1\n    y = 1;\n   otherwise\n    y = 2;\n  end\n catch err\n  y = 0;\n end\nend\n";
    let prepared = prepare(source);
    assert_eq!(prepared.local_functions.len(), 2);
    assert_eq!(prepared.local_functions[0].name, "pair");
    assert!(
        prepared.local_functions[0]
            .source
            .contains("function y = inner")
    );
    assert_eq!(prepared.local_functions[1].name, "sibling");
}

#[test]
fn accepts_octave_explicit_end_keywords_and_same_line_functions() {
    let prepared = prepare("z=f(1);function z=f(x);if x;z=x;endif;endfunction\n");
    assert_eq!(prepared.local_functions.len(), 1);
    assert_eq!(prepared.local_functions[0].name, "f");
    assert!(prepared.source.starts_with("z=f(1);"));
}

#[test]
fn function_and_class_files_pass_through_unchanged() {
    for source in [
        "% comment\nfunction y=f(x)\ny=x;\nend\n",
        "classdef Example\nend\n",
        "function y=f()\ny=\"\\\"\";\n", // Octave escaping and implicit end.
    ] {
        let prepared = prepare(source);
        assert_eq!(prepared.source, source);
        assert!(!prepared.is_transformed());
    }
}

#[test]
fn source_kind_uses_executable_tokens_after_comments_and_bom() {
    let leading = "\u{feff}%{\nfunction misleading()\nclassdef Misleading\n%}\n% explanation\n";
    for (body, expected) in [
        ("function y=f()\ny=1;\nend\n", SourceKind::Function),
        ("classdef Example\nend\n", SourceKind::Class),
        ("value=1;\n", SourceKind::Script),
        ("'function';\n", SourceKind::Script),
        ("\"classdef\";\n", SourceKind::Script),
        ("", SourceKind::Script),
    ] {
        assert_eq!(
            prepare(&format!("{leading}{body}")).source_kind().unwrap(),
            expected
        );
    }
    let prepared = prepare("value=f();\nfunction y=f()\ny=1;\nend\n");
    assert!(prepared.is_transformed());
    assert_eq!(prepared.source_kind().unwrap(), SourceKind::Script);
}

#[test]
fn continued_numeric_expression_does_not_start_a_new_function_statement() {
    let prepared = prepare("value = 1...\n + f();\nfunction y=f()\ny=2;\nend\n");
    assert_eq!(prepared.local_functions.len(), 1);
    assert!(prepared.source.starts_with("value = 1...\n + f();"));
}

#[test]
fn distinguishes_argument_blocks_from_ordinary_arguments_variables() {
    let prepared = prepare("a=f();\nfunction y=f()\narguments = 2;\ny=arguments;\nend\n");
    assert_eq!(prepared.local_functions.len(), 1);
    let prepared =
        prepare("a=f(2);\nfunction y=f(x)\narguments (Input)\nx (1,1) double\nend\ny=x;\nend\n");
    assert_eq!(prepared.local_functions.len(), 1);
}

#[test]
fn shell_commands_are_script_statements_without_language_blocks() {
    let prepared = prepare("!echo 'function; end'\na=f();\nfunction y=f()\ny=2;\nend\n");
    assert_eq!(prepared.local_functions.len(), 1);
    assert!(prepared.source.starts_with("!echo 'function; end'"));
}

#[test]
fn supports_script_statements_between_local_definitions() {
    let prepared = prepare(
        "a = first(1);\nfunction y=first(x)\ny=x;\nend\nb=second(a);\nfunction y=second(x)\ny=x+1;\nend\n",
    );
    assert_eq!(prepared.local_functions.len(), 2);
    assert!(prepared.source.contains("b=second(a);"));
}

#[test]
fn preserves_crlf_and_utf8_byte_spans() {
    let source = "a=f(); % π\r\nfunction y=f()\r\ny='λ';\r\nend\r\n";
    let prepared = prepare(source);
    assert_eq!(prepared.source.len(), source.len());
    assert_eq!(prepared.source.matches("\r\n").count(), 4);
    assert!(prepared.local_functions[0].source.contains("'λ'"));
}

#[test]
fn private_padding_retains_positions_without_copying_large_script_literals() {
    let source = format!(
        "long='{}';\n\n\t  function y=f()\ny=1;\nend\n",
        "a".repeat(32_000)
    );
    let prepared = prepare(&source);
    let function = &prepared.local_functions[0];
    assert_eq!(function.span.start_line, 3);
    assert!(function.source.starts_with("\n\n\t  function"));
    assert!(function.source.len() < 100);
    assert_eq!(prepared.source.len(), source.len());
}

#[test]
fn registration_anchor_keeps_block_comments_and_bom_intact() {
    let source = "\u{feff}%{\nfunction misleading()\nend\n%}\n% explanation\n  value=f();\nfunction result=f()\nresult=2;\nend\n";
    let prepared = prepare(source);
    let offset = prepared.scope_registration_offset().unwrap().unwrap();
    assert!(prepared.source[offset..].starts_with("value=f();"));
    assert_eq!(&prepared.source[..offset], &source[..offset]);
    let mut registered = prepared.source.clone();
    registered.insert_str(offset, "mathlab_scope_register('scope',{@f}); ");
    assert!(registered.starts_with("\u{feff}%{\nfunction misleading()\nend\n%}"));
    assert_eq!(
        registered.matches('\n').count(),
        source.matches('\n').count()
    );
    // The fake function inside the block comment remains invisible to parsing.
    assert!(!prepare(&registered).is_transformed());
}

#[test]
fn registration_anchor_is_absent_for_native_function_files_and_plain_scripts() {
    assert_eq!(
        prepare("function y=f()\ny=1;\nend\n")
            .scope_registration_offset()
            .unwrap(),
        None
    );
    assert_eq!(
        prepare("value=1;\n").scope_registration_offset().unwrap(),
        None
    );
}

#[test]
fn rejects_ambiguous_or_invalid_script_local_forms() {
    for (source, message) in [
        ("z=f();\nfunction y=f()\ny=1;\n", "Unclosed"),
        ("z=1;\nif z\nfunction f()\nend\nend", "inside control"),
        ("z=f();\nfunction f()\nend\nfunction f()\nend", "Duplicate"),
        (
            "z=f();\nfunction f()\nif 1\nendfunction\nend",
            "does not close",
        ),
        ("z=1;\nclassdef X\nend\nfunction f()\nend", "Local class"),
    ] {
        let failure = prepare_script(Path::new("bad.m"), source).unwrap_err();
        assert!(failure.message.contains(message), "{failure}");
    }
}

#[test]
fn apparent_block_comment_with_trailing_text_is_line_comment() {
    let prepared = prepare("%{ this is an ordinary comment\na=f();\nfunction y=f()\ny=1;\nend\n");
    assert_eq!(prepared.local_functions.len(), 1);
}

#[test]
fn materialization_rejects_overwriting_files_and_preserves_names() {
    let directory =
        std::env::temp_dir().join(format!("mathlab-materialization-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    let prepared = prepare("x = helper(2);\nfunction y = helper(x)\ny=x;\nend\n");
    let written = prepared.write_to(&directory).unwrap();
    assert!(
        written
            .script_path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("__mathlab_script_")
    );
    assert_eq!(written.script_path.extension().unwrap(), "mlsource");
    assert_eq!(
        written.source_mapping[0].original_path,
        Path::new("example.m")
    );
    assert_eq!(written.function_paths.len(), 1);
    assert_eq!(
        written.source_mapping[1].local_function.as_deref(),
        Some("helper")
    );
    assert_eq!(
        std::fs::read_to_string(&written.script_path).unwrap(),
        prepared.source
    );
    assert!(prepared.write_to(&directory).is_err());
    std::fs::remove_dir_all(directory).unwrap();
}
