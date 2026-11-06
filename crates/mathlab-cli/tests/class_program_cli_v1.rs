// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(unix)]
use std::{fs::{self,File},os::unix::fs::{PermissionsExt,symlink},path::PathBuf,process::{Child,Command,Stdio},thread,time::{Duration,Instant}};
use serde_json::{Value,json};
use sha2::{Digest,Sha256};
use tempfile::TempDir;
struct Owned{child:Child,out:PathBuf,err:PathBuf}
impl Drop for Owned{fn drop(&mut self){if !matches!(self.child.try_wait(),Ok(Some(_))){let _=self.child.kill();}let _=self.child.wait();if thread::panicking(){for(label,path)in[("owned_stdout",&self.out),("owned_stderr",&self.err)]{let mut bytes=[0u8;4096];if let Ok(mut file)=File::open(path){if let Ok(count)=std::io::Read::read(&mut file,&mut bytes){let _=std::io::Write::write_fmt(&mut std::io::stderr().lock(),format_args!("{label}_prefix_max4096={:?}\n",String::from_utf8_lossy(&bytes[..count])));}}}}}}
impl Owned{
 fn bounded(&self){for p in [&self.out,&self.err]{assert!(fs::metadata(p).unwrap().len()<=1024*1024);}}
 fn ready(&mut self)->Value{let until=Instant::now()+Duration::from_secs(2);loop{self.bounded();let raw=fs::read(&self.out).unwrap();if let Some(i)=raw.iter().position(|b|*b==b'\n'){let row:Value=serde_json::from_slice(&raw[..i]).unwrap();assert_eq!(row["event"],"ready");assert_eq!(row["schema_version"],4);return row}if let Some(status)=self.child.try_wait().unwrap(){panic!("owned child exited before ready: {status}")}assert!(Instant::now()<until);thread::sleep(Duration::from_millis(1));}}
 fn finish(self)->(i32,Vec<Value>){self.finish_bound(Duration::from_secs(3))}
 fn finish_bound(mut self,bound:Duration)->(i32,Vec<Value>){let until=Instant::now()+bound;loop{self.bounded();if let Some(status)=self.child.try_wait().unwrap(){let out=fs::read_to_string(&self.out).unwrap();let err=fs::read_to_string(&self.err).unwrap();assert!(err.is_empty(),"{err}");return(status.code().unwrap(),out.lines().map(|s|serde_json::from_str(s).unwrap()).collect())}assert!(Instant::now()<until,"owned child deadline; no retry");thread::sleep(Duration::from_millis(1));}}
}
struct Case{dir:TempDir,serial:usize}
impl Case{
 fn new()->Self{let dir=tempfile::tempdir().unwrap();let p=dir.path().join("octave-cli");fs::write(&p,"#!/bin/sh\ntouch native-called\nexit 91\n").unwrap();fs::set_permissions(p,fs::Permissions::from_mode(0o755)).unwrap();Self{dir,serial:0}}
 fn entry(&self)->PathBuf{self.dir.path().join("entry.m")}
 fn write(&self,entry:&str,modules:&[(&str,&str)])->Vec<PathBuf>{fs::write(self.entry(),entry).unwrap();modules.iter().map(|(name,text)|{let p=self.dir.path().join(name);fs::write(&p,text).unwrap();p}).collect()}
 fn spawn(&mut self,entry:&str,modules:&[(&str,&str)],extra:&[&str])->Owned{let paths=self.write(entry,modules);self.start(&paths,extra)}
 fn start(&mut self,modules:&[PathBuf],extra:&[&str])->Owned{self.start_probe(modules,extra,None)}
 fn start_probe(&mut self,modules:&[PathBuf],extra:&[&str],probe:Option<&std::path::Path>)->Owned{self.serial+=1;let out=self.dir.path().join(format!("out{}",self.serial));let err=self.dir.path().join(format!("err{}",self.serial));let mut cmd=Command::new(env!("CARGO_BIN_EXE_mathlab"));cmd.env_clear();cmd.args(["--json","run","--rust-program","--profile","class-v1","--workspace"]);for p in modules{cmd.arg("--module-file").arg(p);}if let Some(path)=probe{cmd.env("ONL_QR_PHASE_SOCKET",path);}let child=cmd.args(extra).arg(self.entry()).env("PATH",self.dir.path()).env("MATHLAB_OCTAVE",self.dir.path().join("octave-cli")).env("TMPDIR",self.dir.path()).current_dir(self.dir.path()).stdin(Stdio::null()).stdout(File::create(&out).unwrap()).stderr(File::create(&err).unwrap()).spawn().unwrap();Owned{child,out,err}}
 fn no_native(&self){assert!(!self.dir.path().join("native-called").exists());}
}
fn binding<'a>(out:&'a Value,name:&str)->&'a Value{&out["workspace"].as_array().unwrap().iter().find(|e|e["name"]==name).unwrap()["value"]}
fn absent(out:&Value,name:&str){assert!(!out["workspace"].as_array().unwrap().iter().any(|e|e["name"]==name));}
fn words(v:&Value)->Vec<f64>{v["binary64_components"].as_array().unwrap().iter().map(|w|f64::from_bits(u64::from_str_radix(w[0].as_str().unwrap(),16).unwrap())).collect()}
fn resources(out:&Value){let s=&out["stats"];assert_eq!(s["ledger_poisoned"],false);for k in ["peak_live_payload_bytes","heap_payload_bytes","peak_operation_reserved_bytes"]{assert!(s[k].as_u64().unwrap()<=64<<20);}for k in ["peak_metadata_bytes","heap_metadata_bytes"]{assert!(s[k].as_u64().unwrap()<=16<<20);}assert!(s["primitive_steps"].as_u64().unwrap()<=1_000_000_000);assert!(s["transitions"].as_u64().unwrap()<=5_000_000);}
fn success(rows:&[Value]){assert_eq!(rows.len(),2);for(e,r)in ["ready","outcome"].into_iter().zip(rows){assert_eq!(r["event"],e);assert_eq!(r["schema_version"],4);assert_eq!(r["profile"],"class-v1");assert_eq!(r["backend"],"rust");assert_eq!(r["kind"],"mathlab-rust-program");assert_eq!(r["source_graph_complete"],true);}assert_eq!(rows[0]["source_graph_sha256"],rows[1]["source_graph_sha256"]);assert_eq!(rows[1]["status"],"success");assert_eq!(rows[1]["command_exit"],0);assert_eq!(rows[1]["events_complete"],true);assert!(rows[1]["error"].is_null());resources(&rows[1]);}
fn hex(b:&[u8])->String{b.iter().map(|v|format!("{v:02x}")).collect()}
fn graph_hash(sources:&Value)->String{let mut h=Sha256::new();for s in sources.as_array().unwrap(){let path=s["path_display"].as_str().unwrap().as_bytes();h.update((path.len() as u64).to_le_bytes());h.update(path);h.update(s["id"].as_u64().unwrap().to_le_bytes());h.update(s["bytes"].as_u64().unwrap().to_le_bytes());let d=s["sha256"].as_str().unwrap();let b:Vec<u8>=(0..64).step_by(2).map(|i|u8::from_str_radix(&d[i..i+2],16).unwrap()).collect();h.update(b);}hex(&h.finalize())}
fn object(v:&Value,kind:&str,valid:bool){assert_eq!(v["storage"],"object");assert_eq!(v["object_kind"],kind);assert_eq!(v["valid"],valid);assert!(v["identity"]["slot"].is_u64());assert!(v["identity"]["generation"].is_u64());for k in ["shape","class","binary64_components","logical_numel","properties"]{assert!(v.get(k).is_none());}}
const J01:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-foundation-source-reference/fixtures/J01/J01.m");
const JV01:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-foundation-source-reference/fixtures/J01/Jv01.m");
const J02:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-foundation-source-reference/fixtures/J02/J02.m");
const JH02:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-foundation-source-reference/fixtures/J02/Jh02.m");
const J03:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-foundation-source-reference/fixtures/J03/J03.m");
const JG03:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-foundation-source-reference/fixtures/J03/Jg03.m");
const JW03:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-foundation-source-reference/fixtures/J03/Jw03.m");
const J04:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-foundation-source-reference/fixtures/J04/J04.m");
const JH04:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-foundation-source-reference/fixtures/J04/Jh04.m");
const J05:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-foundation-source-reference/fixtures/J05/J05.m");
const JV05:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-foundation-source-reference/fixtures/J05/Jv05.m");
const J06:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-foundation-source-reference/fixtures/J06/J06.m");
const JB06:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-foundation-source-reference/fixtures/J06/Jb06.m");
const JD06:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-foundation-source-reference/fixtures/J06/Jd06.m");
const K01:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-call-source-reference/fixtures/K01/K01.m");
const KC01:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-call-source-reference/fixtures/K01/Kc01.m");
const K02:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-call-source-reference/fixtures/K02/K02.m");
const KM02:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-call-source-reference/fixtures/K02/Km02.m");
const K03:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-call-source-reference/fixtures/K03/K03.m");
const KS03:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-call-source-reference/fixtures/K03/Ks03.m");
const K04:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-call-source-reference/fixtures/K04/K04.m");
const KG04:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-call-source-reference/fixtures/K04/Kg04.m");
const KW04:&str=include_str!("../../../docs/research/matlab-r2026b/probes/rust-class-call-source-reference/fixtures/K04/Kw04.m");

