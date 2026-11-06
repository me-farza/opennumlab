// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(unix)]
//! Actual original CLI paths, bounded owned children, no native fallback.
use std::{fs::{self,File},os::unix::fs::PermissionsExt,path::{Path,PathBuf},process::{Child,Command,Stdio},thread,time::{Duration,Instant}};
use serde_json::{Value,json};
use sha2::{Digest,Sha256};
use tempfile::TempDir;
const LOG_LIMIT:u64=1024*1024;
struct Owned{child:Child,out:PathBuf,err:PathBuf}
impl Drop for Owned{fn drop(&mut self){if !matches!(self.child.try_wait(),Ok(Some(_))){let _=self.child.kill();}let _=self.child.wait();}}
impl Owned{
    fn bounded(&self){for path in [&self.out,&self.err]{assert!(fs::metadata(path).unwrap().len()<=LOG_LIMIT,"owned child output bound");}}
    fn ready(&mut self){let until=Instant::now()+Duration::from_secs(2);loop{self.bounded();let raw=fs::read(&self.out).unwrap();if let Some(end)=raw.iter().position(|b|*b==b'\n'){let row:Value=serde_json::from_slice(&raw[..end]).unwrap();assert_eq!(row["event"],"ready");assert_eq!(row["schema_version"],3);assert_eq!(row["profile"],"scientific-v2");return}assert!(self.child.try_wait().unwrap().is_none());assert!(Instant::now()<until,"ready deadline");thread::sleep(Duration::from_millis(1));}}
    fn finish(mut self,seconds:u64)->(i32,Vec<Value>){let until=Instant::now()+Duration::from_secs(seconds);loop{self.bounded();if let Some(status)=self.child.try_wait().unwrap(){let output=fs::read_to_string(&self.out).unwrap();let errors=fs::read_to_string(&self.err).unwrap();assert!(errors.is_empty(),"{errors}");let rows=output.lines().map(|line|serde_json::from_str(line).unwrap()).collect();return(status.code().unwrap(),rows)}assert!(Instant::now()<until,"owned child deadline; no implicit retry");thread::sleep(Duration::from_millis(1));}}
}
struct Case{dir:TempDir,serial:usize}
impl Case{
    fn new()->Self{let dir=tempfile::tempdir().unwrap();let fake=dir.path().join("octave-cli");fs::write(&fake,"#!/bin/sh\ntouch native-called\nexit 91\n").unwrap();fs::set_permissions(&fake,fs::Permissions::from_mode(0o755)).unwrap();Self{dir,serial:0}}
    fn source_path(&self)->PathBuf{self.dir.path().join("script.m")}
    fn spawn(&mut self,source:&str,extra:&[&str])->Owned{self.spawn_profile("scientific-v2",source,extra)}
    fn spawn_profile(&mut self,profile:&str,source:&str,extra:&[&str])->Owned{
        fs::write(self.source_path(),source).unwrap();self.serial+=1;let out=self.dir.path().join(format!("out{}",self.serial));let err=self.dir.path().join(format!("err{}",self.serial));let child=Command::new(env!("CARGO_BIN_EXE_mathlab")).args(["--json","run","--rust-program","--profile",profile,"--workspace"]).args(extra).arg(self.source_path()).env_clear().env("PATH",self.dir.path()).env("MATHLAB_OCTAVE",self.dir.path().join("octave-cli")).env("TMPDIR",self.dir.path()).current_dir(self.dir.path()).stdin(Stdio::null()).stdout(File::create(&out).unwrap()).stderr(File::create(&err).unwrap()).spawn().unwrap();Owned{child,out,err}
    }
    fn no_native(&self){assert!(!self.dir.path().join("native-called").exists());}
}
fn binding<'a>(out:&'a Value,name:&str)->&'a Value{&out["workspace"].as_array().unwrap().iter().find(|entry|entry["name"]==name).unwrap()["value"]}
fn absent(out:&Value,name:&str){assert!(!out["workspace"].as_array().unwrap().iter().any(|entry|entry["name"]==name));}
fn word(value:&Value,index:usize)->f64{f64::from_bits(u64::from_str_radix(value["binary64_components"][index][0].as_str().unwrap(),16).unwrap())}
fn instrumentation(out:&Value){let stats=&out["stats"];assert_eq!(stats["ledger_poisoned"],false);assert!(stats["peak_live_payload_bytes"].as_u64().unwrap()<=64<<20);assert!(stats["peak_operation_reserved_bytes"].as_u64().unwrap()<=64<<20);assert!(stats["peak_metadata_bytes"].as_u64().unwrap()<=16<<20);assert!(stats["transitions"].as_u64().unwrap()<=5_000_000);assert!(stats["primitive_steps"].as_u64().unwrap()<=1_000_000_000);}
fn success(rows:&[Value]){assert_eq!(rows.len(),2);for(event,row)in ["ready","outcome"].into_iter().zip(rows){assert_eq!(row["event"],event);assert_eq!(row["schema_version"],3);assert_eq!(row["kind"],"mathlab-rust-program");assert_eq!(row["backend"],"rust");assert_eq!(row["profile"],"scientific-v2");}let out=&rows[1];assert_eq!(out["status"],"success");assert_eq!(out["command_exit"],0);assert_eq!(out["events_complete"],true);assert!(out["reason"].is_null());assert!(out["error"].is_null());instrumentation(out);}

