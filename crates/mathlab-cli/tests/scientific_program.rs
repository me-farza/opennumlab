// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(unix)]
use std::{fs::{self,File},io::Read,os::unix::fs::PermissionsExt,path::{Path,PathBuf},process::{Command,Stdio,Child},thread,time::{Duration,Instant}};
use serde_json::Value;
use tempfile::TempDir;
struct Owned{child:Child,out:PathBuf,err:PathBuf}
impl Drop for Owned{fn drop(&mut self){if !matches!(self.child.try_wait(),Ok(Some(_))){let _=self.child.kill();}let _=self.child.wait();}}
impl Owned{
    fn bounded(&self){for p in [&self.out,&self.err]{assert!(fs::metadata(p).unwrap().len()<=1024*1024)}}
    fn ready(&mut self){let until=Instant::now()+Duration::from_secs(2);loop{self.bounded();let bytes=fs::read(&self.out).unwrap();if let Some(i)=bytes.iter().position(|b|*b==b'\n'){let row:Value=serde_json::from_slice(&bytes[..i]).unwrap();assert_eq!(row["event"],"ready");assert_eq!(row["schema_version"],2);return}assert!(self.child.try_wait().unwrap().is_none());assert!(Instant::now()<until);thread::sleep(Duration::from_millis(1));}}
    fn finish(mut self,seconds:u64)->(i32,Vec<Value>){let until=Instant::now()+Duration::from_secs(seconds);loop{self.bounded();if let Some(status)=self.child.try_wait().unwrap(){let out=fs::read_to_string(&self.out).unwrap();let err=fs::read_to_string(&self.err).unwrap();assert!(err.is_empty(),"{err}");return(status.code().unwrap(),out.lines().map(|l|serde_json::from_str(l).unwrap()).collect())}assert!(Instant::now()<until,"ownedchilddeadline");thread::sleep(Duration::from_millis(1));}}
}
struct Case{dir:TempDir,serial:usize}
impl Case{
    fn new()->Self{let dir=tempfile::tempdir().unwrap();let p=dir.path().join("octave-cli");fs::write(&p,"#!/bin/sh\ntouch octave-called\nexit 91\n").unwrap();fs::set_permissions(p,fs::Permissions::from_mode(0o755)).unwrap();Self{dir,serial:0}}
    fn source(&self,s:&str)->PathBuf{let p=self.dir.path().join("script.m");fs::write(&p,s).unwrap();p}
    fn spawn(&mut self,source:&str,extra:&[&str])->Owned{let path=self.source(source);self.serial+=1;let out=self.dir.path().join(format!("out{}",self.serial));let err=self.dir.path().join(format!("err{}",self.serial));let child=Command::new(env!("CARGO_BIN_EXE_mathlab")).args(["--json","run","--rust-program","--profile","scientific-v1","--workspace"]).args(extra).arg(path).env_clear().env("PATH",self.dir.path()).env("MATHLAB_OCTAVE",self.dir.path().join("octave-cli")).env("TMPDIR",self.dir.path()).current_dir(self.dir.path()).stdin(Stdio::null()).stdout(File::create(&out).unwrap()).stderr(File::create(&err).unwrap()).spawn().unwrap();Owned{child,out,err}}
    fn no_native(&self){assert!(!self.dir.path().join("octave-called").exists())}
}
fn binding<'a>(out:&'a Value,name:&str)->&'a Value{&out["workspace"].as_array().unwrap().iter().find(|v|v["name"]==name).unwrap()["value"]}
#[test]
fn scientific_cli_full_original_family_complete_export_independent_fraction_oracle(){
    let mut c=Case::new();let destination=c.dir.path().join("Y.onlsci");let file=destination.to_str().unwrap();let source=include_str!("../../../examples/scientific/frequency_response.m");let(code,records)=c.spawn(source,&["--export-binding","Y","--export-file",file]).finish(40);assert_eq!(code,0);assert_eq!(records.len(),2);let out=&records[1];assert_eq!(out["schema_version"],2);assert_eq!(out["stats"]["solves_completed"],8);let y=binding(out,"Y");assert_eq!(y["class"],"double");assert_eq!(y["complex"],true);assert_eq!(y["shape"],serde_json::json!([128,32,8]));assert_eq!(y["window_offset"],0);assert_eq!(y["window_count"],256);assert_eq!(y["payload_complete"],false);assert_eq!(out["export"]["complete"],true);assert_eq!(out["export"]["payload_bytes"],524288);assert!(fs::metadata(&destination).unwrap().len()<=524288+32768);
    let oracle=Path::new(env!("CARGO_MANIFEST_DIR")).join("../mathlab-array/tests/scientific_oracle.py");let output=c.dir.path().join("oracle-out");let errors=c.dir.path().join("oracle-err");let child=Command::new("/usr/bin/python3").arg(oracle).arg(&destination).env_clear().env("PYTHONNOUSERSITE","1").env("PYTHONDONTWRITEBYTECODE","1").stdin(Stdio::null()).stdout(File::create(&output).unwrap()).stderr(File::create(&errors).unwrap()).spawn().unwrap();let(code,rows)=Owned{child,out:output,err:errors}.finish(15);assert_eq!(code,0);assert_eq!(rows[0]["complex_words"],32768);assert_eq!(rows[0]["pages"],8);c.no_native();
}
#[test]
fn scientific_export_never_overwrites_existing_destination_and_full_borrow_survives(){
    let mut c=Case::new();let path=c.dir.path().join("existing");fs::write(&path,b"KEEP").unwrap();let(code,rows)=c.spawn("X=complex([1 2],[3 4]);",&["--export-binding","X","--export-file",path.to_str().unwrap()]).finish(3);assert_eq!(code,1);assert_eq!(rows[1]["status"],"success");assert_eq!(rows[1]["export"]["code"],"DestinationExists");assert_eq!(rows[1]["export"]["complete"],false);assert_eq!(fs::read(&path).unwrap(),b"KEEP");assert_eq!(binding(&rows[1],"X")["binary64_components"][1][1],"4010000000000000");c.no_native();
}
#[test]
fn scientific_real_owned_sigint_and_deadline_retain_complete_candidates(){
    let mut c=Case::new();let source="prior=7;A=eye(32);B=ones(32,8);while 1,X=A\\B;end";let mut child=c.spawn(source,&[]);child.ready();thread::sleep(Duration::from_millis(20));assert_eq!(unsafe{libc::kill(child.child.id() as i32,libc::SIGINT)},0);let(code,rows)=child.finish(3);assert_eq!(code,130);assert_eq!(rows[1]["reason"],"UserStop");if let Some(entries)=rows[1]["workspace"].as_array(){for entry in entries{if entry["name"]=="X"{assert_eq!(entry["value"]["shape"],serde_json::json!([32,8]));for word in entry["value"]["binary64_components"].as_array().unwrap(){assert_eq!(word[0],"3ff0000000000000");}}}}
    let(code,rows)=c.spawn(source,&["--timeout","1"]).finish(3);assert_eq!(code,124);assert_eq!(rows[1]["reason"],"Deadline");c.no_native();
}
#[test]
fn scientific_fuel_and_located_breakdown_have_no_fallback(){
    let mut c=Case::new();let(code,rows)=c.spawn("x=1;",&["--scientific-primitive-fuel","0"]).finish(3);assert_eq!(code,1);assert_eq!(rows[1]["status"],"resource");assert!(rows[1]["workspace"].as_array().unwrap().is_empty());
    let(code,rows)=c.spawn("prior=7;\nA=[1 1;1 1];X=A\\[1;1];after=9;",&[]).finish(3);assert_eq!(code,1);assert_eq!(rows[1]["error"]["code"],"Singular");assert_eq!(rows[1]["error"]["location"]["span"]["start_line"],2);assert_eq!(binding(&rows[1],"prior")["binary64_components"][0][0],"401c000000000000");c.no_native();
}

#[test]
fn scientific_export_io_failure_preserves_successful_workspace_and_reports_command_error(){
    let mut c=Case::new();let destination=c.dir.path().join("missing-parent").join("X.onlsci");
    let(code,rows)=c.spawn("prior=7;X=complex([1 2],[3 4]);",&["--export-binding","X","--export-file",destination.to_str().unwrap()]).finish(3);
    let out=&rows[1];assert_eq!(code,1);assert_eq!(out["command_exit"],code);assert_eq!(out["status"],"success");assert_eq!(out["export"]["code"],"ExportIo");assert_eq!(out["export"]["complete"],false);assert!(out["export"]["payload_bytes"].is_null());assert!(!destination.exists());assert!(!destination.parent().unwrap().exists());
    assert_eq!(binding(out,"prior")["binary64_components"][0][0],"401c000000000000");assert_eq!(binding(out,"X")["binary64_components"][1][1],"4010000000000000");c.no_native();
}