#[test]
fn class_cli_all_six_complete_native_foundation_graphs_and_invalid_handle_prefix(){
 for(entry,modules,expected)in [(J01,vec![("Jv01.m",JV01)],vec![4.,9.]),(J02,vec![("Jh02.m",JH02)],vec![9.,9.]),(J03,vec![("Jg03.m",JG03),("Jw03.m",JW03)],vec![9.,9.,9.]),(J05,vec![("Jv05.m",JV05)],vec![4.,12.,9.]),(J06,vec![("Jb06.m",JB06),("Jd06.m",JD06)],vec![4.,8.])]{let mut c=Case::new();let(code,rows)=c.spawn(entry,&modules,&[]).finish();assert_eq!(code,0);success(&rows);let out=&rows[1];assert_eq!(words(binding(out,if entry==J06{"pf_dispatch"}else{"pf_after"})),expected);assert_eq!(binding(out,"pf_done")["logical_values"],json!([true]));assert_eq!(out["source_graph_sha256"],graph_hash(&out["source_graph"]));if entry==J01{object(binding(out,"a"),"value",true);object(binding(out,"b"),"value",true);assert_ne!(binding(out,"a")["identity"],binding(out,"b")["identity"]);}if entry==J02{object(binding(out,"a"),"handle",true);assert_eq!(binding(out,"a")["identity"],binding(out,"b")["identity"]);}c.no_native();}
 let mut c=Case::new();let(code,rows)=c.spawn(J04,&[("Jh04.m",JH04)],&[]).finish();assert_eq!(code,1);let out=&rows[1];assert_eq!(out["error"]["code"],"InvalidHandle");assert_eq!(out["error"]["location"]["source_id"],1);assert_eq!(out["error"]["location"]["span"]["start_line"],7);assert_eq!(binding(out,"pf_after_valid")["logical_values"],json!([false,false]));assert_eq!(binding(out,"pf_mutation_reached")["logical_values"],json!([true]));assert_eq!(binding(out,"pf_done")["logical_values"],json!([false]));absent(out,"pf_mutation_completed");object(binding(out,"a"),"handle",false);object(binding(out,"b"),"handle",false);resources(out);c.no_native();
}