#[test]
fn sparse_v2_profile_typed_storage_source_identity_and_v1_boundary(){
    let mut c=Case::new();let source="A=sparse([2 1 2],[2 1 2],[3 6 -3],32768,32768);x=[1;2];";let(code,rows)=c.spawn(source,&[]).finish(3);assert_eq!(code,0);success(&rows);let out=&rows[1];let a=binding(out,"A");assert_eq!(a["storage"],"csc");assert_eq!(a["storage_order"],"CSC");assert_eq!(a["shape"],json!([32768,32768]));assert_eq!(a["logical_numel"],1_073_741_824u64);assert_eq!(a["nnz"],1);assert_eq!(a["nzmax"],1);assert_eq!(a["stored_entry_count"],1);assert_eq!(a["stored_entries_complete"],true);assert_eq!(a["entries"][0],json!({"row":0,"column":0,"binary64_components":["4018000000000000"]}));assert!(a.get("binary64_components").is_none());let x=binding(out,"x");assert_eq!(x["storage"],"dense");assert_eq!(x["logical_numel"],2);assert_eq!(x["storage_order"],"column-major");let hash=format!("{:x}",Sha256::digest(source.as_bytes()));for row in &rows{assert_eq!(row["source"]["sha256"],hash);assert_eq!(row["source"]["bytes"],source.len());assert_eq!(row["source"]["id"],1);}assert_eq!(out["stats"]["sparse_constructors"],1);
    let(code,old)=c.spawn_profile("scientific-v1","x=[1;2];",&[]).finish(3);assert_eq!(code,0);assert_eq!(old[1]["schema_version"],2);assert_eq!(old[1]["profile"],"scientific-v1");assert!(binding(&old[1],"x").get("storage").is_none());c.no_native();
}

#[test]
fn sparse_v2_empty_scalar_expansion_and_demoting_constructor_native_derivatives(){
    let mut c=Case::new();let(code,rows)=c.spawn("A=sparse([],[],nan(),2,3);B=sparse(1,[],[],2,3);C=sparse([],1,[],2,3);D=sparse([],[],complex(2,3),0,0);E=sparse([1 1],[1 1],complex([2 -2],[4 -4]),1,1);",&[]).finish(3);assert_eq!(code,0);success(&rows);for name in ["A","B","C","D","E"]{let a=binding(&rows[1],name);assert_eq!(a["storage"],"csc");assert_eq!(a["complex"],false);assert_eq!(a["nnz"],0);assert_eq!(a["nzmax"],1);assert_eq!(a["stored_entries_complete"],true);assert!(a["entries"].as_array().unwrap().is_empty());}assert_eq!(binding(&rows[1],"A")["shape"],json!([2,3]));assert_eq!(binding(&rows[1],"D")["shape"],json!([0,0]));c.no_native();
}

