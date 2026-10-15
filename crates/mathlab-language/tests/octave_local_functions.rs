// SPDX-License-Identifier: GPL-3.0-or-later
use mathlab_language::prepare_script;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static DIRECTORY_ID: AtomicUsize = AtomicUsize::new(0);

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        let id = DIRECTORY_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "mathlab-language-octave-{}-{id}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn octave(code: &str, directory: &Path) -> Option<String> {
    let executable = std::env::var_os("MATHLAB_OCTAVE").unwrap_or_else(|| "octave-cli".into());
    let output = match Command::new(executable)
        .args([
            "--quiet",
            "--no-gui",
            "--no-history",
            "--no-init-file",
            "--no-site-file",
            "--eval",
            code,
        ])
        .current_dir(directory)
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("Skipping Octave integration test: install octave-cli or set MATHLAB_OCTAVE");
            return None;
        }
        Err(error) => panic!("Cannot start Octave: {error}"),
    };
    assert!(
        output.status.success(),
        "Octave failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Some(String::from_utf8(output.stdout).unwrap())
}

fn literal(path: &Path) -> String {
    format!("'{}'", path.to_str().unwrap().replace('\'', "''"))
}

fn run(path: &Path) -> String {
    format!(
        "saved_path=path(); addpath({}); source({}, 'base'); path(saved_path); clear saved_path;",
        literal(path.parent().unwrap()),
        literal(path)
    )
}

#[test]
fn unchanged_matlab_fixture_runs_in_base_workspace_with_nested_functions() {
    let temporary = Directory::new();
    let fixture = include_str!("../../../tests/fixtures/script_local_functions.m");
    let prepared = prepare_script(Path::new("script_local_functions.m"), fixture).unwrap();
    let written = prepared.write_to(&temporary.0.join("fixture")).unwrap();
    // Direct calls outside the script must be invisible; retained scoped handles work.
    let code = format!(
        "{} assert(local_sum == 10); assert(local_handle([3 7]) == 10); assert(exist('local_statistics', 'file') == 0);",
        run(&written.script_path)
    );
    octave(&code, &temporary.0);
}

#[test]
fn local_functions_shadow_global_functions_but_base_variables_take_precedence() {
    let temporary = Directory::new();
    std::fs::write(
        temporary.0.join("twice.m"),
        "function y=twice(x)\ny=999;\nend\n",
    )
    .unwrap();
    let prepared = prepare_script(
        Path::new("scope.m"),
        "result=twice(3);\nfunction y=twice(x)\ny=x*2;\nend\n",
    )
    .unwrap();
    let written = prepared.write_to(&temporary.0.join("scope")).unwrap();
    let code = format!(
        "{} assert(result == 6); twice=@(x)x+10; {} assert(result == 13); clear twice; assert(twice(3) == 999);",
        run(&written.script_path),
        run(&written.script_path)
    );
    octave(&code, &temporary.0);
}

#[test]
fn same_names_in_different_scripts_and_retained_sibling_handles_are_isolated() {
    let temporary = Directory::new();
    let first = "h=@local; first=local(5);\nfunction y=local(x)\ny=helper(x);\nend\nfunction y=helper(x)\ny=x*2;\nend\n";
    let second = "h2=@local; second=local(5);\nfunction y=local(x)\ny=helper(x);\nend\nfunction y=helper(x)\ny=x*3;\nend\n";
    let a = prepare_script(Path::new("first.m"), first)
        .unwrap()
        .write_to(&temporary.0.join("first"))
        .unwrap();
    let b = prepare_script(Path::new("second.m"), second)
        .unwrap()
        .write_to(&temporary.0.join("second"))
        .unwrap();
    let code = format!(
        "{} {} assert(first==10); assert(second==15); assert(h(7)==14); assert(h2(7)==21); assert(exist('local','file')==0); assert(exist('helper','file')==0);",
        run(&a.script_path),
        run(&b.script_path)
    );
    octave(&code, &temporary.0);
}

#[test]
fn function_error_lines_retain_the_original_file_positions() {
    let temporary = Directory::new();
    let source = "value=failure(2);\n\nfunction y=failure(x)\n y=x;\n error('MathLab:Fixture','expected failure');\nend\n";
    let prepared = prepare_script(Path::new("line_mapping.m"), source).unwrap();
    let written = prepared.write_to(&temporary.0.join("failure")).unwrap();
    let code = format!(
        "saved_path=path(); addpath({}); try; source({},'base'); error('expected a failure'); catch caught; assert(strcmp(caught.identifier,'MathLab:Fixture')); assert(caught.stack(1).line==5); assert(strcmp(caught.stack(1).file,{})); assert(caught.stack(2).line==1); end; path(saved_path);",
        literal(written.script_path.parent().unwrap()),
        literal(&written.script_path),
        literal(&written.function_paths[0])
    );
    octave(&code, &temporary.0);
}

