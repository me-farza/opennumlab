// SPDX-License-Identifier: GPL-3.0-or-later
//! Selected sparse-page allocation/transfer observation, not a speed comparison.

use mathlab_engine::{
    BindingToken, Engine, EngineConfig, NumericComponents, PageData, PageWindow, ValuePageRequest,
    ValuePath, WorkspaceSnapshot, discover_octave,
};
use serde_json::json;
use std::{error::Error, path::PathBuf};

fn binding(snapshot: &WorkspaceSnapshot, name: &str) -> BindingToken {
    snapshot
        .variables
        .iter()
        .find(|entry| entry.variable.name == name)
        .expect("owned profile variable must exist")
        .token
        .clone()
}

fn request(token: BindingToken, rows: u32, columns: u32) -> ValuePageRequest {
    ValuePageRequest {
        path: ValuePath {
            root: token,
            steps: vec![],
        },
        window: PageWindow {
            row_start: 1.into(),
            column_start: 1.into(),
            rows,
            columns,
            higher_indices: vec![],
        },
    }
}

fn scalar(engine: &mut Engine, token: BindingToken) -> Result<f64, Box<dyn Error>> {
    let page = engine.read_value_page_checked(&request(token, 1, 1))?;
    let PageData::Dense {
        real: NumericComponents::Double(values),
        imag: None,
    } = page.data
    else {
        return Err("expected a real double process-statistic scalar".into());
    };
    if values.len() != 1 || !values[0].is_finite() || values[0] < 0.0 {
        return Err("process peak RSS is unavailable".into());
    }
    Ok(values[0])
}

fn execute(engine: &mut Engine, source: &str) -> Result<(), Box<dyn Error>> {
    let result = engine.execute(source)?;
    if let Some(error) = result.error {
        return Err(format!("{}: {}", error.identifier, error.message).into());
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let output = std::env::args().nth(1).map(PathBuf::from);
    let executable = discover_octave().ok_or("GNU Octave is required for this profile")?;
    let directory = tempfile::Builder::new()
        .prefix("numjinn-page-profile-")
        .tempdir()?;
    let mut engine = Engine::start(EngineConfig {
        executable: executable.clone(),
        working_directory: directory.path().into(),
    })?;
    execute(
        &mut engine,
        "S=sparse([1,100000],[1,1000],[2,-3],100000,1000); \
         stats=builtin('getrusage'); peak_before=stats.maxrss; clear stats;",
    )?;
    let snapshot = engine.workspace_snapshot()?;
    let before = scalar(&mut engine, binding(&snapshot, "peak_before"))?;
    let page_request = request(binding(&snapshot, "S"), 30, 12);
    let mut sizes = Vec::new();
    let mut milliseconds = Vec::new();
    for _ in 0..64 {
        let (page, metrics) = engine.read_value_page_checked_with_metrics(&page_request)?;
        let PageData::SparseCoo {
            rows,
            columns,
            real: NumericComponents::Double(values),
            imag: None,
        } = page.data
        else {
            return Err("sparse storage was not preserved".into());
        };
        if rows != [1] || columns != [1] || values != [2.0] {
            return Err("selected sparse page contains incorrect logical values".into());
        }
        sizes.push(metrics.result_json_bytes);
        milliseconds.push(metrics.elapsed.as_secs_f64() * 1000.0);
    }
    execute(
        &mut engine,
        "stats=builtin('getrusage'); peak_after=stats.maxrss; clear stats;",
    )?;
    let after_snapshot = engine.workspace_snapshot()?;
    let after = scalar(&mut engine, binding(&after_snapshot, "peak_after"))?;
    // Darwin reports bytes; Linux reports KiB. These platform units are sourced
    // in docs/VALUE_PAGE_PROTOCOL.md. Other platforms require their own evidence.
    let scale = if cfg!(target_os = "macos") {
        1.0
    } else if cfg!(target_os = "linux") {
        1024.0
    } else {
        return Err("peak RSS normalization is not qualified on this platform".into());
    };
    let before_bytes = (before * scale) as u64;
    let after_bytes = (after * scale) as u64;
    let increase = after_bytes.saturating_sub(before_bytes);
    milliseconds.sort_by(f64::total_cmp);
    let report = json!({
        "schema_version":1, "kind":"selected_sparse_page_profile",
        "platform":std::env::consts::OS, "runtime":executable,
        "octave_version":engine.version(), "worker_pid":engine.process_id(),
        "source_shape":[100000,1000], "source_nonzeros":2,
        "dense_equivalent_bytes":800_000_000_u64,
        "page_shape":[30,12], "returned_nonzeros":1, "iterations":64,
        "typed_result_json_bytes":{"min":sizes.iter().min(),"max":sizes.iter().max(),"total":sizes.iter().sum::<usize>()},
        "request_elapsed_ms":{"min":milliseconds[0],"median":milliseconds[milliseconds.len()/2],"max":milliseconds[milliseconds.len()-1]},
        "worker_peak_rss_bytes":{"before":before_bytes,"after":after_bytes,"increase":increase},
        "peak_growth_limit_bytes":64*1024*1024,
        "status":if increase < 64*1024*1024 {"passed_selected_observation"} else {"failed_peak_growth_limit"},
        "interpretation":"Development build. Elapsed time includes request serialization, worker, cancellation settlement and decode. Transfer counts typed result JSON only. RSS is the worker process high-water mark across this selected journey, including metadata/statistic queries; it is not a per-operation allocator count, client RSS, proof for all sparse inputs, or MATLAB performance comparison."
    });
    let encoded = serde_json::to_string_pretty(&report)? + "\n";
    if let Some(path) = output {
        std::fs::write(path, &encoded)?;
    }
    print!("{encoded}");
    if increase >= 64 * 1024 * 1024 {
        return Err("selected page journey exceeded its peak RSS growth limit".into());
    }
    Ok(())
}
