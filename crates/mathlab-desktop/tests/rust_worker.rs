//! Real existing desktop actor and existing class engine, without a GUI substitute evaluator.
use mathlab_desktop::{
    rust_worker::*,
    worker::{self, Event, Request},
};
use std::{
    path::{Path, PathBuf},
    sync::mpsc::{Receiver, Sender},
    time::Duration,
};
struct Actor {
    tx: Sender<Request>,
    rx: Receiver<Event>,
    gate: NativeGate,
    generation: u64,
    job: u64,
}
impl Actor {
    fn new() -> Self {
        let (tx, rx) = worker::spawn(eframe::egui::Context::default());
        let mut a = Self {
            tx,
            rx,
            gate: NativeGate::default(),
            generation: 0,
            job: 0,
        };
        a.reset();
        a
    }
    fn reset(&mut self) {
        self.generation += 1;
        self.job = 0;
        self.tx
            .send(Request::NativeStart {
                generation: self.generation,
                lease: self.gate.start().unwrap(),
            })
            .unwrap();
        match self.next() {
            NativeEvent::Ready { generation, lease } => {
                assert_eq!(generation, self.generation);
                drop(lease)
            }
            other => panic!("{other:?}"),
        }
    }
    fn next(&self) -> NativeEvent {
        match self
            .rx
            .recv_timeout(Duration::from_secs(5))
            .expect("bounded actor reply")
        {
            Event::Native(e) => e,
            _ => panic!("native route emitted legacy event"),
        }
    }
    fn request(&mut self, text: &str, modules: &[PathBuf]) -> RunRequest {
        self.job += 1;
        RunRequest::new(
            &self.gate,
            JobKey {
                generation: self.generation,
                job: self.job,
            },
            7,
            Path::new("/private/tmp/desktop-unsaved-entry.m"),
            text,
            modules,
        )
        .unwrap()
    }
    fn finish(&self) -> Finished {
        loop {
            match self.next() {
                NativeEvent::Prepared { .. } => {}
                NativeEvent::Finished(f) => {
                    assert!(f.owned_bytes() <= MAX_RESPONSE_BYTES);
                    return f;
                }
                e => panic!("{e:?}"),
            }
        }
    }
    fn run(&mut self, text: &str, modules: &[PathBuf]) -> Finished {
        let r = self.request(text, modules);
        self.tx.send(Request::NativeRun(r)).unwrap();
        self.finish()
    }
    fn page(
        &self,
        token: &PageToken,
        row: usize,
        col: usize,
        higher: &[u64],
        rows: usize,
        columns: usize,
    ) -> (Page, Option<Lease>) {
        let r = PageRequest::new(&self.gate, 1, token, row, col, higher, rows, columns).unwrap();
        self.tx.send(Request::NativeRead(r)).unwrap();
        match self.next() {
            NativeEvent::Page {
                result: Ok(page),
                lease,
                ..
            } => (page, lease),
            e => panic!("{e:?}"),
        }
    }
}
fn token(f: &Finished, name: &str) -> PageToken {
    PageToken {
        key: f.key,
        revision: f.revision,
        binding: name.into(),
    }
}
fn success(f: &Finished) {
    assert_eq!(f.status, Status::Success, "{:?}", f.error);
    assert!(f.execution_started);
    assert!(f.error.is_none());
}
fn numbers(page: &Page) -> Vec<f64> {
    page.cells
        .iter()
        .flat_map(|r| r.iter())
        .map(|c| c.text.parse().unwrap())
        .collect()
}

#[test]
fn actor_runs_unsaved_entry_retains_workspace_and_immutable_sources() {
    let mut a = Actor::new();
    let mut text = "seed=17;matrix=[1,2;3,4];".to_owned();
    let request = a.request(&text, &[]);
    text.clear();
    a.tx.send(Request::NativeRun(request)).unwrap();
    let f = a.finish();
    success(&f);
    assert!(f.sources[0].text.contains("seed=17"));
    assert!(f.graph_sha256.is_some());
    let t = token(&f, "matrix");
    let (p, lease) = a.page(&t, 0, 0, &[], 30, 12);
    assert_eq!(numbers(&p), [1., 2., 3., 4.]);
    drop((p, lease, f));
    let f = a.run("observed=seed+1;", &[]);
    success(&f);
    let (p, _lease) = a.page(&token(&f, "observed"), 0, 0, &[], 1, 1);
    assert_eq!(numbers(&p), [18.]);
}