#[test]
fn class_cli_renamed_value_handle_and_inheritance_derivatives(){
 for(entry,modules,expected)in [(J01,vec![("Jv01.m",JV01)],vec![4.,9.]),(J02,vec![("Jh02.m",JH02)],vec![9.,9.]),(J06,vec![("Jb06.m",JB06),("Jd06.m",JD06)],vec![4.,8.])]{let mut c=Case::new();let rename=|s:&str|s.replace("Jv01","ValueProbe").replace("Jh02","HandleProbe").replace("Jb06","ParentProbe").replace("Jd06","ChildProbe");let named:Vec<(String,String)>=modules.iter().map(|(p,s)|(rename(p),rename(s))).collect();let borrowed:Vec<(&str,&str)>=named.iter().map(|(p,s)|(p.as_str(),s.as_str())).collect();let(code,rows)=c.spawn(&rename(entry),&borrowed,&[]).finish();assert_eq!(code,0);success(&rows);assert_eq!(words(binding(&rows[1],if entry==J06{"pf_dispatch"}else{"pf_after"})),expected);c.no_native();}
}

#[test]
fn class_cli_native_constructor_method_counts_and_local_class_shadow(){
 for(entry,module,text)in [(K01,"Kc01.m",KC01),(K02,"Km02.m",KM02)]{let mut c=Case::new();let(code,rows)=c.spawn(entry,&[(module,text)],&[]).finish();assert_eq!(code,0);success(&rows);let out=&rows[1];if entry==K01{assert_eq!(words(binding(out,"pf_counts")),[0.,1.,1.,1.]);assert_eq!(words(binding(out,"pf_bare")),[1.,0.,11.]);}else{assert_eq!(words(binding(out,"pf_two")),[4.,9.,2.,2.]);assert_eq!(words(binding(out,"pf_one")),[4.,12.,2.,1.]);assert_eq!(words(binding(out,"pf_bare")),[17.,2.,0.]);}c.no_native();}
 let mut c=Case::new();let derivative="a=Ks03(3);pf_constructed=a.Reading;pf_reading=localShadowNumericK03();\nfunction value=localShadowNumericK03()\nKs03=[7 8];value=Ks03(2);\nend\n";let(code,rows)=c.spawn(derivative,&[("Ks03.m",KS03)],&[]).finish();assert_eq!(code,0);success(&rows);assert_eq!(words(binding(&rows[1],"pf_reading")),[8.]);
 // Original class(selected) returns char, outside the numeric/object value API.
 let(code,rows)=c.spawn(K03,&[("Ks03.m",KS03)],&[]).finish();assert_eq!(code,1);assert!(matches!(rows.last().unwrap()["error"]["code"].as_str(),Some("Unsupported"|"UnsupportedFeature")));c.no_native();

}

#[test]
fn class_cli_shared_handle_default_derivative_and_unsupported_persistent_original(){
 let gauge="classdef DefaultGauge < handle\nproperties\nReading=4\nend\nend\n";let wrapper="classdef DefaultWrapper\nproperties\nGauge=DefaultGauge\nBuilt\nend\nmethods\nfunction obj=DefaultWrapper()\nobj.Built=DefaultGauge;\nend\nend\nend\n";let entry="a=DefaultWrapper;b=DefaultWrapper;a.Gauge.Reading=9;a.Built.Reading=12;pf=[a.Gauge.Reading b.Gauge.Reading a.Built.Reading b.Built.Reading];g=a.Gauge;h=b.Gauge;u=a.Built;v=b.Built;";let mut c=Case::new();let(code,rows)=c.spawn(entry,&[("DefaultGauge.m",gauge),("DefaultWrapper.m",wrapper)],&[]).finish();assert_eq!(code,0);success(&rows);assert_eq!(words(binding(&rows[1],"pf")),[9.,9.,12.,4.]);assert_eq!(binding(&rows[1],"g")["identity"],binding(&rows[1],"h")["identity"]);assert_ne!(binding(&rows[1],"u")["identity"],binding(&rows[1],"v")["identity"]);
 let(code,rows)=c.spawn(K04,&[("Kg04.m",KG04),("Kw04.m",KW04)],&[]).finish();assert_eq!(code,1);let out=rows.last().unwrap();assert_eq!(out["phase"],"parse");assert_eq!(out["error"]["code"],"Unsupported");assert_eq!(out["effect_sequence"],0);assert!(out["key"].is_null());c.no_native();
}

