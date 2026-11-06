// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit immutable class graph on the shared AST/executor and input pipeline.
//! Numeric export headers retain the existing entry-source SHA only. Verify the
//! corresponding schema4 outcome/source_graph_sha256 to bind the entire class closure.
use super::*;
use std::path::PathBuf;
use mathlab_engine::rust_program::class::{ClassProgramSessionV1 as Session,ClassProgramOutcomeV1 as Outcome,ClassProgramErrorCodeV1 as Code,ClassProgramOptionsV1 as Options,ClassProgramStatsV1,ClassValueV1,ClassValueViewV1,ClassEventValueV1,ClassObjectKindV1,ClassObjectSnapshotV1,ClassErrorV1};
use mathlab_language::program::{ClassSourceV1,ClassParseErrorCodeV1,parse_class_graph_controlled_v1,ScientificProgramValueV2 as V,ScientificReadRangeV1,ScientificStorageViewV2,ScientificCscValuesV1};
const PROFILE:&str="class-v1";
const MAX_SOURCES:usize=9;
const GRAPH_BYTES:usize=262144;
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

#[derive(Serialize)]struct ObjectIdentity{slot:usize,generation:u64}
#[derive(Serialize)]struct ObjectDescriptor<'a>{storage:&'static str,class_name:&'a str,object_kind:&'static str,identity:ObjectIdentity,valid:bool}
fn object_kind(kind:ClassObjectKindV1)->&'static str{match kind{ClassObjectKindV1::Value=>"value",ClassObjectKindV1::Handle=>"handle"}}
fn object_snapshot(snapshot:&ClassObjectSnapshotV1)->ObjectDescriptor<'_>{ObjectDescriptor{storage:"object",class_name:snapshot.class_name(),object_kind:object_kind(snapshot.kind()),identity:ObjectIdentity{slot:snapshot.slot(),generation:snapshot.generation()},valid:snapshot.is_valid()}}
struct WorkspaceValue<'a>{session:&'a Session,value:&'a ClassValueV1}
impl Serialize for WorkspaceValue<'_>{fn serialize<S:Serializer>(&self,s:S)->Result<S::Ok,S::Error>{
    match self.session.view(self.value).map_err(|_|<S::Error as serde::ser::Error>::custom("ClassViewUnavailable"))?{
        ClassValueViewV1::Scientific(v)=>ValueWindow{value:v,count:WORKSPACE_WINDOW}.serialize(s),
        ClassValueViewV1::Object{class_name,metadata}=>ObjectDescriptor{storage:"object",class_name,object_kind:object_kind(metadata.kind()),identity:ObjectIdentity{slot:metadata.slot(),generation:metadata.generation()},valid:metadata.is_valid()}.serialize(s)
    }
}}
struct Workspace<'a>(Option<&'a Session>);
impl Serialize for Workspace<'_>{fn serialize<S:Serializer>(&self,s:S)->Result<S::Ok,S::Error>{let mut seq=s.serialize_seq(self.0.map(|v|v.bindings().len()))?;if let Some(session)=self.0{for e in session.bindings(){#[derive(Serialize)]struct Entry<'a>{name:&'a str,value:WorkspaceValue<'a>}seq.serialize_element(&Entry{name:e.name,value:WorkspaceValue{session,value:e.value}})?}}seq.end()}}
struct EventValue<'a>(&'a ClassEventValueV1);
impl Serialize for EventValue<'_>{fn serialize<S:Serializer>(&self,s:S)->Result<S::Ok,S::Error>{match self.0{ClassEventValueV1::Scientific(v)=>ValueWindow{value:v,count:EVENT_WINDOW}.serialize(s),ClassEventValueV1::Object(v)=>object_snapshot(v).serialize(s)}}}
struct Events<'a>(&'a Outcome);
impl Serialize for Events<'_>{fn serialize<S:Serializer>(&self,s:S)->Result<S::Ok,S::Error>{let mut seq=s.serialize_seq(Some(self.0.events.len()))?;for e in &self.0.events{#[derive(Serialize)]struct Event<'a>{sequence:u64,location:Location,function:Option<usize>,name:Option<&'a str>,value:EventValue<'a>}seq.serialize_element(&Event{sequence:e.sequence,location:e.location.into(),function:e.function.map(NodeIdV1::index),name:e.name.as_deref(),value:EventValue(&e.value)})?;}seq.end()}}
fn scientific_cause(e:ScientificErrorV1)->&'static str{match e{ScientificErrorV1::Allocation=>"Allocation",ScientificErrorV1::Resource=>"Resource",ScientificErrorV1::Rank=>"Rank",ScientificErrorV1::Shape=>"Shape",ScientificErrorV1::Type=>"Type",ScientificErrorV1::Index=>"Index",ScientificErrorV1::Unsupported=>"UnsupportedFeature",ScientificErrorV1::Singular=>"Singular",ScientificErrorV1::NonFiniteInput=>"NonFiniteInput",ScientificErrorV1::ArithmeticBreakdown=>"ArithmeticBreakdown",ScientificErrorV1::Cancelled=>"Cancelled"}}
fn cause(e:ClassErrorV1)->&'static str{match e{ClassErrorV1::Type=>"Type",ClassErrorV1::Shape=>"Shape",ClassErrorV1::Arity=>"Arity",ClassErrorV1::UnknownClass=>"UnknownClass",ClassErrorV1::UnknownProperty=>"UnknownProperty",ClassErrorV1::UnknownMethod=>"UnknownMethod",ClassErrorV1::InvalidHandle=>"InvalidHandle",ClassErrorV1::CyclicReference=>"CyclicReference",ClassErrorV1::Unsupported=>"UnsupportedFeature",ClassErrorV1::Allocation=>"Allocation",ClassErrorV1::Resource=>"Resource",ClassErrorV1::Cancelled=>"Cancelled",ClassErrorV1::CounterExhausted=>"CounterExhausted",ClassErrorV1::Scientific(e)=>scientific_cause(e)}}
fn code(c:Code)->&'static str{match c{Code::UnknownBinding=>"UnknownBinding",Code::Arity=>"Arity",Code::UnassignedOutput=>"UnassignedOutput",Code::Type=>"Type",Code::Shape=>"Shape",Code::Index=>"Index",Code::UnsupportedFeature=>"UnsupportedFeature",Code::Allocation=>"Allocation",Code::Resource=>"Resource",Code::CounterExhausted=>"CounterExhausted",Code::Cancelled=>"Cancelled",Code::Value(e)=>cause(e)}}
fn parse_class_code(c:ClassParseErrorCodeV1)->&'static str{match c{ClassParseErrorCodeV1::Syntax=>"Syntax",ClassParseErrorCodeV1::Unsupported=>"Unsupported",ClassParseErrorCodeV1::DuplicateName=>"DuplicateName",ClassParseErrorCodeV1::UnknownBase=>"UnknownBase",ClassParseErrorCodeV1::InheritanceCycle=>"InheritanceCycle",ClassParseErrorCodeV1::Resource=>"Resource",ClassParseErrorCodeV1::Allocation=>"Allocation",ClassParseErrorCodeV1::Cancelled=>"Cancelled"}}
#[derive(Serialize)]
struct ClassStats{transitions:u64,primitive_steps:u64,peak_live_payload_bytes:usize,peak_metadata_bytes:usize,solves_completed:u64,last_initial_component_metric:Option<Word>,last_maximum_u_component_metric:Option<Word>,last_solve_scalar_steps:Option<u64>,last_solve_complete:Option<bool>,maximum_growth:Option<Word>,maximum_growth_finite:Option<bool>,last_growth_class:Option<&'static str>,operations_completed:u64,peak_operation_reserved_bytes:usize,sparse_constructors:u64,sparse_products:u64,ledger_poisoned:bool,heap_object_slots:usize,heap_live_objects:usize,heap_payload_bytes:usize,heap_metadata_bytes:usize,heap_reclaimed_objects:u64,handle_mutations:u64,handle_deletions:u64}
impl From<ClassProgramStatsV1> for ClassStats{fn from(v:ClassProgramStatsV1)->Self{Self{transitions:v.transitions,primitive_steps:v.primitive_steps,peak_live_payload_bytes:v.peak_live_payload_bytes,peak_metadata_bytes:v.peak_metadata_bytes,solves_completed:v.solves_completed,last_initial_component_metric:v.last_solve.map(|d|Word(d.initial_metric)),last_maximum_u_component_metric:v.last_solve.map(|d|Word(d.maximum_u_metric)),last_solve_scalar_steps:v.last_solve.map(|d|d.scalar_steps),last_solve_complete:v.last_solve.map(|_|v.last_solve_complete),maximum_growth:v.maximum_growth.map(Word),maximum_growth_finite:v.maximum_growth.map(f64::is_finite),operations_completed:v.operations_completed,peak_operation_reserved_bytes:v.peak_operation_reserved_bytes,sparse_constructors:v.sparse_constructors,sparse_products:v.sparse_products,ledger_poisoned:v.ledger_poisoned,heap_object_slots:v.heap_object_slots,heap_live_objects:v.heap_live_objects,heap_payload_bytes:v.heap_payload_bytes,heap_metadata_bytes:v.heap_metadata_bytes,heap_reclaimed_objects:v.heap_reclaimed_objects,handle_mutations:v.handle_mutations,handle_deletions:v.handle_deletions,last_growth_class:v.last_solve.map(|d|if d.initial_metric==0.{"undefined-zero-initial"}else if d.growth_factor().unwrap().is_finite(){"finite"}else{"overflow"})}}}
#[derive(Serialize)]
struct ExportResult<'a>{binding:&'a str,status:&'static str,complete:bool,payload_bytes:Option<usize>,sha256:Option<HexDigest>,code:Option<&'static str>}