#[test]
fn actor_executes_all_six_original_class_graphs_with_real_workspace_pages() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../docs/research/matlab-r2026b/probes/rust-class-foundation-source-reference/fixtures",
    );
    let cases: [(&str, &[&str], &str, &[f64]); 6] = [
        ("J01", &["Jv01"], "pf_after", &[4., 9.]),
        ("J02", &["Jh02"], "pf_after", &[9., 9.]),
        ("J03", &["Jg03", "Jw03"], "pf_after", &[9., 9., 9.]),
        ("J04", &["Jh04"], "pf_after_valid", &[0., 0.]),
        ("J05", &["Jv05"], "pf_after", &[4., 12., 9.]),
        ("J06", &["Jb06", "Jd06"], "pf_dispatch", &[4., 8.]),
    ];
    for (id, modules, name, expected) in cases {
        let mut a = Actor::new();
        let dir = base.join(id);
        let text = std::fs::read_to_string(dir.join(format!("{id}.m"))).unwrap();
        let paths: Vec<_> = modules.iter().map(|m| dir.join(format!("{m}.m"))).collect();
        let f = a.run(&text, &paths);
        if id == "J04" {
            assert_eq!(f.status, Status::Error);
            assert!(f.error.as_ref().unwrap().message.contains("InvalidHandle"));
        } else {
            success(&f)
        }
        let (p, _lease) = a.page(&token(&f, name), 0, 0, &[], 30, 12);
        assert_eq!(numbers(&p), expected, "{id}");
    }
}

#[test]
fn actor_pages_complex_nd_and_csc_without_losing_storage_or_implicit_zeros() {
    let mut a = Actor::new();
    let f = a.run(
        "z=reshape(complex(1:8,11:18),[2,2,2]);s=sparse([1,3],[1,2],[7,9],3,2);",
        &[],
    );
    success(&f);
    let (p, lease) = a.page(&token(&f, "z"), 0, 0, &[1], 30, 12);
    assert_eq!(p.dimensions, [2, 2, 2]);
    assert!(p.complex);
    assert!(p.cells[0][0].text.contains("5.0000000000000000e0"));
    assert!(p.cells[0][0].text.contains("1.5000000000000000e1"));
    drop((p, lease));
    let (p, _lease) = a.page(&token(&f, "s"), 0, 0, &[], 30, 12);
    assert_eq!(p.storage, "csc");
    assert!(p.cells[0][0].stored);
    assert!(!p.cells[0][1].stored);
    assert!(p.cells[2][1].stored);
    assert_eq!(numbers(&p), [7., 0., 0., 0., 0., 9.]);
}

#[test]
fn actor_stop_before_enqueue_and_during_execution_preserves_completed_prefix() {
    let mut a = Actor::new();
    let r = a.request("unreached=1;", &[]);
    r.stop.request();
    a.tx.send(Request::NativeRun(r)).unwrap();
    let f = a.finish();
    assert_eq!(f.status, Status::Cancelled);
    assert!(!f.execution_started);
    assert!(f.workspace.is_empty());
    drop(f);
    let r = a.request("seed=19;while true\ncount=1;\nend", &[]);
    let stop = r.stop.clone();
    a.tx.send(Request::NativeRun(r)).unwrap();
    match a.next() {
        NativeEvent::Prepared { cancel, .. } => {
            stop.request();
            cancel.request_stop();
        }
        e => panic!("{e:?}"),
    };
    let f = a.finish();
    assert_eq!(f.status, Status::Cancelled);
    assert!(f.execution_started);
    assert!(f.reason.is_some());
    drop(f);
    let f = a.run("recovered=23;", &[]);
    success(&f);
}