#[test]
fn class_cli_non_scalar_dense_csc_property_copy_and_numeric_dispatch(){
 let module="classdef NumericBox\nproperties\nDense\nSparse\nend\nmethods\nfunction obj=NumericBox()\nobj.Dense=[1 2];obj.Sparse=sparse([1 2],[1 2],[2 3],2,2);\nend\nfunction obj=changed(obj)\nobj.Dense=[9 8];obj.Sparse=sparse(1,1,7,2,2);\nend\nend\nend\n";let mut c=Case::new();let(code,rows)=c.spawn("a=NumericBox;b=a;b=b.changed();pf_a=a.Dense;pf_b=b.Dense;pf_product=a.Sparse*[1;1];stored=a.Sparse;",&[("NumericBox.m",module)],&[]).finish();assert_eq!(code,0);success(&rows);assert_eq!(words(binding(&rows[1],"pf_a")),[1.,2.]);assert_eq!(words(binding(&rows[1],"pf_b")),[9.,8.]);assert_eq!(words(binding(&rows[1],"pf_product")),[2.,3.]);assert_eq!(binding(&rows[1],"stored")["storage"],"csc");assert_eq!(binding(&rows[1],"stored")["nnz"],2);c.no_native();
}

#[test]
fn class_cli_object_event_snapshot_remains_valid_after_live_alias_delete(){
 let mut c=Case::new();let module=JH02.replace("Jh02","EventGauge");let(code,rows)=c.spawn("h=EventGauge;alias=h;h\ndelete(h);pf=isvalid(alias);",&[("EventGauge.m",&module)],&[]).finish();assert_eq!(code,0);success(&rows);let out=&rows[1];assert_eq!(binding(out,"pf")["logical_values"],json!([false]));object(binding(out,"h"),"handle",false);let event=out["events"].as_array().unwrap().iter().find(|e|e["value"]["storage"]=="object").unwrap();object(&event["value"],"handle",true);assert_eq!(event["value"]["class_name"],"EventGauge");assert_eq!(event["value"]["identity"],binding(out,"h")["identity"]);assert_eq!(out["stats"]["handle_deletions"],1);c.no_native();
}

#[test]
fn class_cli_failing_constructor_preserves_shared_effect_and_located_prefix(){
 let gauge=JH02.replace("Jh02","PublishedGauge");let bad="classdef FailedValue\nproperties\nMarker=4\nend\nmethods\nfunction obj=FailedValue(h)\nh.Reading=9;\nobj.Missing=1;\nend\nend\nend\n";let mut c=Case::new();let(code,rows)=c.spawn("h=PublishedGauge;pf_before=h.Reading;result=FailedValue(h);pf_after=h.Reading;",&[("PublishedGauge.m",&gauge),("FailedValue.m",bad)],&[]).finish();assert_eq!(code,1);let out=&rows[1];assert_eq!(out["error"]["code"],"UnknownProperty");assert_eq!(out["error"]["location"]["source_id"],3);assert_eq!(out["error"]["location"]["span"]["start_line"],8);assert_eq!(out["stats"]["handle_mutations"],1);assert_eq!(out["effect_sequence"],3);assert_eq!(out["after"]["revision"],3);assert_eq!(words(binding(out,"pf_before")),[4.]);object(binding(out,"h"),"handle",true);absent(out,"result");absent(out,"pf_after");resources(out);c.no_native();
}