struct Report<'a>{sources:&'a[Source],graph_hash:Option<&'a HexDigest>,graph_complete:bool,outcome:Option<&'a Outcome>,session:Option<&'a Session>,early:Option<Early>,early_source_id:Option<u64>,workspace:bool,complete:bool,export:Option<&'a ExportResult<'a>>}
impl Report<'_>{
    fn status(&self)->&'static str{if let Some(e)=self.early{return e.status}match self.outcome.unwrap().status{ProgramStatusV1::Success=>"success",ProgramStatusV1::Error=>"error",ProgramStatusV1::Resource=>"resource",ProgramStatusV1::Cancelled(_)=>"cancelled"}}
    fn reason(&self)->Option<ProgramCancelReasonV1>{self.early.and_then(|e|e.reason).or_else(||self.outcome.and_then(|o|if let ProgramStatusV1::Cancelled(r)=o.status{Some(r)}else{None}))}
    fn exit(&self)->i32{if self.early.is_some_and(|e|e.phase=="encoding"&&e.code=="ReplyLimit"){return 1}match self.reason(){Some(ProgramCancelReasonV1::UserStop)=>130,Some(ProgramCancelReasonV1::Deadline)=>124,None if self.status()=="success"=>match self.export.and_then(|e|e.code){None=>0,Some("Interrupted")=>130,_=>1},_=>1}}
}
impl Serialize for Report<'_>{fn serialize<S:Serializer>(&self,s:S)->Result<S::Ok,S::Error>{
    let mut out=s.serialize_struct("ClassOutcome",24)?;out.serialize_field("schema_version",&4)?;out.serialize_field("kind",KIND)?;out.serialize_field("backend","rust")?;out.serialize_field("profile",PROFILE)?;out.serialize_field("event","outcome")?;out.serialize_field("status",self.status())?;out.serialize_field("phase",self.early.map(|e|e.phase).unwrap_or("execution"))?;out.serialize_field("source",&self.sources.first())?;out.serialize_field("source_graph",self.sources)?;out.serialize_field("source_graph_sha256",&self.graph_hash)?;out.serialize_field("source_graph_complete",&self.graph_complete)?;
    out.serialize_field("key",&self.outcome.map(|o|Key::from(o.key)))?;out.serialize_field("before",&self.outcome.map(|o|Context::from(o.start)))?;out.serialize_field("after",&self.outcome.map(|o|Context::from(o.final_context)))?;out.serialize_field("effect_sequence",&self.outcome.map_or(0,|o|o.committed_effect_sequence))?;out.serialize_field("completed_leaf_statement_count",&self.outcome.map_or(0,|o|o.completed_leaf_statement_count))?;out.serialize_field("stats",&self.outcome.map(|o|ClassStats::from(o.stats)))?;out.serialize_field("reason",&self.reason().map(super::reason))?;out.serialize_field("events_complete",&self.complete)?;out.serialize_field("events",&self.outcome.filter(|_|self.complete).map(Events))?;out.serialize_field("workspace",&if self.workspace&&self.complete{Some(Workspace(self.session))}else{None})?;
    #[derive(Serialize)]struct Error{stage:&'static str,code:&'static str,source_id:Option<u64>,location:Option<Location>,call_stack:[Option<Location>;32],call_depth:usize,cause_code:Option<&'static str>}
    let error=if let Some(e)=self.early{Some(Error{stage:e.phase,code:e.code,source_id:self.early_source_id,location:e.span.map(|span|Location{source_id:self.early_source_id.unwrap_or(1),span:span.into()}),call_stack:[None;32],call_depth:0,cause_code:None})}else{self.outcome.and_then(|o|o.error).map(|e|Error{stage:"execution",code:code(e.code),source_id:Some(e.location.source_id),location:Some(e.location.into()),call_stack:e.call_stack.map(|x|x.map(Location::from)),call_depth:e.call_depth,cause_code:if let Code::Value(c)=e.code{Some(cause(c))}else{None}})};
    out.serialize_field("error",&error)?;out.serialize_field("export",&self.export)?;out.serialize_field("command_exit",&self.exit())?;out.end()
}}
fn text_numeric(out:&mut Reply,name:&str,v:&V,window:usize)->io::Result<()>{
    writeln!(out,"{name}: {}{} {} rank{} {:?}",if v.class()==ProgramValueClassV1::Double{"double"}else{"logical"},if v.is_complex(){" complex"}else{""},if v.is_sparse(){"CSC"}else{"dense"},v.rank(),&*v.axes())?;
    let count=v.stored_len().min(window);
    match v.storage_view(){
        ScientificStorageViewV2::Dense(d)=>for i in 0..count{let z=d.get(i).unwrap();if d.is_complex(){writeln!(out,"  [{i}] {:.16e} {:+.16e}i",z.re,z.im)?}else{writeln!(out,"  [{i}] {:.16e}",z.re)?}},
        ScientificStorageViewV2::Csc(c)=>{writeln!(out,"  nnz{} nzmax{}; zero-based stored-entry window",c.nnz(),c.nzmax())?;for i in 0..count{let col=c.column_pointers().partition_point(|&p|p as usize<=i)-1;let row=c.row_indices()[i];match c.values(){ScientificCscValuesV1::Real(z)=>writeln!(out,"  ({row},{col}) {:.16e}",z[i])?,ScientificCscValuesV1::Complex(z)=>writeln!(out,"  ({row},{col}) {:.16e} {:+.16e}i",z[i].re,z[i].im)?}}}
    }
    if count<v.stored_len(){writeln!(out,"  stored window0..{} of{}; stored_entries_complete=false",count,v.stored_len())?}Ok(())
}