#[test]
fn sparse_v2_nonfinite_structural_zero_and_complex_vector_native_derivatives(){
    let mut c=Case::new();let source="A=sparse([],[],[],2,2);X=A*[inf();1];B=sparse([1 2],[1 2],[2 3],2,2);Y=B*[inf();1];C=sparse(1,1,inf(),2,2);Z=C*[0;1];D=sparse(1,1,complex(2,3),2,2);W=D*[inf();1];E=sparse(1,1,complex(inf(),1),2,2);Q=E*[0;1];R=C*[1;1];F=sparse(1,1,2,2,2);T=F*[nan();1];S=sparse(1,1,inf(),1,1)*complex(1,0);";let(code,rows)=c.spawn(source,&[]).finish(3);assert_eq!(code,0);success(&rows);let out=&rows[1];for name in ["X","Y","Z","W","Q","R","T"]{assert_eq!(binding(out,name)["storage"],"dense");assert_eq!(binding(out,name)["shape"],json!([2,1]));}assert_eq!(word(binding(out,"X"),0),0.);assert_eq!(word(binding(out,"X"),1),0.);assert!(word(binding(out,"Y"),0).is_infinite());assert_eq!(word(binding(out,"Y"),1),3.);for name in ["Z","Q","T"]{assert!(word(binding(out,name),0).is_nan());assert_eq!(word(binding(out,name),1),0.);assert_eq!(binding(out,name)["complex"],false);}let w=binding(out,"W");assert_eq!(w["complex"],true);assert_eq!(w["binary64_components"][0],json!(["7ff0000000000000","7ff0000000000000"]));assert_eq!(binding(out,"S")["storage"],"csc");assert_eq!(binding(out,"S")["complex"],false);assert_eq!(binding(out,"S")["entries"][0]["binary64_components"][0],"7ff0000000000000");c.no_native();
}

#[test]
fn sparse_v2_nan_truth_checks_before_implicit_zero_and_preserves_prefix(){
    let mut c=Case::new();for condition in ["[1 nan()]","[0 nan()]"]{let source=format!("branch=7;A=sparse(ones(1,2),[1 2],{condition},1,2);\nif A\nbranch=1;\nend\nafter=9;");let(code,rows)=c.spawn(&source,&[]).finish(3);assert_eq!(code,1);let out=&rows[1];assert_eq!(out["status"],"error");assert_eq!(out["error"]["code"],"Type");assert_eq!(word(binding(out,"branch"),0),7.);absent(out,"after");instrumentation(out);}let(code,rows)=c.spawn("branch=0;A=sparse([1 2],[1 1],[1 1],2,1);if A,branch=1;end",&[]).finish(3);assert_eq!(code,0);success(&rows);assert_eq!(word(binding(&rows[1],"branch"),0),1.);c.no_native();
}

#[test]
fn sparse_v2_located_index_error_and_both_fuels_preserve_workspace(){
    let mut c=Case::new();let(code,rows)=c.spawn("prior=7;\nA=sparse([1 0],[1 1],[2 3],2,2);after=9;",&[]).finish(3);assert_eq!(code,1);let out=&rows[1];assert_eq!(out["error"]["code"],"Index");assert_eq!(out["error"]["location"]["span"]["start_line"],2);assert_eq!(word(binding(out,"prior"),0),7.);absent(out,"A");absent(out,"after");instrumentation(out);
    for option in ["--scientific-primitive-fuel","--scientific-transition-fuel"]{let(code,rows)=c.spawn("A=sparse(1,1,2,2,2);",&[option,"0"]).finish(3);assert_eq!(code,1);assert_eq!(rows[1]["status"],"resource");assert!(rows[1]["workspace"].as_array().unwrap().is_empty());assert_eq!(rows[1]["stats"]["ledger_poisoned"],false);}c.no_native();
}

#[test]
fn sparse_v2_owned_sigint_and_deadline_leave_only_complete_csc_or_dense_values(){
    let mut c=Case::new();let source="prior=7;A=sparse([1 2],[1 2],[2 3],2,2);while 1,X=A*[1;2];end";let mut child=c.spawn(source,&[]);child.ready();thread::sleep(Duration::from_millis(20));assert_eq!(unsafe{libc::kill(child.child.id() as i32,libc::SIGINT)},0);let(code,rows)=child.finish(3);assert_eq!(code,130);assert_eq!(rows[1]["reason"],"UserStop");instrumentation(&rows[1]);if rows[1]["workspace"].as_array().unwrap().iter().any(|e|e["name"]=="X"){let x=binding(&rows[1],"X");assert_eq!(word(x,0),2.);assert_eq!(word(x,1),6.);}
    let destination=c.dir.path().join("cancelled.csc");let(code,rows)=c.spawn(source,&["--timeout","1","--export-binding","A","--export-file",destination.to_str().unwrap()]).finish(3);assert_eq!(code,124);assert_eq!(rows[1]["reason"],"Deadline");assert_eq!(rows[1]["export"]["status"],"skipped");assert!(!destination.exists());instrumentation(&rows[1]);c.no_native();
}