#[test]
fn class_cli_explicit_graph_identity_duplicates_bases_cycles_and_no_ambient_lookup(){
 let mut c=Case::new();let module=JV01.replace("Jv01","ClosedValue");let(code,rows)=c.spawn("a=ClosedValue;",&[("ClosedValue.m",&module)],&[]).finish();assert_eq!(code,0);success(&rows);let first=rows[1]["source_graph_sha256"].clone();let changed=module.replace("Reading = 4","Reading = 6");let(code,rows)=c.spawn("a=ClosedValue;",&[("ClosedValue.m",&changed)],&[]).finish();assert_eq!(code,0);assert_ne!(rows[1]["source_graph_sha256"],first);assert_eq!(rows[1]["source_graph_sha256"],graph_hash(&rows[1]["source_graph"]));
 let paths=c.write("prior=7;a=ClosedValue;",&[("ClosedValue.m",&module)]);let(code,rows)=c.start(&[paths[0].clone(),paths[0].clone()],&[]).finish();assert_eq!(code,1);assert_eq!(rows.last().unwrap()["error"]["code"],"DuplicateSource");assert_eq!(rows.last().unwrap()["effect_sequence"],0);
 let(code,rows)=c.start(&[c.dir.path().join("missing.m")],&[]).finish();assert_eq!(code,1);assert_eq!(rows.last().unwrap()["error"]["code"],"SourceIo");assert_eq!(rows.last().unwrap()["source_graph_complete"],false);
 let(code,rows)=c.spawn("prior=7;a=ClosedValue;",&[("A.m",&module),("B.m",&module)],&[]).finish();assert_eq!(code,1);assert_eq!(rows.last().unwrap()["error"]["code"],"DuplicateName");assert_eq!(rows.last().unwrap()["effect_sequence"],0);
 for(modules,expected)in [(vec![("Child.m","classdef Child < Absent\nend\n")],"UnknownBase"),(vec![("Child.m","classdef Child < Parent\nend\n"),("Parent.m","classdef Parent < Child\nend\n")],"InheritanceCycle")]{let(code,rows)=c.spawn("prior=7;",&modules,&[]).finish();assert_eq!(code,1);let out=rows.last().unwrap();assert_eq!(out["error"]["code"],expected);assert_eq!(out["effect_sequence"],0);assert!(out["key"].is_null());}
 c.write("a=ClosedValue;",&[("ClosedValue.m",&module)]);let(code,rows)=c.start(&[],&[]).finish();assert_eq!(code,1);assert_ne!(rows.last().unwrap()["status"],"success");assert_eq!(rows.last().unwrap()["source_graph"].as_array().unwrap().len(),1);c.no_native();
}

#[test]
fn class_cli_utf8_regular_nonblocking_source_limits_and_shared_symlink_policy(){
 let mut c=Case::new();c.write("a=Broken;",&[]);let bad=c.dir.path().join("Broken.m");fs::write(&bad,[0xffu8]).unwrap();let(code,rows)=c.start(&[bad.clone()],&[]).finish();assert_eq!(code,1);let out=rows.last().unwrap();assert_eq!(out["error"]["code"],"InvalidUtf8");assert_eq!(out["error"]["location"]["source_id"],2);assert_eq!(out["source_graph_complete"],false);fs::write(&bad,vec![b' ';65537]).unwrap();let(code,rows)=c.start(&[bad],&[]).finish();assert_eq!(code,1);assert_eq!(rows.last().unwrap()["error"]["code"],"SourceLimit");
 let names:Vec<String>=(0..9).map(|i|format!("Unused{i}.m")).collect();let modules:Vec<(&str,&str)>=names.iter().map(|name|(name.as_str(),"classdef Unused\nend\n")).collect();let paths=c.write("prior=7;",&modules);let(code,rows)=c.start(&paths,&[]).finish();assert_eq!(code,1);assert_eq!(rows.last().unwrap()["error"]["code"],"SourceGraphLimit");
 let texts:Vec<String>=(0..4).map(|i|{let mut s=format!("classdef Limit{i}\nend\n");let padding=65536-s.len();s.extend(std::iter::repeat_n(' ',padding));s}).collect();let names:Vec<String>=(0..4).map(|i|format!("Limit{i}.m")).collect();let modules:Vec<(&str,&str)>=names.iter().zip(&texts).map(|(a,b)|(a.as_str(),b.as_str())).collect();let(code,rows)=c.spawn("prior=7;",&modules,&[]).finish();assert_eq!(code,1);assert_eq!(rows.last().unwrap()["error"]["code"],"SourceGraphLimit");
 let fifo=c.dir.path().join("pipe.m");let path=std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();assert_eq!(unsafe{libc::mkfifo(path.as_ptr(),0o600)},0);let(code,rows)=c.start(&[fifo],&[]).finish();assert_eq!(code,1);assert_eq!(rows.last().unwrap()["error"]["code"],"NotRegularFile");
 let module=JV01.replace("Jv01","LinkedValue");let paths=c.write("a=LinkedValue;pf=a.Reading;",&[("LinkedValue.m",&module)]);let link=c.dir.path().join("alias.m");symlink(&paths[0],&link).unwrap();let(code,rows)=c.start(&[link],&[]).finish();assert_eq!(code,0);success(&rows);assert_eq!(words(binding(&rows[1],"pf")),[4.]);c.no_native();
}