fn text_object(out:&mut Reply,name:&str,v:ObjectDescriptor<'_>)->io::Result<()>{writeln!(out,"{name}: {} {} identity{}:{} valid={}",v.class_name,v.object_kind,v.identity.slot,v.identity.generation,v.valid)}
fn finish(reply:&mut Reply,json:bool,report:Report<'_>)->i32{
    let exit=report.exit();let encoded=if json{reply.json(&report)}else{(||{if let Some(out)=report.outcome{if report.complete{for e in &out.events{match &e.value{ClassEventValueV1::Scientific(v)=>text_numeric(reply,e.name.as_deref().unwrap_or("ans"),v,EVENT_WINDOW)?,ClassEventValueV1::Object(v)=>text_object(reply,e.name.as_deref().unwrap_or("ans"),object_snapshot(v))?}}}}if report.workspace&&report.complete{if let Some(session)=report.session{for e in session.bindings(){match session.view(e.value).map_err(|_|io::ErrorKind::InvalidData)?{ClassValueViewV1::Scientific(v)=>text_numeric(reply,e.name,v,WORKSPACE_WINDOW)?,ClassValueViewV1::Object{class_name,metadata}=>text_object(reply,e.name,ObjectDescriptor{storage:"object",class_name,object_kind:object_kind(metadata.kind()),identity:ObjectIdentity{slot:metadata.slot(),generation:metadata.generation()},valid:metadata.is_valid()})?}}}}if let Some(e)=report.outcome.and_then(|o|o.error){writeln!(reply,"{} at source{} line{} bytes{}..{}",code(e.code),e.location.source_id,e.location.span.start_line,e.location.span.start,e.location.span.end)?;}if let Some(e)=report.early{writeln!(reply,"{} during{} source{:?}",e.code,e.phase,report.early_source_id)?;}if let Some(e)=report.export{writeln!(reply,"Export {}: {}{}",e.binding,e.status,e.code.unwrap_or(""))?;}writeln!(reply,"Class program {}: {} effects.",report.status(),report.outcome.map_or(0,|o|o.committed_effect_sequence))})()};
    if encoded.is_err(){let fallback=Report{early:Some(Early::resource("encoding","ReplyLimit")),complete:false,..report};let encoded=if json{reply.json(&fallback)}else{reply.bytes.clear();writeln!(reply,"Class program resource: ReplyLimit (output incomplete); effects{}; completed{}; revision{}.",fallback.outcome.map_or(0,|o|o.committed_effect_sequence),fallback.outcome.map_or(0,|o|o.completed_leaf_statement_count),fallback.outcome.map_or(0,|o|o.final_context.revision))};if encoded.is_err(){return 1}if emit(reply).is_err(){return 1}return 1}if emit(reply).is_err(){1}else{exit}
}
fn early(reply:&mut Reply,json:bool,workspace:bool,sources:&[Source],hash:Option<&HexDigest>,graph_complete:bool,source_id:Option<u64>,e:Early)->i32{finish(reply,json,Report{sources,graph_hash:hash,graph_complete,outcome:None,session:None,early:Some(e),early_source_id:source_id,workspace,complete:true,export:None})}
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
// Preserve numeric wire bytes: source_sha256 binds only the entry source, not modules.
// Full closure identity is supplied by the corresponding schema4 outcome graph hash.
fn export_numeric<'a>(name:&'a str,path:&Path,value:&V,source:&Source)->ExportResult<'a>{
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