#[test]
fn actor_old_stop_and_page_tokens_cannot_target_new_job_or_reset() {
    let mut a = Actor::new();
    let r = a.request("x=4;", &[]);
    let old_stop = r.stop.clone();
    a.tx.send(Request::NativeRun(r)).unwrap();
    let f = a.finish();
    success(&f);
    let stale = token(&f, "x");
    drop(f);
    old_stop.request();
    let f = a.run("x=9;", &[]);
    success(&f);
    let r = PageRequest::new(&a.gate, 42, &stale, 0, 0, &[], 1, 1).unwrap();
    a.tx.send(Request::NativeRead(r)).unwrap();
    match a.next() {
        NativeEvent::Page {
            token,
            result: Err(_),
            ..
        } => assert_eq!(token, stale),
        e => panic!("{e:?}"),
    };
    drop(f);
    a.reset();
    let f = a.run("x=12;", &[]);
    success(&f);
    assert_ne!(f.key.generation, stale.key.generation);
}

#[test]
fn actor_changed_definitions_refuse_until_reset_and_captured_module_bytes_do_not_change() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("DeskGauge.m");
    std::fs::write(
        &path,
        "classdef DeskGauge < handle\nproperties\nReading=4\nend\nend\n",
    )
    .unwrap();
    let mut a = Actor::new();
    let f = a.run("h=DeskGauge;", &[path.clone()]);
    success(&f);
    let captured = f.sources[1].text.clone();
    drop(f);
    std::fs::write(
        &path,
        "classdef DeskGauge < handle\nproperties\nReading=12\nend\nend\n",
    )
    .unwrap();
    assert!(captured.contains("Reading=4"));
    let f = a.run("bad=DeskGauge;", &[path.clone()]);
    assert_eq!(f.status, Status::Error);
    assert!(!f.workspace.iter().any(|v| v.name == "bad"));
    drop(f);
    a.reset();
    let f = a.run("h=DeskGauge;x=h.Reading;", &[path]);
    success(&f);
    let (p, _lease) = a.page(&token(&f, "x"), 0, 0, &[], 1, 1);
    assert_eq!(numbers(&p), [12.]);
}

#[test]
fn actor_failed_shared_handle_mutation_has_real_object_metadata_and_prefix() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("DeskGauge.m");
    std::fs::write(
        &path,
        "classdef DeskGauge < handle\nproperties\nReading=4\nend\nend\n",
    )
    .unwrap();
    let mut a = Actor::new();
    let f = a.run(
        "h=DeskGauge;alias=h;h.Reading=9;h.Missing=1;unreached=1;",
        &[path.clone()],
    );
    assert_eq!(f.status, Status::Error);
    assert!(!f.workspace.iter().any(|v| v.name == "unreached"));
    let (p, lease) = a.page(&token(&f, "h"), 0, 0, &[], 1, 1);
    assert!(p.object.as_ref().unwrap().contains("valid=true"));
    assert!(
        !f.workspace
            .iter()
            .find(|v| v.name == "h")
            .unwrap()
            .dimensions_known
    );
    drop((p, lease, f));
    let f = a.run("observed=alias.Reading;", &[path]);
    success(&f);
    let (p, _lease) = a.page(&token(&f, "observed"), 0, 0, &[], 1, 1);
    assert_eq!(numbers(&p), [9.]);
}

#[test]
fn native_queue_credit_lives_through_queued_response_cache_and_history_drop() {
    let mut a = Actor::new();
    let f = a.run("x=1;", &[]);
    success(&f);
    assert!(
        RunRequest::new(
            &a.gate,
            JobKey {
                generation: 1,
                job: 99
            },
            1,
            Path::new("/private/tmp/other.m"),
            "x=2;",
            &[]
        )
        .is_err()
    );
    assert_eq!(a.gate.reserved_bytes(), 2 * MAX_RESPONSE_BYTES);
    let history = a.gate.reserve_display(4 << 20).unwrap();
    assert!(a.gate.reserve_display(1).is_err());
    let (p, read_lease) = a.page(&token(&f, "x"), 0, 0, &[], 1, 1);
    assert_eq!(a.gate.reserved_bytes(), 8 << 20);
    assert!(PageRequest::new(&a.gate, 2, &token(&f, "x"), 0, 0, &[], 1, 1).is_err());
    drop((p, read_lease, f));
    assert_eq!(a.gate.reserved_bytes(), 4 << 20);
    drop(history);
    assert_eq!(a.gate.reserved_bytes(), 0);
}