#[test]
fn class_cli_object_export_refusal_numeric_wires_no_clobber_and_missing_parent(){
 let mut c=Case::new();let module=JV01.replace("Jv01","ExportValue");let path=c.dir.path().join("object.export");let(code,rows)=c.spawn("a=ExportValue;pf=a.Reading;",&[("ExportValue.m",&module)],&["--export-binding","a","--export-file",path.to_str().unwrap()]).finish();assert_eq!(code,1);let out=&rows[1];assert_eq!(out["status"],"success");assert_eq!(out["export"]["code"],"UnsupportedObjectExport");assert_eq!(out["export"]["complete"],false);assert!(!path.exists());assert_eq!(words(binding(out,"pf")),[4.]);object(binding(out,"a"),"value",true);
 for(source,name,format,payload)in [("X=complex([1 2],[3 4]);","X","onl-scientific-array-v1",32usize),("A=sparse(1,1,2,2,2);","A","onl-scientific-csc-v1",24usize)]{let path=c.dir.path().join(name);let(code,rows)=c.spawn(source,&[],&["--export-binding",name,"--export-file",path.to_str().unwrap()]).finish();assert_eq!(code,0);success(&rows);let bytes=fs::read(path).unwrap();let end=bytes.iter().position(|b|*b==b'\n').unwrap();let header:Value=serde_json::from_slice(&bytes[..end]).unwrap();assert_eq!(header["format"],format);let body=&bytes[end+1..end+1+payload];assert_eq!(bytes[end+1+payload],b'\n');let footer:Value=serde_json::from_slice(&bytes[end+2+payload..]).unwrap();assert_eq!(footer["complete"],true);assert_eq!(footer["payload_bytes"],payload);assert_eq!(footer["sha256"],hex(&Sha256::digest(body)));assert_eq!(rows[1]["export"]["sha256"],footer["sha256"]);}
 let existing=c.dir.path().join("keep");fs::write(&existing,b"KEEP").unwrap();let(code,rows)=c.spawn("X=[1 2];",&[],&["--export-binding","X","--export-file",existing.to_str().unwrap()]).finish();assert_eq!(code,1);assert_eq!(rows[1]["export"]["code"],"DestinationExists");assert_eq!(fs::read(&existing).unwrap(),b"KEEP");let missing=c.dir.path().join("missing/X");let(code,rows)=c.spawn("X=[1 2];",&[],&["--export-binding","X","--export-file",missing.to_str().unwrap()]).finish();assert_eq!(code,1);assert_eq!(rows[1]["export"]["code"],"ExportIo");assert_eq!(rows[1]["status"],"success");assert!(!missing.parent().unwrap().exists());c.no_native();
}

#[test]
fn class_cli_owned_stop_deadline_fuels_and_immutable_module_after_ready(){
 let mut c=Case::new();let module=JH02.replace("Jh02","LiveGauge");let source="h=LiveGauge;pf_initial=h.Reading;while 1,h.Reading=h.Reading+1;end";let mut child=c.spawn(source,&[("LiveGauge.m",&module)],&[]);let ready=child.ready();fs::write(c.dir.path().join("LiveGauge.m"),module.replace("Reading = 4","Reading = 99")).unwrap();thread::sleep(Duration::from_millis(20));assert_eq!(unsafe{libc::kill(child.child.id() as i32,libc::SIGINT)},0);let(code,rows)=child.finish();assert_eq!(code,130);let out=&rows[1];assert_eq!(out["reason"],"UserStop");assert_eq!(out["source_graph_sha256"],ready["source_graph_sha256"]);assert_eq!(words(binding(out,"pf_initial")),[4.]);object(binding(out,"h"),"handle",true);resources(out);
 let(code,rows)=c.spawn(source,&[("LiveGauge.m",&module)],&["--timeout","1"]).finish();assert_eq!(code,124);assert_eq!(rows[1]["reason"],"Deadline");resources(&rows[1]);
 for option in ["--scientific-transition-fuel","--scientific-primitive-fuel"]{let(code,rows)=c.spawn("a=LiveGauge;",&[("LiveGauge.m",&module)],&[option,"0"]).finish();assert_eq!(code,1);let out=rows.last().unwrap();assert_eq!(out["status"],"resource");assert_eq!(out["effect_sequence"],0);assert!(out["workspace"].as_array().unwrap().is_empty());resources(out);}c.no_native();
}


#[test]
fn class_cli_human_numeric_windows_and_object_metadata(){
 let mut c=Case::new();let modules=c.write("x=1:300;x\nh=HumanGauge;h\n",&[("HumanGauge.m","classdef HumanGauge < handle\nproperties\nReading=4\nend\nend\n")]);
 let out=c.dir.path().join("human-out");let err=c.dir.path().join("human-err");
 let mut cmd=Command::new(env!("CARGO_BIN_EXE_mathlab"));cmd.env_clear();cmd.args(["run","--rust-program","--profile","class-v1","--workspace"]);for p in modules{cmd.arg("--module-file").arg(p);}let child=cmd.arg(c.entry()).env_clear().env("PATH",c.dir.path()).env("MATHLAB_OCTAVE",c.dir.path().join("octave-cli")).env("TMPDIR",c.dir.path()).current_dir(c.dir.path()).stdin(Stdio::null()).stdout(File::create(&out).unwrap()).stderr(File::create(&err).unwrap()).spawn().unwrap();
 let mut owned=Owned{child,out,err};let until=Instant::now()+Duration::from_secs(3);loop{owned.bounded();if let Some(status)=owned.child.try_wait().unwrap(){assert_eq!(status.code(),Some(0));break}assert!(Instant::now()<until,"owned human child deadline; no retry");thread::sleep(Duration::from_millis(1));}
 assert!(fs::read_to_string(&owned.err).unwrap().is_empty());let text=fs::read_to_string(&owned.out).unwrap();
 assert_eq!(text.matches("x: double dense rank2 [1, 300]").count(),2);
 assert_eq!(text.matches("stored window0..32 of300; stored_entries_complete=false").count(),1);assert_eq!(text.matches("stored window0..256 of300; stored_entries_complete=false").count(),1);
 let sections:Vec<&str>=text.split("x: double dense rank2 [1, 300]\n").collect();assert_eq!(sections.len(),3);
 for (section,count) in sections[1..].iter().zip([32usize,256]){let lines:Vec<&str>=section.lines().take_while(|l|!l.contains("stored window")).collect();assert_eq!(lines.len(),count);for(i,line)in lines.iter().enumerate(){assert!(line.starts_with(&format!("  [{i}] ")));}assert!(!section.contains(&format!("  [{count}] ")));}
 assert_eq!(text.matches("h: HumanGauge handle identity").count(),2);assert_eq!(text.matches("valid=true").count(),2);assert!(!text.contains("Reading"));assert!(!text.contains("h: double"));assert!(text.contains("Class program success:"));c.no_native();
}

