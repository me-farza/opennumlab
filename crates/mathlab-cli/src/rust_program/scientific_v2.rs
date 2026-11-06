// SPDX-License-Identifier: GPL-3.0-or-later
//! Scientific schema3 on the same immutable-source/AST/SIGINT pipeline.
use super::*;
use mathlab_engine::rust_program::scientific_v2::{ScientificProgramSessionV2 as Session,ScientificProgramOutcomeV2 as Outcome,ScientificProgramErrorCodeV2 as Code,ScientificProgramOptionsV2 as Options,ScientificProgramStatsV2};
use mathlab_language::program::{ScientificProgramValueV2 as V,ScientificReadRangeV1,ScientificStorageViewV2,ScientificCscValuesV1};
const PROFILE:&str="scientific-v2";
const WORKSPACE_WINDOW:usize=256;
const EVENT_WINDOW:usize=32;
const EXPORT_META:usize=32768;
struct ValueWindow<'a>{value:&'a V,count:usize}
struct DenseComponents<'a>{value:&'a ScientificDenseV1,count:usize}
impl Serialize for DenseComponents<'_>{fn serialize<S:Serializer>(&self,s:S)->Result<S::Ok,S::Error>{let mut seq=s.serialize_seq(Some(self.count))?;for i in 0..self.count{let z=self.value.get(i).unwrap();if self.value.is_complex(){seq.serialize_element(&[Word(z.re),Word(z.im)])?}else{seq.serialize_element(&[Word(z.re)])?}}seq.end()}}
struct LogicalWindow<'a>(&'a ScientificDenseV1,usize);
impl Serialize for LogicalWindow<'_>{fn serialize<S:Serializer>(&self,s:S)->Result<S::Ok,S::Error>{s.collect_seq((0..self.1).map(|i|self.0.get(i).unwrap().re!=0.))}}
struct CscEntries<'a>{value:&'a ScientificCscV1,count:usize}
impl Serialize for CscEntries<'_>{fn serialize<S:Serializer>(&self,s:S)->Result<S::Ok,S::Error>{
    let mut seq=s.serialize_seq(Some(self.count))?;
    for slot in 0..self.count{
        let column=self.value.column_pointers().partition_point(|&p|p as usize<=slot)-1;
        #[derive(Serialize)]struct Entry<T:Serialize>{row:u32,column:usize,binary64_components:T}
        let row=self.value.row_indices()[slot];
        match self.value.values(){ScientificCscValuesV1::Real(v)=>seq.serialize_element(&Entry{row,column,binary64_components:[Word(v[slot])]})?,ScientificCscValuesV1::Complex(v)=>seq.serialize_element(&Entry{row,column,binary64_components:[Word(v[slot].re),Word(v[slot].im)]})?}
    }seq.end()
}}
impl Serialize for ValueWindow<'_>{fn serialize<S:Serializer>(&self,s:S)->Result<S::Ok,S::Error>{
    let value=self.value;let count=value.stored_len().min(self.count);let axes=value.axes();
    let mut out=s.serialize_struct("ScientificValueV2",if value.is_sparse(){13}else{11})?;
    out.serialize_field("class",if value.class()==ProgramValueClassV1::Double{"double"}else{"logical"})?;
    out.serialize_field("complex",&value.is_complex())?;out.serialize_field("rank",&value.rank())?;out.serialize_field("shape",&*axes)?;out.serialize_field("logical_numel",&value.logical_numel())?;
    match value.storage_view(){
        ScientificStorageViewV2::Dense(v)=>{out.serialize_field("storage","dense")?;out.serialize_field("storage_order","column-major")?;out.serialize_field("window_offset",&0)?;out.serialize_field("window_count",&count)?;out.serialize_field("payload_complete",&(count==v.len()))?;if value.class()==ProgramValueClassV1::Logical{out.serialize_field("logical_values",&LogicalWindow(v,count))?}else{out.serialize_field("binary64_components",&DenseComponents{value:v,count})?}},
        ScientificStorageViewV2::Csc(v)=>{out.serialize_field("storage","csc")?;out.serialize_field("storage_order","CSC")?;out.serialize_field("nnz",&v.nnz())?;out.serialize_field("nzmax",&v.nzmax())?;out.serialize_field("stored_entry_offset",&0)?;out.serialize_field("stored_entry_count",&count)?;out.serialize_field("stored_entries_complete",&(count==v.nnz()))?;out.serialize_field("entries",&CscEntries{value:v,count})?}
    }out.end()
}}
struct Workspace<'a>(Option<&'a Session>);
impl Serialize for Workspace<'_>{fn serialize<S:Serializer>(&self,s:S)->Result<S::Ok,S::Error>{let mut seq=s.serialize_seq(self.0.map(|v|v.bindings().len()))?;if let Some(session)=self.0{for e in session.bindings(){#[derive(Serialize)]struct Entry<'a>{name:&'a str,value:ValueWindow<'a>}seq.serialize_element(&Entry{name:e.name,value:ValueWindow{value:e.value,count:WORKSPACE_WINDOW}})?}}seq.end()}}
struct Events<'a>(&'a Outcome);
impl Serialize for Events<'_>{fn serialize<S:Serializer>(&self,s:S)->Result<S::Ok,S::Error>{let mut seq=s.serialize_seq(Some(self.0.events.len()))?;for event in &self.0.events{#[derive(Serialize)]struct Event<'a>{sequence:u64,location:Location,function:Option<usize>,name:Option<&'a str>,value:ValueWindow<'a>}seq.serialize_element(&Event{sequence:event.sequence,location:event.location.into(),function:event.function.map(NodeIdV1::index),name:event.name.as_deref(),value:ValueWindow{value:&event.value,count:EVENT_WINDOW}})?;}seq.end()}}
fn code(c:Code)->&'static str{match c{Code::UnknownBinding=>"UnknownBinding",Code::Arity=>"Arity",Code::UnassignedOutput=>"UnassignedOutput",Code::Type=>"Type",Code::Shape=>"Shape",Code::Index=>"Index",Code::UnsupportedFeature=>"UnsupportedFeature",Code::Allocation=>"Allocation",Code::Resource=>"Resource",Code::CounterExhausted=>"CounterExhausted",Code::Cancelled=>"Cancelled",Code::Value(e)=>cause(e)}}
fn cause(e:ScientificErrorV1)->&'static str{match e{ScientificErrorV1::Allocation=>"Allocation",ScientificErrorV1::Resource=>"Resource",ScientificErrorV1::Rank=>"Rank",ScientificErrorV1::Shape=>"Shape",ScientificErrorV1::Type=>"Type",ScientificErrorV1::Index=>"Index",ScientificErrorV1::Unsupported=>"UnsupportedFeature",ScientificErrorV1::Singular=>"Singular",ScientificErrorV1::NonFiniteInput=>"NonFiniteInput",ScientificErrorV1::ArithmeticBreakdown=>"ArithmeticBreakdown",ScientificErrorV1::Cancelled=>"Cancelled"}}
#[derive(Serialize)]
struct ScientificStats{transitions:u64,primitive_steps:u64,peak_live_payload_bytes:usize,peak_metadata_bytes:usize,solves_completed:u64,last_initial_component_metric:Option<Word>,last_maximum_u_component_metric:Option<Word>,last_solve_scalar_steps:Option<u64>,last_solve_complete:Option<bool>,maximum_growth:Option<Word>,maximum_growth_finite:Option<bool>,last_growth_class:Option<&'static str>,operations_completed:u64,peak_operation_reserved_bytes:usize,sparse_constructors:u64,sparse_products:u64,ledger_poisoned:bool}
impl From<ScientificProgramStatsV2> for ScientificStats{fn from(v:ScientificProgramStatsV2)->Self{Self{transitions:v.transitions,primitive_steps:v.primitive_steps,peak_live_payload_bytes:v.peak_live_payload_bytes,peak_metadata_bytes:v.peak_metadata_bytes,solves_completed:v.solves_completed,last_initial_component_metric:v.last_solve.map(|d|Word(d.initial_metric)),last_maximum_u_component_metric:v.last_solve.map(|d|Word(d.maximum_u_metric)),last_solve_scalar_steps:v.last_solve.map(|d|d.scalar_steps),last_solve_complete:v.last_solve.map(|_|v.last_solve_complete),maximum_growth:v.maximum_growth.map(Word),maximum_growth_finite:v.maximum_growth.map(f64::is_finite),operations_completed:v.operations_completed,peak_operation_reserved_bytes:v.peak_operation_reserved_bytes,sparse_constructors:v.sparse_constructors,sparse_products:v.sparse_products,ledger_poisoned:v.ledger_poisoned,last_growth_class:v.last_solve.map(|d|if d.initial_metric==0.{"undefined-zero-initial"}else if d.growth_factor().unwrap().is_finite(){"finite"}else{"overflow"})}}}
#[derive(Serialize)]
struct ExportResult<'a>{binding:&'a str,status:&'static str,complete:bool,payload_bytes:Option<usize>,sha256:Option<HexDigest>,code:Option<&'static str>}
struct Report<'a>{source:Option<&'a Source>,outcome:Option<&'a Outcome>,session:Option<&'a Session>,early:Option<Early>,workspace:bool,complete:bool,export:Option<&'a ExportResult<'a>>}
impl Report<'_>{
    fn status(&self)->&'static str{if let Some(e)=self.early{return e.status}match self.outcome.unwrap().status{ProgramStatusV1::Success=>"success",ProgramStatusV1::Error=>"error",ProgramStatusV1::Resource=>"resource",ProgramStatusV1::Cancelled(_)=>"cancelled"}}
    fn reason(&self)->Option<ProgramCancelReasonV1>{self.early.and_then(|e|e.reason).or_else(||self.outcome.and_then(|o|if let ProgramStatusV1::Cancelled(reason)=o.status{Some(reason)}else{None}))}
    fn exit(&self)->i32{if self.early.is_some_and(|e|e.phase=="encoding"&&e.code=="ReplyLimit"){return 1}match self.reason(){Some(ProgramCancelReasonV1::UserStop)=>130,Some(ProgramCancelReasonV1::Deadline)=>124,None if self.status()=="success"=>match self.export.and_then(|e|e.code){None=>0,Some("Interrupted")=>130,_=>1},_=>1}}
}
impl Serialize for Report<'_>{fn serialize<S:Serializer>(&self,s:S)->Result<S::Ok,S::Error>{
    let mut out=s.serialize_struct("ScientificOutcome",21)?;out.serialize_field("schema_version",&3)?;out.serialize_field("kind",KIND)?;out.serialize_field("backend","rust")?;out.serialize_field("profile",PROFILE)?;out.serialize_field("event","outcome")?;out.serialize_field("status",self.status())?;out.serialize_field("phase",self.early.map(|e|e.phase).unwrap_or("execution"))?;out.serialize_field("source",&self.source)?;
    out.serialize_field("key",&self.outcome.map(|o|Key::from(o.key)))?;out.serialize_field("before",&self.outcome.map(|o|Context::from(o.start)))?;out.serialize_field("after",&self.outcome.map(|o|Context::from(o.final_context)))?;out.serialize_field("effect_sequence",&self.outcome.map_or(0,|o|o.committed_effect_sequence))?;out.serialize_field("completed_leaf_statement_count",&self.outcome.map_or(0,|o|o.completed_leaf_statement_count))?;out.serialize_field("stats",&self.outcome.map(|o|ScientificStats::from(o.stats)))?;out.serialize_field("reason",&self.reason().map(super::reason))?;out.serialize_field("events_complete",&self.complete)?;
    out.serialize_field("events",&self.outcome.filter(|_|self.complete).map(Events))?;out.serialize_field("workspace",&if self.workspace&&self.complete{Some(Workspace(self.session))}else{None})?;
    #[derive(Serialize)]struct Error{stage:&'static str,code:&'static str,location:Option<Location>,call_stack:[Option<Location>;32],call_depth:usize,cause_code:Option<&'static str>}
    let error=if let Some(e)=self.early{Some(Error{stage:e.phase,code:e.code,location:e.span.map(|span|Location{source_id:1,span:span.into()}),call_stack:[None;32],call_depth:0,cause_code:None})}else{self.outcome.and_then(|o|o.error).map(|e|Error{stage:"execution",code:code(e.code),location:Some(e.location.into()),call_stack:e.call_stack.map(|l|l.map(Location::from)),call_depth:e.call_depth,cause_code:if let Code::Value(c)=e.code{Some(cause(c))}else{None}})};
    out.serialize_field("error",&error)?;out.serialize_field("export",&self.export)?;out.serialize_field("command_exit",&self.exit())?;out.end()
}}
fn text_value(out:&mut Reply,name:&str,v:&V,window:usize)->io::Result<()>{
    writeln!(out,"{name}: {}{} {} rank{} {:?}",if v.class()==ProgramValueClassV1::Double{"double"}else{"logical"},if v.is_complex(){" complex"}else{""},if v.is_sparse(){"CSC"}else{"dense"},v.rank(),&*v.axes())?;
    let count=v.stored_len().min(window);
    match v.storage_view(){
        ScientificStorageViewV2::Dense(d)=>for i in 0..count{let z=d.get(i).unwrap();if d.is_complex(){writeln!(out,"  [{i}] {:.16e} {:+.16e}i",z.re,z.im)?}else{writeln!(out,"  [{i}] {:.16e}",z.re)?}},
        ScientificStorageViewV2::Csc(c)=>{writeln!(out,"  nnz{} nzmax{}; zero-based stored-entry window",c.nnz(),c.nzmax())?;for i in 0..count{let col=c.column_pointers().partition_point(|&p|p as usize<=i)-1;let row=c.row_indices()[i];match c.values(){ScientificCscValuesV1::Real(z)=>writeln!(out,"  ({row},{col}) {:.16e}",z[i])?,ScientificCscValuesV1::Complex(z)=>writeln!(out,"  ({row},{col}) {:.16e} {:+.16e}i",z[i].re,z[i].im)?}}}
    }
    if count<v.stored_len(){writeln!(out,"  stored window0..{} of{}; stored_entries_complete=false",count,v.stored_len())?}Ok(())
}
fn finish(reply:&mut Reply,json:bool,report:Report<'_>)->i32{
    let exit=report.exit();let encoded=if json{reply.json(&report)}else{( ||{if let Some(out)=report.outcome{if report.complete{for event in &out.events{text_value(reply,event.name.as_deref().unwrap_or("ans"),&event.value,EVENT_WINDOW)?}}}if report.workspace&&report.complete{if let Some(session)=report.session{for e in session.bindings(){text_value(reply,e.name,e.value,WORKSPACE_WINDOW)?}}}if let Some(error)=report.outcome.and_then(|o|o.error){writeln!(reply,"{} at source{} line{} bytes{}..{}",code(error.code),error.location.source_id,error.location.span.start_line,error.location.span.start,error.location.span.end)?;}if let Some(error)=report.early{writeln!(reply,"{} during{}",error.code,error.phase)?;}if let Some(export)=report.export{writeln!(reply,"Export {}: {}{}",export.binding,export.status,export.code.unwrap_or(""))?;}writeln!(reply,"Scientific program {}: {} effects.",report.status(),report.outcome.map_or(0,|o|o.committed_effect_sequence))})()};
    if encoded.is_err(){let fallback=Report{early:Some(Early::resource("encoding","ReplyLimit")),complete:false,..report};let encoded=if json{reply.json(&fallback)}else{reply.bytes.clear();writeln!(reply,"Scientific program resource: ReplyLimit (output incomplete); effects{}; completed{}; revision{}.",fallback.outcome.map_or(0,|o|o.committed_effect_sequence),fallback.outcome.map_or(0,|o|o.completed_leaf_statement_count),fallback.outcome.map_or(0,|o|o.final_context.revision))};if encoded.is_err(){return 1}if emit(reply).is_err(){return 1}return 1}if emit(reply).is_err(){1}else{exit}
}
fn early(reply:&mut Reply,json:bool,workspace:bool,source:Option<&Source>,e:Early)->i32{finish(reply,json,Report{source,outcome:None,session:None,early:Some(e),workspace,complete:true,export:None})}
#[derive(Serialize)]
struct ExportHeader<'a>{format:&'static str,class:&'static str,complex:bool,shape:&'a[u64],elements:usize,component_encoding:&'static str,storage:&'static str,binding:&'a str,source_sha256:&'a HexDigest}
#[derive(Serialize)]
struct CscSection{ name:&'static str,encoding:&'static str,elements:usize,#[serde(skip_serializing_if="Option::is_none")]components:Option<usize>,bytes:usize,offset_bytes:usize }
#[derive(Serialize)]
struct CscHeader<'a>{format:&'static str,class:&'static str,complex:bool,shape:&'a[u64],logical_elements:u64,nnz:usize,nzmax:usize,index_base:u8,storage:&'static str,sections:[CscSection;3],payload_bytes:usize,binding:&'a str,source_sha256:&'a HexDigest}
#[derive(Serialize)]
struct ExportFooter{complete:bool,payload_bytes:usize,sha256:HexDigest}
struct PayloadWriter<'a>{file:&'a mut File,hash:Sha256,buffer:[u8;8192],used:usize,bytes:usize}
impl<'a> PayloadWriter<'a>{
    fn new(file:&'a mut File)->Self{Self{file,hash:Sha256::new(),buffer:[0;8192],used:0,bytes:0}}
    fn checkpoint()->io::Result<()>{if signal::requested(){Err(io::Error::from(io::ErrorKind::Interrupted))}else{Ok(())}}
    fn flush_chunk(&mut self)->io::Result<()>{if self.used!=0{Self::checkpoint()?;self.file.write_all(&self.buffer[..self.used])?;self.hash.update(&self.buffer[..self.used]);self.bytes=self.bytes.checked_add(self.used).ok_or(io::ErrorKind::InvalidData)?;self.used=0;}Ok(())}
    fn word(&mut self,word:&[u8])->io::Result<()>{if self.used+word.len()>self.buffer.len(){self.flush_chunk()?}self.buffer[self.used..self.used+word.len()].copy_from_slice(word);self.used+=word.len();Ok(())}
    fn finish(mut self)->io::Result<(usize,HexDigest)>{self.flush_chunk()?;Self::checkpoint()?;let digest:[u8;32]=self.hash.finalize().into();Ok((self.bytes,HexDigest(digest)))}
}
/// Exact create-new regular destination; no implicit directories or densification.
/// Execution deadline excludes final encoding/export, as in scientific-v1.
/// Stop/IO may leave a partial file; complete footer+digest is required, no fsync promise.
fn export<'a>(name:&'a str,path:&Path,value:&V,source:&Source)->ExportResult<'a>{
    let mut result=ExportResult{binding:name,status:"error",complete:false,payload_bytes:None,sha256:None,code:Some("ExportIo")};
    let operation=(||->io::Result<HexDigest>{
        PayloadWriter::checkpoint()?;let mut metadata=Reply::new(EXPORT_META)?;let axes=value.axes();
        let expected=match value.storage_view(){
            ScientificStorageViewV2::Dense(d)=>{let unit=if value.class()==ProgramValueClassV1::Logical{1}else if d.is_complex(){16}else{8};let bytes=d.len().checked_mul(unit).filter(|n|*n<=(16<<20)).ok_or(io::ErrorKind::InvalidData)?;metadata.json(&ExportHeader{format:"onl-scientific-array-v1",class:if value.class()==ProgramValueClassV1::Double{"double"}else{"logical"},complex:value.is_complex(),shape:&axes,elements:d.len(),component_encoding:if value.class()==ProgramValueClassV1::Double{"ieee754-binary64-le"}else{"uint8-bool"},storage:"column-major-interleaved",binding:name,source_sha256:&source.sha256})?;bytes},
            ScientificStorageViewV2::Csc(c)=>{let cp=c.column_pointers().len().checked_mul(4).ok_or(io::ErrorKind::InvalidData)?;let ri=c.row_indices().len().checked_mul(4).ok_or(io::ErrorKind::InvalidData)?;let components=if c.is_complex(){2}else{1};let values=c.nnz().checked_mul(components*8).ok_or(io::ErrorKind::InvalidData)?;let value_offset=cp.checked_add(ri).ok_or(io::ErrorKind::InvalidData)?;let bytes=value_offset.checked_add(values).filter(|n|*n<=(16<<20)).ok_or(io::ErrorKind::InvalidData)?;let sections=[CscSection{name:"column_pointers",encoding:"u32-le",elements:c.column_pointers().len(),components:None,bytes:cp,offset_bytes:0},CscSection{name:"row_indices",encoding:"u32-le",elements:c.nnz(),components:None,bytes:ri,offset_bytes:cp},CscSection{name:"values",encoding:"ieee754-binary64-le",elements:c.nnz(),components:Some(components),bytes:values,offset_bytes:value_offset}];metadata.json(&CscHeader{format:"onl-scientific-csc-v1",class:"double",complex:c.is_complex(),shape:&axes,logical_elements:value.logical_numel(),nnz:c.nnz(),nzmax:c.nzmax(),index_base:0,storage:"csc",sections,payload_bytes:bytes,binding:name,source_sha256:&source.sha256})?;bytes}
        };
        let mut options=OpenOptions::new();options.write(true).create_new(true);
        #[cfg(unix)]{use std::os::unix::fs::OpenOptionsExt;options.custom_flags(libc::O_NOFOLLOW|libc::O_NONBLOCK|libc::O_CLOEXEC);}
        let mut file=options.open(path)?;if !file.metadata()?.is_file(){return Err(io::ErrorKind::InvalidInput.into())}file.write_all(&metadata.bytes)?;
        let mut writer=PayloadWriter::new(&mut file);
        match value.storage_view(){
            ScientificStorageViewV2::Dense(d)=>for start in (0..d.len()).step_by(128){PayloadWriter::checkpoint()?;let count=(d.len()-start).min(128);let range=d.read_range(start,count).map_err(|_|io::ErrorKind::InvalidData)?;match range{ScientificReadRangeV1::Real(v)=>for x in v{writer.word(&x.to_bits().to_le_bytes())?},ScientificReadRangeV1::Complex(v)=>for x in v{writer.word(&x.re.to_bits().to_le_bytes())?;writer.word(&x.im.to_bits().to_le_bytes())?},ScientificReadRangeV1::Logical(v)=>writer.word(v)?}},
            ScientificStorageViewV2::Csc(c)=>{for chunk in c.column_pointers().chunks(128){PayloadWriter::checkpoint()?;for x in chunk{writer.word(&x.to_le_bytes())?}}for chunk in c.row_indices().chunks(128){PayloadWriter::checkpoint()?;for x in chunk{writer.word(&x.to_le_bytes())?}}match c.values(){ScientificCscValuesV1::Real(v)=>for chunk in v.chunks(128){PayloadWriter::checkpoint()?;for x in chunk{writer.word(&x.to_bits().to_le_bytes())?}},ScientificCscValuesV1::Complex(v)=>for chunk in v.chunks(128){PayloadWriter::checkpoint()?;for x in chunk{writer.word(&x.re.to_bits().to_le_bytes())?;writer.word(&x.im.to_bits().to_le_bytes())?}}}}
        }
        let(bytes,digest)=writer.finish()?;if bytes!=expected{return Err(io::ErrorKind::InvalidData.into())}result.payload_bytes=Some(bytes);metadata.json(&ExportFooter{complete:true,payload_bytes:bytes,sha256:HexDigest(digest.0)})?;PayloadWriter::checkpoint()?;file.write_all(b"\n")?;file.write_all(&metadata.bytes)?;file.flush()?;Ok(digest)
    })();
    match operation{Ok(digest)=>{result.status="complete";result.complete=true;result.sha256=Some(digest);result.code=None},Err(e)=>{result.payload_bytes=None;result.code=Some(match e.kind(){io::ErrorKind::AlreadyExists=>"DestinationExists",io::ErrorKind::Interrupted=>"Interrupted",io::ErrorKind::OutOfMemory=>"Allocation",_=>"ExportIo"});}}result
}
#[derive(Serialize)]
struct ScientificLimits{transition_fuel:u64,primitive_fuel:u64,max_rank:usize,max_elements:usize,max_value_bytes:usize,max_live_payload_bytes:usize,max_metadata_bytes:usize,max_sparse_axis:u64,max_sparse_logical_elements:u64,max_sparse_nnz:usize}
#[derive(Serialize)]
struct ScientificReady<'a>{schema_version:u8,kind:&'static str,backend:&'static str,profile:&'static str,event:&'static str,source:&'a Source,key:Key,context:Context,limits:ScientificLimits}
pub(crate) fn run(path:&Path,json:bool,workspace:bool,timeout:Option<Duration>,transition_fuel:Option<u64>,primitive_fuel:Option<u64>,requested_export:Option<(&str,&Path)>)->i32{
    let mut reply=match Reply::new(REPLY_LIMIT){Ok(reply)=>reply,Err(_)=>{let message=if json{"{\"schema_version\":3,\"kind\":\"mathlab-rust-program\",\"backend\":\"rust\",\"profile\":\"scientific-v2\",\"event\":\"outcome\",\"status\":\"resource\",\"phase\":\"admission\",\"error\":{\"code\":\"Allocation\"},\"events_complete\":false,\"workspace\":null}\n"}else{"Scientific program resource: Allocation\n"};let _=io::stdout().lock().write_all(message.as_bytes());return 1}};
    if requested_export.is_some_and(|(name,_)|name.len()>2048){return early(&mut reply,json,workspace,None,Early::error("admission","InvalidExportName"))}
    let _signal=match signal::SignalGuard::install(){Ok(guard)=>guard,Err(_)=>return early(&mut reply,json,workspace,None,Early::error("admission","SignalSetup"))};
    let PreparedSource{source,module,deadline}=match prepare_file(path,timeout){Ok(source)=>source,Err(failure)=>return early(&mut reply,json,workspace,failure.source.as_ref(),failure.early)};
    let mut session=match Session::new(){Ok(session)=>session,Err(e)=>return early(&mut reply,json,workspace,Some(&source),Early::resource("admission",code(e)))};
    let context=session.context();let options=Options{deadline,transition_fuel:transition_fuel.unwrap_or(5_000_000),primitive_fuel:primitive_fuel.unwrap_or(1_000_000_000)};
    let request=match session.prepare(module,options){Ok(request)=>request,Err(e)=>return early(&mut reply,json,workspace,Some(&source),Early::resource("admission",code(e)))};
    let handle=request.cancel_handle();
    let ready=if json{reply.json(&ScientificReady{schema_version:3,kind:KIND,backend:"rust",profile:PROFILE,event:"ready",source:&source,key:handle.key().into(),context:context.into(),limits:ScientificLimits{transition_fuel:options.transition_fuel,primitive_fuel:options.primitive_fuel,max_rank:8,max_elements:1<<20,max_value_bytes:16<<20,max_live_payload_bytes:64<<20,max_metadata_bytes:16<<20,max_sparse_axis:1<<20,max_sparse_logical_elements:1u64<<40,max_sparse_nnz:1<<20}})}else{writeln!(&mut reply,"Running scientific-v2; Ctrl+C requests Stop.")};
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