struct SourceGraph{descriptors:Vec<Source>,texts:Vec<ClassSourceV1>,hash:HexDigest}
struct GraphFailure{descriptors:Vec<Source>,source_id:Option<u64>,early:Early}
fn source_graph_hash(sources:&[Source])->HexDigest{let mut hash=Sha256::new();for s in sources{hash.update((s.path_display.len() as u64).to_le_bytes());hash.update(s.path_display.as_bytes());hash.update(s.id.to_le_bytes());hash.update((s.bytes as u64).to_le_bytes());hash.update(s.sha256.0);}HexDigest(hash.finalize().into())}
/// Exactly supplied paths, regular/nonblocking shared source loader, no search.
/// Input symlinks resolving to regular files retain shared-loader policy.
fn load_graph(entry:&Path,modules:&[PathBuf])->Result<SourceGraph,GraphFailure>{
    let mut descriptors=Vec::new();let mut texts=Vec::new();let mut canonical=Vec::new();
    if modules.len()>=MAX_SOURCES{return Err(GraphFailure{descriptors,source_id:None,early:Early::resource("source","SourceGraphLimit")})}
    if descriptors.try_reserve_exact(MAX_SOURCES).is_err()||texts.try_reserve_exact(MAX_SOURCES).is_err()||canonical.try_reserve_exact(MAX_SOURCES).is_err(){return Err(GraphFailure{descriptors,source_id:None,early:Early::resource("admission","Allocation")})}
    if descriptors.capacity()>MAX_SOURCES||texts.capacity()>MAX_SOURCES||canonical.capacity()>MAX_SOURCES{return Err(GraphFailure{descriptors,source_id:None,early:Early::resource("admission","Allocation")})}
    let mut total=0usize;
    for (index,path) in std::iter::once(entry).chain(modules.iter().map(PathBuf::as_path)).enumerate(){
        let source_id=index as u64+1;
        // canonicalize and path formatting allocate inside the host standard library;
        // these boundaries do not claim fully fallible allocation handling.
        let physical=match path.canonicalize(){Ok(p)=>p,Err(_)=>return Err(GraphFailure{descriptors,source_id:Some(source_id),early:Early::error("source","SourceIo")})};
        if canonical.contains(&physical){return Err(GraphFailure{descriptors,source_id:Some(source_id),early:Early::error("source","DuplicateSource")})}
        let(mut descriptor,text)=match load(path){Ok(v)=>v,Err(mut failure)=>{if let Some(mut s)=failure.source.take(){s.id=source_id;descriptors.push(s)}return Err(GraphFailure{descriptors,source_id:Some(source_id),early:failure.early})}};descriptor.id=source_id;
        total=match total.checked_add(descriptor.bytes){Some(n)if n<=GRAPH_BYTES=>n,_=>return Err(GraphFailure{descriptors,source_id:Some(source_id),early:Early::resource("source","SourceGraphLimit")})};
        canonical.push(physical);descriptors.push(descriptor);texts.push(ClassSourceV1{source_id,text});
    }
    let hash=source_graph_hash(&descriptors);Ok(SourceGraph{descriptors,texts,hash})
}
#[derive(Serialize)]struct ClassLimits{transition_fuel:u64,primitive_fuel:u64,max_sources:usize,max_file_bytes:usize,max_source_bytes:usize,max_classes:usize,max_properties:usize,max_rank:usize,max_elements:usize,max_value_bytes:usize,max_live_payload_bytes:usize,max_metadata_bytes:usize}
#[derive(Serialize)]struct ClassReady<'a>{schema_version:u8,kind:&'static str,backend:&'static str,profile:&'static str,event:&'static str,source:&'a Source,source_graph:&'a[Source],source_graph_sha256:&'a HexDigest,source_graph_complete:bool,key:Key,context:Context,limits:ClassLimits}