#[test]
fn cli_class_tall_sc06_rank_refusal_and_square_lu_are_real_headless_commands(){
 let mut c=Case::new();let(code,rows)=c.spawn(r"A=[1,0;0,1;1,1];B=[1,2;2,4;4,8];X=A\B;",&[],&[]).finish();assert_eq!(code,0);success(&rows);let actual=words(binding(&rows[1],"X"));for(a,b)in actual.iter().zip([4./3.,7./3.,8./3.,14./3.]){assert!((a-b).abs()<1e-12);}assert_eq!(binding(&rows[1],"X")["shape"],json!([2,2]));
 let(code,rows)=c.spawn(r"seed=17;bad=[1,2;2,4;3,6]\[1;2;3];unreached=1;",&[],&[]).finish();assert_eq!(code,1);assert_eq!(rows[1]["status"],"error");assert_eq!(rows[1]["error"]["cause_code"],"Singular");assert_eq!(words(binding(&rows[1],"seed")),[17.]);assert!(!rows[1]["workspace"].as_array().unwrap().iter().any(|v|v["name"]=="bad"||v["name"]=="unreached"));resources(&rows[1]);
 let(code,rows)=c.spawn(r"X=[4,1;1,3]\[1;2];",&[],&[]).finish();assert_eq!(code,0);success(&rows);let x=words(binding(&rows[1],"X"));assert!((x[0]-1./11.).abs()<1e-12&&(x[1]-7./11.).abs()<1e-12);c.no_native();
}
const LS_EXPECTED_SOURCE_SHA:&str="aa18087566194bbd2a306875d831dab934f5fe4d2a55f8c146308a8e6f173adf";

#[test]
fn cli_class_full_4096_by_32_eight_rhs_noisy_fit_complete_export_and_fraction_oracle(){
 let mut c=Case::new();let source=include_str!("../../../examples/rust-least-squares.m");let destination=c.dir.path().join("least-squares.array");
 let child=c.spawn(source,&[],&["--export-binding","ls_validation","--export-file",destination.to_str().unwrap()]);let outcome_path=child.out.clone();let(code,rows)=child.finish_bound(Duration::from_secs(60));assert_eq!(code,0);success(&rows);
 assert_eq!(binding(&rows[1],"ls_X")["shape"],json!([32,8]));assert_eq!(binding(&rows[1],"ls_validation")["shape"],json!([196864,1]));assert_eq!(rows[1]["export"]["complete"],true);assert_eq!(rows[1]["export"]["payload_bytes"],3_149_824);assert!(rows[1]["stats"]["primitive_steps"].as_u64().unwrap()<=630_765_696);assert!(rows[1]["stats"]["peak_live_payload_bytes"].as_u64().unwrap()<=24<<20);let reserved=rows[1]["stats"]["peak_operation_reserved_bytes"].as_u64().unwrap();assert!(reserved>=3_149_824 && reserved<=4<<20,"complete validation pack is separately reserved; QR-only candidate is2692736B");
 assert_eq!(hex(&Sha256::digest(source.as_bytes())),LS_EXPECTED_SOURCE_SHA);let hash=LS_EXPECTED_SOURCE_SHA.to_owned();assert_eq!(rows[1]["source"]["sha256"],hash);
 let oracle=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/least_squares_oracle.py");let out=c.dir.path().join("oracle-out");let err=c.dir.path().join("oracle-err");
 let child=Command::new("/usr/bin/python3").arg("-B").arg(oracle).arg("--source").arg(c.entry()).arg("--expected-source-sha256").arg(&hash).arg("--outcome").arg(outcome_path).arg("--export").arg(&destination).env_clear().env("PYTHONNOUSERSITE","1").env("PYTHONDONTWRITEBYTECODE","1").stdin(Stdio::null()).stdout(File::create(&out).unwrap()).stderr(File::create(&err).unwrap()).spawn().unwrap();let(code,oracle_rows)=Owned{child,out,err}.finish_bound(Duration::from_secs(60));assert_eq!(code,0);assert_eq!(oracle_rows.len(),1);assert_eq!(oracle_rows[0]["status"],"passed");assert_eq!(oracle_rows[0]["source_sha256"],hash);assert_eq!(oracle_rows[0]["complex_words_checked"],196864);
 println!("{}",json!({"kind":"class-rectangular-least-squares-actual-metrics","source_sha256":hash,"stats":rows[1]["stats"],"independent_oracle":oracle_rows[0]}));c.no_native();
}


