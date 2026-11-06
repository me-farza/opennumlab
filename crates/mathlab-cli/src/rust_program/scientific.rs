// SPDX-License-Identifier: GPL-3.0-or-later
//! Scientific schema2 on the same immutable-source/AST/SIGINT pipeline.
use super::*;
use mathlab_engine::rust_program::scientific::{ScientificProgramSessionV1 as Session,ScientificProgramOutcomeV1 as Outcome,ScientificProgramErrorCodeV1 as Code,ScientificProgramOptionsV1 as Options,ScientificProgramStatsV1};
use mathlab_language::program::{ScientificProgramValueV1 as V,ScientificReadRangeV1};
const PROFILE:&str="scientific-v1";
const WORKSPACE_WINDOW:usize=256;
const EVENT_WINDOW:usize=32;
const EXPORT_META:usize=32768;
struct ValueWindow<'a>{value:&'a V,count:usize}
impl Serialize for ValueWindow<'_>{
    fn serialize<S:Serializer>(&self,s:S)->Result<S::Ok,S::Error>{
        let value=self.value;let count=value.len().min(self.count);let mut out=s.serialize_struct("ScientificValue",9)?;
        out.serialize_field("class",if value.class()==ProgramValueClassV1::Double{"double"}else{"logical"})?;
        out.serialize_field("complex",&value.is_complex())?;out.serialize_field("rank",&value.rank())?;out.serialize_field("shape",value.shape())?;out.serialize_field("storage_order","column-major")?;out.serialize_field("window_offset",&0)?;out.serialize_field("window_count",&count)?;out.serialize_field("payload_complete",&(count==value.len()))?;
        struct Components<'a>{v:&'a V,count:usize}
        impl Serialize for Components<'_>{fn serialize<S:Serializer>(&self,s:S)->Result<S::Ok,S::Error>{let mut seq=s.serialize_seq(Some(self.count))?;for i in 0..self.count{let z=self.v.component(i).unwrap();if self.v.is_complex(){seq.serialize_element(&[Word(z.re),Word(z.im)])?}else{seq.serialize_element(&[Word(z.re)])?}}seq.end()}}
        if value.class()==ProgramValueClassV1::Logical{out.serialize_field("logical_values",&LogicalWindow(value,count))?}else{out.serialize_field("binary64_components",&Components{v:value,count})?}out.end()
    }
}
struct LogicalWindow<'a>(&'a V,usize);
impl Serialize for LogicalWindow<'_>{fn serialize<S:Serializer>(&self,s:S)->Result<S::Ok,S::Error>{s.collect_seq((0..self.1).map(|i|self.0.word(i).unwrap()!=0.))}}
struct Workspace<'a>(Option<&'a Session>);
impl Serialize for Workspace<'_>{fn serialize<S:Serializer>(&self,s:S)->Result<S::Ok,S::Error>{let mut seq=s.serialize_seq(self.0.map(|v|v.bindings().len()))?;if let Some(session)=self.0{for e in session.bindings(){#[derive(Serialize)]struct Entry<'a>{name:&'a str,value:ValueWindow<'a>}seq.serialize_element(&Entry{name:e.name,value:ValueWindow{value:e.value,count:WORKSPACE_WINDOW}})?}}seq.end()}}
struct Events<'a>(&'a Outcome);
impl Serialize for Events<'_>{fn serialize<S:Serializer>(&self,s:S)->Result<S::Ok,S::Error>{let mut seq=s.serialize_seq(Some(self.0.events.len()))?;for event in &self.0.events{#[derive(Serialize)]struct Event<'a>{sequence:u64,location:Location,function:Option<usize>,name:Option<&'a str>,value:ValueWindow<'a>}seq.serialize_element(&Event{sequence:event.sequence,location:event.location.into(),function:event.function.map(NodeIdV1::index),name:event.name.as_deref(),value:ValueWindow{value:&event.value,count:EVENT_WINDOW}})?;}seq.end()}}
fn code(c:Code)->&'static str{match c{Code::UnknownBinding=>"UnknownBinding",Code::Arity=>"Arity",Code::UnassignedOutput=>"UnassignedOutput",Code::Type=>"Type",Code::Shape=>"Shape",Code::Index=>"Index",Code::UnsupportedFeature=>"UnsupportedFeature",Code::Allocation=>"Allocation",Code::Resource=>"Resource",Code::CounterExhausted=>"CounterExhausted",Code::Cancelled=>"Cancelled",Code::Value(e)=>cause(e)}}
fn cause(e:ScientificErrorV1)->&'static str{match e{ScientificErrorV1::Allocation=>"Allocation",ScientificErrorV1::Resource=>"Resource",ScientificErrorV1::Rank=>"Rank",ScientificErrorV1::Shape=>"Shape",ScientificErrorV1::Type=>"Type",ScientificErrorV1::Index=>"Index",ScientificErrorV1::Unsupported=>"UnsupportedFeature",ScientificErrorV1::Singular=>"Singular",ScientificErrorV1::NonFiniteInput=>"NonFiniteInput",ScientificErrorV1::ArithmeticBreakdown=>"ArithmeticBreakdown",ScientificErrorV1::Cancelled=>"Cancelled"}}
#[derive(Serialize)]
struct ScientificStats{transitions:u64,primitive_steps:u64,peak_live_payload_bytes:usize,peak_metadata_bytes:usize,solves_completed:u64,last_initial_component_metric:Option<Word>,last_maximum_u_component_metric:Option<Word>,last_solve_scalar_steps:Option<u64>,last_solve_complete:Option<bool>,maximum_growth:Option<Word>,maximum_growth_finite:Option<bool>,last_growth_class:Option<&'static str>}
impl From<ScientificProgramStatsV1> for ScientificStats{fn from(v:ScientificProgramStatsV1)->Self{Self{transitions:v.transitions,primitive_steps:v.primitive_steps,peak_live_payload_bytes:v.peak_live_payload_bytes,peak_metadata_bytes:v.peak_metadata_bytes,solves_completed:v.solves_completed,last_initial_component_metric:v.last_solve.map(|d|Word(d.initial_metric)),last_maximum_u_component_metric:v.last_solve.map(|d|Word(d.maximum_u_metric)),last_solve_scalar_steps:v.last_solve.map(|d|d.scalar_steps),last_solve_complete:v.last_solve.map(|_|v.last_solve_complete),maximum_growth:v.maximum_growth.map(Word),maximum_growth_finite:v.maximum_growth.map(f64::is_finite),last_growth_class:v.last_solve.map(|d|if d.initial_metric==0.{"undefined-zero-initial"}else if d.growth_factor().unwrap().is_finite(){"finite"}else{"overflow"})}}}
#[derive(Serialize)]
struct ExportResult<'a>{binding:&'a str,status:&'static str,complete:bool,payload_bytes:Option<usize>,sha256:Option<HexDigest>,code:Option<&'static str>}
struct Report<'a>{source:Option<&'a Source>,outcome:Option<&'a Outcome>,session:Option<&'a Session>,early:Option<Early>,workspace:bool,complete:bool,export:Option<&'a ExportResult<'a>>}
impl Report<'_>{
    fn status(&self)->&'static str{if let Some(e)=self.early{return e.status}match self.outcome.unwrap().status{ProgramStatusV1::Success=>"success",ProgramStatusV1::Error=>"error",ProgramStatusV1::Resource=>"resource",ProgramStatusV1::Cancelled(_)=>"cancelled"}}
    fn reason(&self)->Option<ProgramCancelReasonV1>{self.early.and_then(|e|e.reason).or_else(||self.outcome.and_then(|o|if let ProgramStatusV1::Cancelled(reason)=o.status{Some(reason)}else{None}))}
    fn exit(&self)->i32{if self.early.is_some_and(|e|e.phase=="encoding"&&e.code=="ReplyLimit"){return 1}match self.reason(){Some(ProgramCancelReasonV1::UserStop)=>130,Some(ProgramCancelReasonV1::Deadline)=>124,None if self.status()=="success"=>match self.export.and_then(|e|e.code){None=>0,Some("Interrupted")=>130,_=>1},_=>1}}
}
impl Serialize for Report<'_>{fn serialize<S:Serializer>(&self,s:S)->Result<S::Ok,S::Error>{
    let mut out=s.serialize_struct("ScientificOutcome",21)?;out.serialize_field("schema_version",&2)?;out.serialize_field("kind",KIND)?;out.serialize_field("backend","rust")?;out.serialize_field("profile",PROFILE)?;out.serialize_field("event","outcome")?;out.serialize_field("status",self.status())?;out.serialize_field("phase",self.early.map(|e|e.phase).unwrap_or("execution"))?;out.serialize_field("source",&self.source)?;
    out.serialize_field("key",&self.outcome.map(|o|Key::from(o.key)))?;out.serialize_field("before",&self.outcome.map(|o|Context::from(o.start)))?;out.serialize_field("after",&self.outcome.map(|o|Context::from(o.final_context)))?;out.serialize_field("effect_sequence",&self.outcome.map_or(0,|o|o.committed_effect_sequence))?;out.serialize_field("completed_leaf_statement_count",&self.outcome.map_or(0,|o|o.completed_leaf_statement_count))?;out.serialize_field("stats",&self.outcome.map(|o|ScientificStats::from(o.stats)))?;out.serialize_field("reason",&self.reason().map(super::reason))?;out.serialize_field("events_complete",&self.complete)?;
    out.serialize_field("events",&self.outcome.filter(|_|self.complete).map(Events))?;out.serialize_field("workspace",&if self.workspace&&self.complete{Some(Workspace(self.session))}else{None})?;
    #[derive(Serialize)]struct Error{stage:&'static str,code:&'static str,location:Option<Location>,call_stack:[Option<Location>;32],call_depth:usize,cause_code:Option<&'static str>}
    let error=if let Some(e)=self.early{Some(Error{stage:e.phase,code:e.code,location:e.span.map(|span|Location{source_id:1,span:span.into()}),call_stack:[None;32],call_depth:0,cause_code:None})}else{self.outcome.and_then(|o|o.error).map(|e|Error{stage:"execution",code:code(e.code),location:Some(e.location.into()),call_stack:e.call_stack.map(|l|l.map(Location::from)),call_depth:e.call_depth,cause_code:if let Code::Value(c)=e.code{Some(cause(c))}else{None}})};
    out.serialize_field("error",&error)?;out.serialize_field("export",&self.export)?;out.serialize_field("command_exit",&self.exit())?;out.end()
}}
fn text_value(out:&mut Reply,name:&str,v:&V,window:usize)->io::Result<()>{
    writeln!(out,"{name}: {}{} rank{} {:?}",if v.class()==ProgramValueClassV1::Double{"double"}else{"logical"},if v.is_complex(){" complex"}else{""},v.rank(),v.shape())?;
    for i in 0..v.len().min(window){let z=v.component(i).unwrap();if v.is_complex(){writeln!(out,"  [{i}] {:.16e} {:+.16e}i",z.re,z.im)?}else{writeln!(out,"  [{i}] {:.16e}",z.re)?}}
    if v.len()>window{writeln!(out,"  window0..{} of{}; payload_complete=false",window,v.len())?}Ok(())
}
fn finish(reply:&mut Reply,json:bool,report:Report<'_>)->i32{
    let exit=report.exit();let encoded=if json{reply.json(&report)}else{( ||{if let Some(out)=report.outcome{if report.complete{for event in &out.events{text_value(reply,event.name.as_deref().unwrap_or("ans"),&event.value,EVENT_WINDOW)?}}}if report.workspace&&report.complete{if let Some(session)=report.session{for e in session.bindings(){text_value(reply,e.name,e.value,WORKSPACE_WINDOW)?}}}if let Some(error)=report.outcome.and_then(|o|o.error){writeln!(reply,"{} at source{} line{} bytes{}..{}",code(error.code),error.location.source_id,error.location.span.start_line,error.location.span.start,error.location.span.end)?;}if let Some(error)=report.early{writeln!(reply,"{} during{}",error.code,error.phase)?;}if let Some(export)=report.export{writeln!(reply,"Export {}: {}{}",export.binding,export.status,export.code.unwrap_or(""))?;}writeln!(reply,"Scientific program {}: {} effects.",report.status(),report.outcome.map_or(0,|o|o.committed_effect_sequence))})()};
    if encoded.is_err(){let fallback=Report{early:Some(Early::resource("encoding","ReplyLimit")),complete:false,..report};let encoded=if json{reply.json(&fallback)}else{reply.bytes.clear();writeln!(reply,"Scientific program resource: ReplyLimit (output incomplete); effects{}; completed{}; revision{}.",fallback.outcome.map_or(0,|o|o.committed_effect_sequence),fallback.outcome.map_or(0,|o|o.completed_leaf_statement_count),fallback.outcome.map_or(0,|o|o.final_context.revision))};if encoded.is_err(){return 1}if emit(reply).is_err(){return 1}return 1}if emit(reply).is_err(){1}else{exit}
}
fn early(reply:&mut Reply,json:bool,workspace:bool,source:Option<&Source>,e:Early)->i32{finish(reply,json,Report{source,outcome:None,session:None,early:Some(e),workspace,complete:true,export:None})}
#[derive(Serialize)]
struct ExportHeader<'a>{format:&'static str,class:&'static str,complex:bool,shape:&'a[u64],elements:usize,component_encoding:&'static str,storage:&'static str,binding:&'a str,source_sha256:&'a HexDigest}
#[derive(Serialize)]
struct ExportFooter{complete:bool,payload_bytes:usize,sha256:HexDigest}
/// Exact new destination, no truncation/overwrite and no implicit directories.
/// Failure can leave a partial file. Readers require the complete footer and
/// digest; a late flush error remains a command error even if bytes are complete.
/// This format promises integrity/completeness, not crash-durable fsync.
fn export<'a>(name:&'a str,path:&Path,value:&V,source:&Source)->ExportResult<'a>{
    let mut result=ExportResult{binding:name,status:"error",complete:false,payload_bytes:None,sha256:None,code:Some("ExportIo")};
    let operation=(||->io::Result<HexDigest>{
        if signal::requested(){return Err(io::Error::from(io::ErrorKind::Interrupted))}
        let mut metadata=Reply::new(EXPORT_META)?;
        metadata.json(&ExportHeader{format:"onl-scientific-array-v1",class:if value.class()==ProgramValueClassV1::Double{"double"}else{"logical"},complex:value.is_complex(),shape:value.shape(),elements:value.len(),component_encoding:if value.class()==ProgramValueClassV1::Double{"ieee754-binary64-le"}else{"uint8-bool"},storage:"column-major-interleaved",binding:name,source_sha256:&source.sha256})?;
        let mut options=OpenOptions::new();options.write(true).create_new(true);
        #[cfg(unix)]{use std::os::unix::fs::OpenOptionsExt;options.custom_flags(libc::O_NOFOLLOW|libc::O_NONBLOCK|libc::O_CLOEXEC);}
        let mut file=options.open(path)?;if !file.metadata()?.is_file(){return Err(io::Error::from(io::ErrorKind::InvalidInput))}
        file.write_all(&metadata.bytes)?;
        let mut hash=Sha256::new();let mut buffer=[0u8;8192];let mut used=0usize;let mut bytes=0usize;
        let unit=if value.class()==ProgramValueClassV1::Logical{1}else if value.is_complex(){16}else{8};
        let expected=value.len().checked_mul(unit).filter(|n|*n<=16*1024*1024).ok_or(io::ErrorKind::InvalidData)?;
        for start in (0..value.len()).step_by(128){
            if signal::requested(){return Err(io::Error::from(io::ErrorKind::Interrupted))}
            let count=(value.len()-start).min(128);
            let range=value.read_range(start,count).map_err(|_|io::Error::from(io::ErrorKind::InvalidData))?;
            let mut append=|word:&[u8]|->io::Result<()>{
                if used+word.len()>buffer.len(){file.write_all(&buffer[..used])?;hash.update(&buffer[..used]);bytes+=used;used=0;}
                buffer[used..used+word.len()].copy_from_slice(word);used+=word.len();Ok(())
            };
            match range{ScientificReadRangeV1::Real(words)=>for word in words{append(&word.to_bits().to_le_bytes())?},ScientificReadRangeV1::Complex(words)=>for word in words{append(&word.re.to_bits().to_le_bytes())?;append(&word.im.to_bits().to_le_bytes())?},ScientificReadRangeV1::Logical(words)=>append(words)?}
        }
        if used!=0{file.write_all(&buffer[..used])?;hash.update(&buffer[..used]);bytes+=used;}
        if bytes!=expected{return Err(io::Error::from(io::ErrorKind::InvalidData))}
        if signal::requested(){return Err(io::Error::from(io::ErrorKind::Interrupted))}
        let digest:[u8;32]=hash.finalize().into();result.payload_bytes=Some(bytes);
        metadata.json(&ExportFooter{complete:true,payload_bytes:bytes,sha256:HexDigest(digest)})?;
        file.write_all(b"\n")?;file.write_all(&metadata.bytes)?;file.flush()?;
        Ok(HexDigest(digest))
    })();
    match operation{Ok(digest)=>{result.status="complete";result.complete=true;result.sha256=Some(digest);result.code=None},Err(e)=>{result.payload_bytes=None;result.code=Some(match e.kind(){io::ErrorKind::AlreadyExists=>"DestinationExists",io::ErrorKind::Interrupted=>"Interrupted",io::ErrorKind::OutOfMemory=>"Allocation",_=>"ExportIo"});}}result
}
#[derive(Serialize)]
struct ScientificLimits{transition_fuel:u64,primitive_fuel:u64,max_rank:usize,max_elements:usize,max_value_bytes:usize,max_live_payload_bytes:usize,max_metadata_bytes:usize}
#[derive(Serialize)]
struct ScientificReady<'a>{schema_version:u8,kind:&'static str,backend:&'static str,profile:&'static str,event:&'static str,source:&'a Source,key:Key,context:Context,limits:ScientificLimits}
pub(crate) fn run(path:&Path,json:bool,workspace:bool,timeout:Option<Duration>,transition_fuel:Option<u64>,primitive_fuel:Option<u64>,requested_export:Option<(&str,&Path)>)->i32{
    let mut reply=match Reply::new(REPLY_LIMIT){Ok(reply)=>reply,Err(_)=>{let message=if json{"{\"schema_version\":2,\"kind\":\"mathlab-rust-program\",\"backend\":\"rust\",\"profile\":\"scientific-v1\",\"event\":\"outcome\",\"status\":\"resource\",\"phase\":\"admission\",\"error\":{\"code\":\"Allocation\"},\"events_complete\":false,\"workspace\":null}\n"}else{"Scientific program resource: Allocation\n"};let _=io::stdout().lock().write_all(message.as_bytes());return 1}};
    if requested_export.is_some_and(|(name,_)|name.len()>2048){return early(&mut reply,json,workspace,None,Early::error("admission","InvalidExportName"))}
    let _signal=match signal::SignalGuard::install(){Ok(guard)=>guard,Err(_)=>return early(&mut reply,json,workspace,None,Early::error("admission","SignalSetup"))};
    let PreparedSource{source,module,deadline}=match prepare_file(path,timeout){Ok(source)=>source,Err(failure)=>return early(&mut reply,json,workspace,failure.source.as_ref(),failure.early)};
    let mut session=match Session::new(){Ok(session)=>session,Err(e)=>return early(&mut reply,json,workspace,Some(&source),Early::resource("admission",code(e)))};
    let context=session.context();let options=Options{deadline,transition_fuel:transition_fuel.unwrap_or(5_000_000),primitive_fuel:primitive_fuel.unwrap_or(1_000_000_000)};
    let request=match session.prepare(module,options){Ok(request)=>request,Err(e)=>return early(&mut reply,json,workspace,Some(&source),Early::resource("admission",code(e)))};
    let handle=request.cancel_handle();
    let ready=if json{reply.json(&ScientificReady{schema_version:2,kind:KIND,backend:"rust",profile:PROFILE,event:"ready",source:&source,key:handle.key().into(),context:context.into(),limits:ScientificLimits{transition_fuel:options.transition_fuel,primitive_fuel:options.primitive_fuel,max_rank:8,max_elements:1<<20,max_value_bytes:16<<20,max_live_payload_bytes:64<<20,max_metadata_bytes:16<<20}})}else{writeln!(&mut reply,"Running scientific-v1; Ctrl+C requests Stop.")};
    if ready.is_err(){drop(request);return early(&mut reply,json,workspace,Some(&source),Early::resource("encoding","ReplyLimit"))}
    if emit(&reply).is_err(){return 1}reply.limit-=reply.bytes.len();reply.bytes.clear();
    let outcome=request.execute_controlled(|_,handle|{if signal::requested(){handle.request_stop();}});
    let export_result=requested_export.map(|(name,path)|{
        if outcome.status!=ProgramStatusV1::Success{ExportResult{binding:name,status:"skipped",complete:false,payload_bytes:None,sha256:None,code:Some("ExecutionNotSuccessful")}}
        else if let Some(value)=session.binding(name){export(name,path,value,&source)}
        else{ExportResult{binding:name,status:"error",complete:false,payload_bytes:None,sha256:None,code:Some("UnknownBinding")}}
    });
    finish(&mut reply,json,Report{source:Some(&source),outcome:Some(&outcome),session:Some(&session),early:None,workspace,complete:true,export:export_result.as_ref()})
}