#[test]
fn sparse_v2_export_no_clobber_missing_parent_and_used_empty_capacity(){
    let mut c=Case::new();let existing=c.dir.path().join("existing");fs::write(&existing,b"KEEP").unwrap();let source="prior=7;A=sparse(1,1,complex(2,3),2,2);";let(code,rows)=c.spawn(source,&["--export-binding","A","--export-file",existing.to_str().unwrap()]).finish(3);assert_eq!(code,1);assert_eq!(rows[1]["status"],"success");assert_eq!(rows[1]["export"]["code"],"DestinationExists");assert_eq!(fs::read(&existing).unwrap(),b"KEEP");assert_eq!(binding(&rows[1],"A")["complex"],true);
    let missing=c.dir.path().join("missing-parent/out.csc");let(code,rows)=c.spawn(source,&["--export-binding","A","--export-file",missing.to_str().unwrap()]).finish(3);assert_eq!(code,1);assert_eq!(rows[1]["status"],"success");assert_eq!(rows[1]["export"]["code"],"ExportIo");assert_eq!(rows[1]["export"]["complete"],false);assert!(rows[1]["export"]["payload_bytes"].is_null());assert!(!missing.parent().unwrap().exists());assert_eq!(word(binding(&rows[1],"prior"),0),7.);
    let empty=c.dir.path().join("empty.csc");let(code,rows)=c.spawn("A=sparse([],[],[],2,3);",&["--export-binding","A","--export-file",empty.to_str().unwrap()]).finish(3);assert_eq!(code,0);success(&rows);assert_eq!(rows[1]["export"]["payload_bytes"],16);let bytes=fs::read(empty).unwrap();let h=bytes.iter().position(|b|*b==b'\n').unwrap();let header:Value=serde_json::from_slice(&bytes[..h]).unwrap();assert_eq!(header["nnz"],0);assert_eq!(header["nzmax"],1);assert_eq!(header["sections"][2]["elements"],0);assert_eq!(&bytes[h+1..h+17],&[0u8;16]);assert_eq!(bytes[h+17],b'\n');let footer:Value=serde_json::from_slice(&bytes[h+18..]).unwrap();assert_eq!(footer["complete"],true);assert_eq!(footer["payload_bytes"],16);assert_eq!(footer["sha256"],format!("{:x}",Sha256::digest([0u8;16])));c.no_native();
}

#[test]
fn sparse_v2_complex_csc_and_unchanged_dense_export_section_words(){
    let mut c=Case::new();
    for (label,source,binding_name,expected,storage) in [
        ("complex-csc","A=sparse(1,1,complex(2,3),2,2);","A",32usize,"csc"),
        ("complex-dense","X=complex([1 2],[3 4]);","X",32usize,"column-major-interleaved"),
        ("logical-dense","X=[1 0]==[1 1];","X",2usize,"column-major-interleaved")]{
        let path=c.dir.path().join(label);let(code,rows)=c.spawn(source,&["--export-binding",binding_name,"--export-file",path.to_str().unwrap()]).finish(3);assert_eq!(code,0);success(&rows);let bytes=fs::read(path).unwrap();let end=bytes.iter().position(|b|*b==b'\n').unwrap();let header:Value=serde_json::from_slice(&bytes[..end]).unwrap();assert_eq!(header["storage"],storage);let payload=&bytes[end+1..end+1+expected];assert_eq!(bytes[end+1+expected],b'\n');let footer:Value=serde_json::from_slice(&bytes[end+2+expected..]).unwrap();assert_eq!(footer["complete"],true);assert_eq!(footer["payload_bytes"],expected);assert_eq!(footer["sha256"],format!("{:x}",Sha256::digest(payload)));
        if storage=="csc"{assert_eq!(header["format"],"onl-scientific-csc-v1");assert_eq!(header["sections"][2]["components"],2);assert_eq!(header["sections"][2]["offset_bytes"],16);let mut wanted=Vec::new();for x in [0u32,1,1,0]{wanted.extend_from_slice(&x.to_le_bytes());}wanted.extend_from_slice(&2f64.to_bits().to_le_bytes());wanted.extend_from_slice(&3f64.to_bits().to_le_bytes());assert_eq!(payload,wanted);let value=binding(&rows[1],"A");assert_eq!(value["entries"][0]["binary64_components"],json!(["4000000000000000","4008000000000000"]));}
        else if label=="complex-dense"{assert_eq!(header["format"],"onl-scientific-array-v1");assert_eq!(header["component_encoding"],"ieee754-binary64-le");let wanted:Vec<u8>=[1f64,3.,2.,4.].iter().flat_map(|x|x.to_bits().to_le_bytes()).collect();assert_eq!(payload,wanted);}
        else{assert_eq!(header["format"],"onl-scientific-array-v1");assert_eq!(header["class"],"logical");assert_eq!(header["component_encoding"],"uint8-bool");assert_eq!(payload,[1u8,0]);assert_eq!(binding(&rows[1],"X")["logical_values"],json!([true,false]));}
    }c.no_native();
}