// Compiled only into an explicitly instrumented diagnostic executable. It adds
// no normal-profile option, wire event or environment read.
#[cfg(onl_qr_phase_probe)]
mod qr_phase_probe {
 use super::*;
 use std::{cell::RefCell,io::{Read,Write},os::unix::{net::UnixStream,fs::{FileTypeExt,MetadataExt,PermissionsExt}},time::Instant};
 struct State{stream:UnixStream,key:ProgramRequestKeyV1,deadline:Option<Instant>,_path:PathBuf}
 thread_local!{static STATE:RefCell<Option<State>>=const{RefCell::new(None)};}
 pub(super) struct Guard{_observer:mathlab_language::program::ScientificQrPhaseProbeGuardV1}
 impl Drop for Guard{fn drop(&mut self){STATE.with(|s|{s.borrow_mut().take();});}}
 fn connect_bounded(path:&Path)->io::Result<UnixStream>{
  use std::os::{fd::{AsRawFd,FromRawFd},unix::ffi::OsStrExt};
  let raw=unsafe{libc::socket(libc::AF_UNIX,libc::SOCK_STREAM,0)};if raw<0{return Err(io::Error::last_os_error())}
  let stream=unsafe{UnixStream::from_raw_fd(raw)};stream.set_nonblocking(true)?;
  let mut address:libc::sockaddr_un=unsafe{std::mem::zeroed()};address.sun_family=libc::AF_UNIX as _;let bytes=path.as_os_str().as_bytes();if bytes.len()>=address.sun_path.len(){return Err(io::ErrorKind::InvalidInput.into())}
  for(dst,src)in address.sun_path.iter_mut().zip(bytes){*dst=*src as _;}
  let length=std::mem::offset_of!(libc::sockaddr_un,sun_path)+bytes.len()+1;
  #[cfg(any(target_os="macos",target_os="ios",target_os="freebsd"))]{address.sun_len=length as _;}
  let connected=unsafe{libc::connect(stream.as_raw_fd(),(&address as *const libc::sockaddr_un).cast(),length as libc::socklen_t)};
  if connected<0{let error=io::Error::last_os_error();if !matches!(error.raw_os_error(),Some(libc::EINPROGRESS)|Some(libc::EAGAIN)){return Err(error)}
   let mut fd=libc::pollfd{fd:stream.as_raw_fd(),events:libc::POLLOUT,revents:0};let ready=unsafe{libc::poll(&mut fd,1,2000)};if ready<=0{return Err(if ready==0{io::ErrorKind::TimedOut.into()}else{io::Error::last_os_error()})}
   let mut status:libc::c_int=0;let mut size=std::mem::size_of_val(&status)as libc::socklen_t;if unsafe{libc::getsockopt(stream.as_raw_fd(),libc::SOL_SOCKET,libc::SO_ERROR,(&mut status as *mut libc::c_int).cast(),&mut size)}<0{return Err(io::Error::last_os_error())}if status!=0{return Err(io::Error::from_raw_os_error(status))}
  }stream.set_nonblocking(false)?;Ok(stream)
 }
 pub(super) fn install(key:ProgramRequestKeyV1,deadline:Option<Instant>)->Option<Guard>{
  let raw=std::env::var_os("ONL_QR_PHASE_SOCKET")?;let path=PathBuf::from(raw);
  assert!(path.is_absolute()&&path.as_os_str().len()<=100&&path.capacity()<=256,"diagnostic socket path/capacity bound");
  let metadata=std::fs::symlink_metadata(&path).expect("diagnostic owned socket");
  assert!(metadata.file_type().is_socket());assert_eq!(metadata.uid(),unsafe{libc::geteuid()});
  let parent=std::fs::symlink_metadata(path.parent().unwrap()).unwrap();assert!(parent.is_dir());assert_eq!(parent.uid(),unsafe{libc::geteuid()});assert_eq!(parent.permissions().mode()&0o077,0,"diagnostic socket directory private");
  let stream=connect_bounded(&path).expect("diagnostic bounded connect");stream.set_write_timeout(Some(Duration::from_secs(2))).unwrap();
  STATE.with(|s|{let mut s=s.borrow_mut();assert!(s.is_none(),"diagnostic phase guard reentry");*s=Some(State{stream,key,deadline,_path:path});});
  Some(Guard{_observer:mathlab_language::program::install_scientific_qr_phase_probe_v1(observe)})
 }
 fn observe(phase:mathlab_language::program::ScientificQrPhaseV1){
  assert_eq!(phase,mathlab_language::program::ScientificQrPhaseV1::ApplyB);
  STATE.with(|cell|{let mut borrow=cell.borrow_mut();let state=borrow.as_mut().expect("diagnostic guard lifetime");
   let mut frame=[0u8;32];frame[..8].copy_from_slice(b"ONLQRP1\0");for(i,v)in[state.key.session_id,state.key.epoch,state.key.request_id].into_iter().enumerate(){frame[8+i*8..16+i*8].copy_from_slice(&v.to_le_bytes());}
   state.stream.write_all(&frame).expect("diagnostic bounded phase frame");
   let hard=Instant::now().checked_add(Duration::from_secs(10)).unwrap();let until=state.deadline.map_or(hard,|d|d.min(hard));let mut release=[0u8;8];let mut used=0;
   loop{let now=Instant::now();if now>=until{assert!(state.deadline.is_some_and(|d|now>=d),"diagnostic release safety ceiling");return}
    state.stream.set_read_timeout(Some((until-now).max(Duration::from_millis(1)))).unwrap();
    match state.stream.read(&mut release[used..]){Ok(0)=>panic!("diagnostic peer closed before release"),Ok(n)=>{used+=n;if used==8{assert_eq!(&release,b"RELEASE1");return}},Err(e)if matches!(e.kind(),io::ErrorKind::Interrupted|io::ErrorKind::WouldBlock|io::ErrorKind::TimedOut)=>{},Err(e)=>panic!("diagnostic release IO: {e}")}
   }
  });
 }
}