#[test]
fn persistent_scope_can_survive_without_exposing_a_script_copy() {
    let temporary = Directory::new();
    let source = "local_count=counter();\nfunction result=counter()\n persistent count=0;\n count=count+1;\n result=count;\nend\n";
    let prepared = prepare_script(Path::new("persistent_local.m"), source).unwrap();
    let written = prepared
        .write_to(&temporary.0.join("persistent_scope"))
        .unwrap();
    let stem = written.script_path.file_stem().unwrap().to_str().unwrap();
    let code = format!(
        "start_directory=pwd(); addpath({},'-end'); source({},'base'); assert(local_count==1); assert(exist('{stem}','file')==0); source({},'base'); assert(local_count==2); assert(exist('{stem}','file')==0); assert(exist('counter','file')==0); assert(strcmp(pwd(),start_directory));",
        literal(written.script_path.parent().unwrap()),
        literal(&written.script_path),
        literal(&written.script_path)
    );
    octave(&code, &temporary.0);
}

#[test]
fn immutable_non_m_script_preserves_persistent_state_across_dispatch_activations() {
    let temporary = Directory::new();
    let source = "count_result=counter(); scoped_handle=@counter;\nfunction result=counter()\n persistent count=0;\n count=count+1; result=count;\nend\n";
    let written = prepare_script(Path::new("counter_script.m"), source)
        .unwrap()
        .write_to(&temporary.0.join("prepared"))
        .unwrap();
    std::fs::write(
        temporary.0.join("dispatch.m"),
        "function dispatch(source_path)\n captured=evalc(\"source(source_path,'base');\");\nend\n",
    )
    .unwrap();
    let stem = written.script_path.file_stem().unwrap().to_str().unwrap();
    let code = format!(
        "addpath({}); addpath({},'-end'); dispatch({}); assert(count_result==1); dispatch({}); assert(count_result==2); assert(scoped_handle()==3); assert(exist('{stem}','file')==0); assert(exist('counter','file')==0);",
        literal(&temporary.0),
        literal(written.script_path.parent().unwrap()),
        literal(&written.script_path),
        literal(&written.script_path)
    );
    octave(&code, &temporary.0);
}

#[test]
fn saved_original_in_current_directory_cannot_hijack_generated_script_parsing() {
    let temporary = Directory::new();
    let fixture = include_str!("../../../tests/fixtures/script_local_functions.m");
    let original = temporary.0.join("script_local_functions.m");
    std::fs::write(&original, fixture).unwrap();
    let prepared = prepare_script(&original, fixture).unwrap();
    let written = prepared.write_to(&temporary.0.join("prepared")).unwrap();
    // The saved original contains a nested local function that Octave cannot
    // parse as a script, exposing basename collisions during private lookup.
    let code = format!(
        "start_directory=pwd(); addpath({},'-end'); source({},'base'); assert(local_sum==10); assert(strcmp(pwd(),start_directory));",
        literal(written.script_path.parent().unwrap()),
        literal(&written.script_path)
    );
    octave(&code, &temporary.0);
}

#[test]
fn two_saved_originals_with_same_basename_have_distinct_private_scopes() {
    let temporary = Directory::new();
    let first = "h=@local; first=local(5);\nfunction y=local(x)\ny=helper(x);\nend\nfunction y=helper(x)\ny=x*2;\nend\n";
    let second = first
        .replace("h=@local; first=", "h2=@local; second=")
        .replace("x*2", "x*3");
    let project_a = temporary.0.join("project_a");
    let project_b = temporary.0.join("project_b");
    std::fs::create_dir(&project_a).unwrap();
    std::fs::create_dir(&project_b).unwrap();
    let original_a = project_a.join("sample.m");
    let original_b = project_b.join("sample.m");
    std::fs::write(&original_a, first).unwrap();
    std::fs::write(&original_b, &second).unwrap();
    let a = prepare_script(&original_a, first)
        .unwrap()
        .write_to(&temporary.0.join("generated_a"))
        .unwrap();
    let b = prepare_script(&original_b, &second)
        .unwrap()
        .write_to(&temporary.0.join("generated_b"))
        .unwrap();
    assert_ne!(a.script_path.file_name(), b.script_path.file_name());
    let code = format!(
        "start_directory=pwd(); addpath({},'-end'); source({},'base'); addpath({},'-end'); source({},'base'); assert(first==10); assert(second==15); assert(h(8)==16); assert(h2(8)==24); assert(exist('local','file')==0); assert(exist('helper','file')==0); assert(strcmp(pwd(),start_directory));",
        literal(a.script_path.parent().unwrap()),
        literal(&a.script_path),
        literal(b.script_path.parent().unwrap()),
        literal(&b.script_path)
    );
    octave(&code, &project_a);
}