#[cfg(test)]
mod scientific_encoding_tests{
    use super::*;
    struct Control;
    impl ScientificControlV1 for Control{fn charge(&mut self,_:u64)->Result<(),ScientificErrorV1>{Ok(())}fn reserve(&mut self,_:usize)->Result<(),ScientificErrorV1>{Ok(())}}
    #[test]
    fn human_windows_and_replylimit_exit_preserve_reason(){
        let value=V::from_dense(ScientificDenseV1::zeros(ScientificShapeV1::new(&[1,40]).unwrap(),true,&mut Control).unwrap());
        let mut reply=Reply::new(32768).unwrap();text_value(&mut reply,"event",&value,EVENT_WINDOW).unwrap();
        let text=std::str::from_utf8(&reply.bytes).unwrap();assert!(text.contains("[31]"));assert!(!text.contains("[32]"));assert!(text.contains("window0..32 of40; payload_complete=false"));
        reply.bytes.clear();text_value(&mut reply,"workspace",&value,WORKSPACE_WINDOW).unwrap();let text=std::str::from_utf8(&reply.bytes).unwrap();assert!(text.contains("[39]"));assert!(!text.contains("payload_complete=false"));
        for reason in [ProgramCancelReasonV1::UserStop,ProgramCancelReasonV1::Deadline]{
            let report=Report{source:None,outcome:None,session:None,early:Some(Early{reason:Some(reason),..Early::resource("encoding","ReplyLimit")}),workspace:true,complete:false,export:None};
            assert_eq!(report.exit(),1);assert_eq!(report.reason(),Some(reason));reply.json(&report).unwrap();let row:serde_json::Value=serde_json::from_slice(&reply.bytes).unwrap();assert_eq!(row["command_exit"],1);assert_eq!(row["reason"],super::super::reason(reason));assert_eq!(row["events_complete"],false);
        }
    }
}