#[test]
fn sparse_v2_full_poisson_two_complete_exports_independent_fraction_oracle(){
    // These are child safety ceilings, not a measured runtime promise or a fuel raise.
    // Final outer validation must admit the entire two-run plus oracle group.
    let mut c=Case::new();let source=include_str!("../../../examples/scientific/poisson_pcg.m");let matrix=c.dir.path().join("A.csc");let solution=c.dir.path().join("solution.array");let matrix_child=c.spawn(source,&["--export-binding","A","--export-file",matrix.to_str().unwrap()]);let matrix_outcome=matrix_child.out.clone();let(code,matrix_rows)=matrix_child.finish(40);assert_eq!(code,0);success(&matrix_rows);let solution_child=c.spawn(source,&["--export-binding","solution","--export-file",solution.to_str().unwrap()]);let solution_outcome=solution_child.out.clone();let(code,solution_rows)=solution_child.finish(40);assert_eq!(code,0);success(&solution_rows);
    for out in [&matrix_rows[1],&solution_rows[1]]{let flag=binding(out,"flag");let iterations=word(binding(out,"iterations"),0);assert_eq!(word(flag,0),0.);assert!(iterations>0.&&iterations<=512.&&iterations.fract()==0.);assert_eq!(out["stats"]["sparse_constructors"],1);assert!(out["stats"]["sparse_products"].as_u64().unwrap()>0);assert!(out["stats"]["operations_completed"].as_u64().unwrap()>0);assert_eq!(out["stats"]["peak_operation_reserved_bytes"],6_381_572);let a=binding(out,"A");assert_eq!(a["logical_numel"],1_073_741_824u64);assert_eq!(a["nnz"],223232);assert_eq!(a["stored_entry_count"],256);assert_eq!(a["stored_entries_complete"],false);assert_eq!(binding(out,"solution")["payload_complete"],false);}
    assert!(fs::metadata(&matrix).unwrap().len()<=2_809_860+65536);assert!(fs::metadata(&solution).unwrap().len()<=262144+65536);let hash=format!("{:x}",Sha256::digest(source.as_bytes()));let oracle=Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/sparse_oracle.py");let output=c.dir.path().join("oracle-out");let errors=c.dir.path().join("oracle-err");let child=Command::new("/usr/bin/python3").arg(oracle).arg("--source").arg(c.source_path()).arg("--expected-source-sha256").arg(&hash).arg("--matrix-export").arg(&matrix).arg("--matrix-outcome").arg(matrix_outcome).arg("--solution-export").arg(&solution).arg("--solution-outcome").arg(solution_outcome).env_clear().env("PYTHONNOUSERSITE","1").env("PYTHONDONTWRITEBYTECODE","1").stdin(Stdio::null()).stdout(File::create(&output).unwrap()).stderr(File::create(&errors).unwrap()).spawn().unwrap();let(code,rows)=Owned{child,out:output,err:errors}.finish(15);assert_eq!(code,0);assert_eq!(rows.len(),1);assert_eq!(rows[0]["status"],"passed");assert_eq!(rows[0]["source_sha256"],hash);assert_eq!(rows[0]["csc"]["stored_entries_checked"],223232);assert_eq!(rows[0]["solution"]["returned_words_checked"],32768);
    println!("{}",json!({"kind":"sparse-v2-full-poisson-actual-metrics","source_sha256":hash,"runs":[{"export_binding":"A","iterations":word(binding(&matrix_rows[1],"iterations"),0),"stats":matrix_rows[1]["stats"]},{"export_binding":"solution","iterations":word(binding(&solution_rows[1],"iterations"),0),"stats":solution_rows[1]["stats"]}],"independent_oracle":rows[0]}));c.no_native();
}