#[cfg(not(onl_qr_phase_probe))]
#[test]
fn cli_class_normal_build_ignores_qr_probe_environment(){
 let mut c=Case::new();let modules=c.write(r"A=[1,0;0,1;1,1];B=[1;2;4];X=A\B;done=1;",&[]);let impossible=c.dir.path().join("not-a-socket");fs::write(&impossible,b"normal build must not inspect or connect").unwrap();let(code,rows)=c.start_probe(&modules,&[],Some(&impossible)).finish();assert_eq!(code,0);success(&rows);let x=words(binding(&rows[1],"X"));assert!((x[0]-4./3.).abs()<1e-12);assert!((x[1]-7./3.).abs()<1e-12);c.no_native();
}

#[cfg(onl_qr_phase_probe)]
fn qr_probe_phase(listener:&std::os::unix::net::UnixListener,ready:&Value)->std::os::unix::net::UnixStream{
 use std::{io::Read,os::fd::AsRawFd};
 let mut fd=libc::pollfd{fd:listener.as_raw_fd(),events:libc::POLLIN,revents:0};assert_eq!(unsafe{libc::poll(&mut fd,1,2000)},1,"bounded diagnostic peer accept");let(mut stream,_)=listener.accept().unwrap();stream.set_nonblocking(false).unwrap();let flags=unsafe{libc::fcntl(stream.as_raw_fd(),libc::F_GETFL)};assert!(flags>=0);assert_eq!(flags&libc::O_NONBLOCK,0,"bounded phase read requires blocking accepted stream");stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();stream.set_write_timeout(Some(Duration::from_secs(2))).unwrap();let mut frame=[0u8;32];stream.read_exact(&mut frame).unwrap();assert_eq!(&frame[..8],b"ONLQRP1\0");for(i,field)in["session_id","epoch","request_id"].into_iter().enumerate(){assert_eq!(u64::from_le_bytes(frame[8+i*8..16+i*8].try_into().unwrap()),ready["key"][field].as_u64().unwrap());}stream
}
#[cfg(onl_qr_phase_probe)]
fn qr_probe_cancel(deadline:bool){
 use std::{io::Write,os::unix::net::UnixListener};
 let mut c=Case::new();fs::set_permissions(c.dir.path(),fs::Permissions::from_mode(0o700)).unwrap();assert_eq!(fs::metadata(c.dir.path()).unwrap().permissions().mode()&0o777,0o700);let socket=c.dir.path().join("qr.sock");assert!(socket.as_os_str().len()<=100);let listener=UnixListener::bind(&socket).unwrap();listener.set_nonblocking(true).unwrap();let modules=c.write(r"seed=17;A=[1,0;0,1;1,1];B=[1;2;4];X=A\B;done=1;",&[]);let mut child=c.start_probe(&modules,if deadline{&["--timeout","1"]}else{&["--timeout","10"]},Some(&socket));let ready=child.ready();let mut stream=qr_probe_phase(&listener,&ready);
 // Phase frame is emitted only from the first real ApplyB boundary, after QR
 // has allocated its actual candidate. No delay guesses or synthetic cancel.
 if !deadline{assert_eq!(unsafe{libc::kill(child.child.id()as i32,libc::SIGINT)},0);stream.write_all(b"RELEASE1").unwrap();}
 let(code,rows)=child.finish_bound(Duration::from_secs(4));assert_eq!(rows.len(),2);let out=&rows[1];assert_eq!(code,if deadline{124}else{130});assert_eq!(out["reason"],if deadline{"Deadline"}else{"UserStop"});assert_eq!(out["phase"],"execution");assert_eq!(out["key"],ready["key"]);assert_eq!(out["source_graph_sha256"],ready["source_graph_sha256"]);assert_eq!(words(binding(out,"seed")),[17.]);assert_eq!(words(binding(out,"A")),[1.,0.,1.,0.,1.,1.]);assert_eq!(words(binding(out,"B")),[1.,2.,4.]);absent(out,"X");absent(out,"done");assert_eq!(out["after"]["revision"],3);assert_eq!(out["effect_sequence"],3);assert_eq!(out["stats"]["peak_operation_reserved_bytes"],312);resources(out);
 drop(stream);drop(listener);fs::remove_file(&socket).unwrap();let(code,rows)=c.spawn("recovered=31;",&[],&[]).finish();assert_eq!(code,0);success(&rows);assert_eq!(words(binding(&rows[1],"recovered")),[31.]);c.no_native();println!("QR_INSTRUMENTED_CLI_CANCEL phase=first_ApplyB reason={} exact_prefix=true normal_latency_or_layout_claim=false",if deadline{"Deadline"}else{"UserStop"});
}
#[cfg(onl_qr_phase_probe)]
#[test]
fn cli_class_instrumented_qr_apply_b_sigint_uses_real_signal_and_checkpoint(){qr_probe_cancel(false)}
#[cfg(onl_qr_phase_probe)]
#[test]
fn cli_class_instrumented_qr_apply_b_original_deadline_uses_real_checkpoint(){qr_probe_cancel(true)}
