use std::fs;
use std::path::PathBuf;
use async_trait::async_trait;
use serde_json::{json, Value};
use crate::orchestrator::tool::Tool;
use crate::orchestrator::tools::ToolInput;

pub struct FsLs;
#[async_trait] impl Tool for FsLs {
    fn name(&self) -> &str { "fs_ls" }
    fn description(&self) -> &str { "List files and directories" }
    fn input_schema(&self) -> Value { json!({"type":"object","properties":{"path":{"type":"string"}},"required":[]}) }
    async fn execute(&self, input: Value) -> Result<Value, String> {
        let i: ToolInput = serde_json::from_value(input).map_err(|e|e.to_string())?;
        let p = i.path.as_deref().unwrap_or(".");
        let fp = PathBuf::from(p);
        if fp.is_absolute() { return Err("Absolute paths not allowed".into()); }
        let ents = fs::read_dir(&fp).map_err(|e|e.to_string())?;
        let mut items: Vec<Value> = Vec::new();
        for e in ents.flatten() { let pt=e.path(); items.push(json!({"name":e.file_name().to_string_lossy(),"type":if pt.is_dir(){"directory"}else{"file"}})); }
        Ok(json!({"path":p,"entries":items}))
    }
}

pub struct FsCat;
#[async_trait] impl Tool for FsCat {
    fn name(&self) -> &str { "fs_cat" }
    fn description(&self) -> &str { "Read file contents" }
    fn input_schema(&self) -> Value { json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}) }
    async fn execute(&self, input: Value) -> Result<Value, String> {
        let i: ToolInput = serde_json::from_value(input).map_err(|e|e.to_string())?;
        let p = i.path.ok_or("path required")?;
        let fp = PathBuf::from(&p);
        if fp.is_absolute() { return Err("Absolute paths not allowed".into()); }
        if !fp.is_file() { return Err("Not a file".into()); }
        let c = fs::read_to_string(&fp).map_err(|e|e.to_string())?;
        let t = if c.len()>50000{format!("{}...[truncated]",&c[..50000])}else{c};
        Ok(json!({"path":p,"content":t,"size":fs::metadata(&fp).map(|m|m.len()).unwrap_or(0)}))
    }
}

pub struct FsGrep;
#[async_trait] impl Tool for FsGrep {
    fn name(&self) -> &str { "fs_grep" }
    fn description(&self) -> &str { "Search for pattern in files (no shell)" }
    fn input_schema(&self) -> Value { json!({"type":"object","properties":{"pattern":{"type":"string"},"path":{"type":"string"},"extensions":{"type":"array","items":{"type":"string"}}},"required":["pattern"]}) }
    async fn execute(&self, input: Value) -> Result<Value, String> {
        let i: ToolInput = serde_json::from_value(input).map_err(|e|e.to_string())?;
        let pat = i.pattern.ok_or("pattern required")?;
        let p = i.path.as_deref().unwrap_or(".");
        let fp = PathBuf::from(p);
        if fp.is_absolute() { return Err("Absolute paths not allowed".into()); }
        let exts: Vec<String> = i.args.as_ref().and_then(|a|a.get("extensions")).and_then(|v|v.as_array()).map(|a|a.iter().filter_map(|v|v.as_str().map(String::from)).collect()).unwrap_or_default();
        let re = regex::Regex::new(&pat).map_err(|e|e.to_string())?;
        let mut matches = Vec::new();
        collect(&fp,&re,&exts,&mut matches);
        Ok(json!({"pattern":pat,"path":p,"matches":matches,"total":matches.len()}))
    }
}

fn collect(dir:&std::path::Path,re:&regex::Regex,exts:&[String],m:&mut Vec<Value>){
    let rd = match fs::read_dir(dir){Ok(r)=>r,Err(_)=>return};
    for e in rd.flatten(){
        let pt=e.path();
        if pt.is_dir(){let n=pt.file_name().unwrap_or_default().to_string_lossy();if!n.starts_with('.')&&n!="target"&&n!="node_modules"&&n!="build"{collect(&pt,re,exts,m)}}else if pt.is_file(){
            let ex=pt.extension().and_then(|e|e.to_str()).unwrap_or("");
            if!exts.is_empty()&&!exts.iter().any(|x|x.as_str()==ex){continue}
            let cnt = match fs::read_to_string(&pt){Ok(c)=>c,Err(_)=>continue};
            let mut fm = Vec::new();
            for(ln,lnt)in cnt.lines().enumerate(){if re.is_match(lnt){fm.push(json!({"line":ln+1,"content":lnt}))}}
            if!fm.is_empty(){m.push(json!({"file":pt.to_string_lossy(),"lines":fm}))}
        }
    }
}