pub(crate) fn run(path:&Path,module_files:&[PathBuf],json:bool,workspace:bool,timeout:Option<Duration>,transition_fuel:Option<u64>,primitive_fuel:Option<u64>,requested_export:Option<(&str,&Path)>)->i32{
    let mut reply=match Reply::new(REPLY_LIMIT){Ok(r)=>r,Err(_)=>{let text=if json{"{\"schema_version\":4,\"kind\":\"mathlab-rust-program\",\"backend\":\"rust\",\"profile\":\"class-v1\",\"event\":\"outcome\",\"status\":\"resource\",\"phase\":\"admission\",\"error\":{\"code\":\"Allocation\"},\"source_graph_complete\":false,\"source_graph_sha256\":null,\"events_complete\":false,\"workspace\":null}\n"}else{"Class program resource: Allocation\n"};let _=io::stdout().lock().write_all(text.as_bytes());return 1}};
    if requested_export.is_some_and(|(name,_)|name.len()>2048){return early(&mut reply,json,workspace,&[],None,false,None,Early::error("admission","InvalidExportName"))}
    let _signal=match signal::SignalGuard::install(){Ok(g)=>g,Err(_)=>return early(&mut reply,json,workspace,&[],None,false,None,Early::error("admission","SignalSetup"))};
    let SourceGraph{descriptors,texts,hash}=match load_graph(path,module_files){Ok(g)=>g,Err(f)=>return early(&mut reply,json,workspace,&f.descriptors,None,false,f.source_id,f.early)};
    // All input IO precedes the same cooperative parse/setup/ready/execution clock.
    let deadline=match timeout.map(|d|Instant::now().checked_add(d)){Some(None)=>return early(&mut reply,json,workspace,&descriptors,Some(&hash),true,None,Early::error("admission","InvalidDeadline")),Some(Some(d))=>Some(d),None=>None};
    let mut cancel=None;
    let graph=parse_class_graph_controlled_v1(texts,||{if cancel.is_none(){if signal::requested(){cancel=Some(ProgramCancelReasonV1::UserStop)}else if deadline.is_some_and(|d|Instant::now()>=d){cancel=Some(ProgramCancelReasonV1::Deadline)}}cancel.is_some()});
    let graph=match graph{Ok(g)=>Arc::new(g),Err(e)=>{let diagnostic=if let Some(reason)=cancel{Early{span:Some(e.span),..Early::cancelled("parse",reason)}}else{Early{span:Some(e.span),..if matches!(e.code,ClassParseErrorCodeV1::Resource|ClassParseErrorCodeV1::Allocation){Early::resource("parse",parse_class_code(e.code))}else{Early::error("parse",parse_class_code(e.code))}}};return early(&mut reply,json,workspace,&descriptors,Some(&hash),true,Some(e.source_id),diagnostic)}};
    let mut session=match Session::new(){Ok(s)=>s,Err(e)=>return early(&mut reply,json,workspace,&descriptors,Some(&hash),true,None,Early::resource("admission",code(e)))};
    let context=session.context();let options=Options{deadline,transition_fuel:transition_fuel.unwrap_or(5_000_000),primitive_fuel:primitive_fuel.unwrap_or(1_000_000_000)};
    let request=match session.prepare(graph,options){Ok(r)=>r,Err(e)=>return early(&mut reply,json,workspace,&descriptors,Some(&hash),true,None,Early::resource("admission",code(e)))};let handle=request.cancel_handle();
    #[cfg(onl_qr_phase_probe)]
    let _qr_phase_probe=qr_phase_probe::install(handle.key(),options.deadline);
    let ready=if json{reply.json(&ClassReady{schema_version:4,kind:KIND,backend:"rust",profile:PROFILE,event:"ready",source:&descriptors[0],source_graph:&descriptors,source_graph_sha256:&hash,source_graph_complete:true,key:handle.key().into(),context:context.into(),limits:ClassLimits{transition_fuel:options.transition_fuel,primitive_fuel:options.primitive_fuel,max_sources:9,max_file_bytes:65536,max_source_bytes:262144,max_classes:8,max_properties:64,max_rank:8,max_elements:1<<20,max_value_bytes:16<<20,max_live_payload_bytes:64<<20,max_metadata_bytes:16<<20}})}else{writeln!(&mut reply,"Running class-v1; Ctrl+C requests Stop.")};
    if ready.is_err(){drop(request);return early(&mut reply,json,workspace,&descriptors,Some(&hash),true,None,Early::resource("encoding","ReplyLimit"))}if emit(&reply).is_err(){return 1}reply.limit-=reply.bytes.len();reply.bytes.clear();
    let outcome=request.execute_controlled(|_,handle|{if signal::requested(){handle.request_stop();}});
    let export_result=requested_export.map(|(name,path)|{if outcome.status!=ProgramStatusV1::Success{ExportResult{binding:name,status:"skipped",complete:false,payload_bytes:None,sha256:None,code:Some("ExecutionNotSuccessful")}}else if let Some(value)=session.bindings().find(|b|b.name==name).map(|b|b.value){match value{ClassValueV1::Scientific(v)=>export_numeric(name,path,v,&descriptors[0]),ClassValueV1::Object(_)=>ExportResult{binding:name,status:"error",complete:false,payload_bytes:None,sha256:None,code:Some("UnsupportedObjectExport")}}}else{ExportResult{binding:name,status:"error",complete:false,payload_bytes:None,sha256:None,code:Some("UnknownBinding")}}});
    finish(&mut reply,json,Report{sources:&descriptors,graph_hash:Some(&hash),graph_complete:true,outcome:Some(&outcome),session:Some(&session),early:None,early_source_id:None,workspace,complete:true,export:export_result.as_ref()})
}