#[test]
fn page_bounds_reject_zero_product_huge_extents_and_preserve_empty_shape() {
    let gate = NativeGate::default();
    let t = PageToken {
        key: JobKey {
            generation: 1,
            job: 1,
        },
        revision: 0,
        binding: "empty".into(),
    };
    assert!(PageRequest::new(&gate, 1, &t, 0, 0, &[], usize::MAX, 0).is_err());
    assert!(PageRequest::new(&gate, 1, &t, 0, 0, &[], 0, usize::MAX).is_err());
    assert_eq!(gate.reserved_bytes(), 0);
    let mut a = Actor::new();
    let f = a.run("empty=zeros(1000000,0);", &[]);
    success(&f);
    let (p, _lease) = a.page(&token(&f, "empty"), 0, 0, &[], 512, 0);
    assert_eq!(p.dimensions, [1000000, 0]);
    assert_eq!((p.rows, p.columns), (0, 0));
    assert!(p.cells.is_empty());
}

#[test]
fn actor_source_caps_utf8_regular_file_and_preview_omission_are_explicit() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("module.m");
    std::fs::write(&path, [0xff, 0xfe]).unwrap();
    let mut a = Actor::new();
    let f = a.run("x=1;", &[path.clone()]);
    assert_eq!(f.status, Status::Error);
    assert!(!f.execution_started);
    assert!(f.graph_sha256.is_none());
    drop(f);
    std::fs::write(&path, vec![b' '; MAX_SOURCE_BYTES + 1]).unwrap();
    let f = a.run("x=1;", &[path]);
    assert!(!f.execution_started);
    drop(f);
    let f = a.run("x=1;", &[dir.path().to_owned()]);
    assert!(!f.execution_started);
    drop(f);
    assert!(
        RunRequest::new(
            &a.gate,
            JobKey {
                generation: 1,
                job: 99
            },
            1,
            Path::new("/private/tmp/x.m"),
            &" ".repeat(MAX_SOURCE_BYTES + 1),
            &[]
        )
        .is_err()
    );
    let f = a.run("for k=1:300\nk\nend", &[]);
    success(&f);
    assert!(f.event_count > MAX_EVENT_PREVIEWS);
    assert!(!f.events_complete);
    assert!(f.workspace_complete);
    assert!(f.owned_bytes() <= MAX_RESPONSE_BYTES);
}

#[test]
fn actor_switches_both_real_cube_planes_and_rejects_out_of_range_and_stale_plane_tokens(){
    let mut a=Actor::new();let f=a.run("cube=reshape(1:8,[2,2,2]);",&[]);success(&f);let old=token(&f,"cube");
    let(p,lease)=a.page(&old,0,0,&[0],30,12);assert_eq!(numbers(&p),[1.,3.,2.,4.]);assert_eq!(p.higher,[0]);drop((p,lease));
    let(p,lease)=a.page(&old,0,0,&[1],30,12);assert_eq!(numbers(&p),[5.,7.,6.,8.]);assert_eq!(p.higher,[1]);drop((p,lease));
    let request=PageRequest::new(&a.gate,3,&old,0,0,&[2],30,12).unwrap();a.tx.send(Request::NativeRead(request)).unwrap();match a.next(){NativeEvent::Page{token,result:Err(_),..}=>assert_eq!(token,old),e=>panic!("{e:?}")};
    drop(f);let f=a.run("cube=reshape(11:18,[2,2,2]);",&[]);success(&f);let request=PageRequest::new(&a.gate,4,&old,0,0,&[1],30,12).unwrap();a.tx.send(Request::NativeRead(request)).unwrap();match a.next(){NativeEvent::Page{token,result:Err(_),..}=>assert_eq!(token,old),e=>panic!("{e:?}")};
    let(p,_lease)=a.page(&token(&f,"cube"),0,0,&[1],30,12);assert_eq!(numbers(&p),[15.,17.,16.,18.]);
}