#[cfg(test)]
mod class_codec_tests{
 use super::*;
 #[test]
 fn object_event_codec_owns_metadata_after_delete_and_session_drop(){
  let sources=vec![ClassSourceV1{source_id:1,text:Arc::from("h=CodecGauge;h\ndelete(h);")},ClassSourceV1{source_id:2,text:Arc::from("classdef CodecGauge < handle\nproperties\nReading=4\nend\nend\n")}];
  let graph=Arc::new(parse_class_graph_controlled_v1(sources,||false).unwrap());let mut session=Session::new().unwrap();let outcome=session.prepare(graph,Options::default()).unwrap().execute();assert_eq!(outcome.status,ProgramStatusV1::Success,"located class error: {:?}",outcome.error);
  drop(session);
  let event=outcome.events.iter().find(|e|matches!(&e.value,ClassEventValueV1::Object(_))).unwrap();let row=serde_json::to_value(EventValue(&event.value)).unwrap();assert_eq!(row["storage"],"object");assert_eq!(row["class_name"],"CodecGauge");assert_eq!(row["object_kind"],"handle");assert_eq!(row["valid"],true);assert!(row["identity"]["generation"].is_u64());assert!(row.get("binary64_components").is_none());
 }

 #[test]
 fn schema4_reply_limit_preserves_cancel_reason_but_prioritizes_exit_one(){
  // Synthetic cancellation status isolates codec priority from scheduling/timing.
  // The complete report exceeds 4KiB; its bounded fallback must still fit.
  for reason in [ProgramCancelReasonV1::UserStop,ProgramCancelReasonV1::Deadline]{
   let graph=Arc::new(parse_class_graph_controlled_v1(vec![ClassSourceV1{source_id:1,text:Arc::from("x=1:300;x\nh=FallbackGauge;h\n")},ClassSourceV1{source_id:2,text:Arc::from("classdef FallbackGauge < handle\nproperties\nReading=4\nend\nend\n")}],||false).unwrap());
   let mut session=Session::new().unwrap();let mut outcome=session.prepare(graph,Options::default()).unwrap().execute();assert_eq!(outcome.status,ProgramStatusV1::Success);outcome.status=ProgramStatusV1::Cancelled(reason);
   let report=Report{sources:&[],graph_hash:None,graph_complete:true,outcome:Some(&outcome),session:Some(&session),early:None,early_source_id:None,workspace:true,complete:true,export:None};
   assert_eq!(report.exit(),if reason==ProgramCancelReasonV1::UserStop{130}else{124});
   assert!(serde_json::to_vec(&report).unwrap().len()>4096);
   let mut reply=Reply::new(4096).unwrap();assert_eq!(finish(&mut reply,true,report),1);
   let row:serde_json::Value=serde_json::from_slice(&reply.bytes).unwrap();
   assert_eq!(row["schema_version"],4);assert_eq!(row["profile"],"class-v1");assert_eq!(row["status"],"resource");assert_eq!(row["phase"],"encoding");assert_eq!(row["error"]["code"],"ReplyLimit");assert_eq!(row["reason"],super::super::reason(reason));assert_eq!(row["command_exit"],1);assert_eq!(row["events_complete"],false);assert!(row["events"].is_null());assert!(row["workspace"].is_null());assert_eq!(row["effect_sequence"],outcome.committed_effect_sequence);assert_eq!(row["after"]["revision"],outcome.final_context.revision);
  }
 }
}
