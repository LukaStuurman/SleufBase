//! Managed local vLLM executable. Never installs a runtime; model resolution
//! remains the external executable's explicit, user-triggered serve operation.
use std::{collections::BTreeMap,fs::{self,OpenOptions},path::{Path,PathBuf},process::{Child,Command,Stdio},sync::{Mutex,atomic::{AtomicBool,Ordering}},time::{Duration,Instant}};
pub const DEFAULT_BASE_URL:&str="http://127.0.0.1:8000/v1";
pub const DEFAULT_MODEL:&str="ciocan/gemma-4-E4B-it-W4A16";
#[derive(Clone)]
pub struct Config {pub root:PathBuf,pub base_url:String,pub model:String,pub autostart:bool,pub startup_timeout:Duration,pub environment:BTreeMap<String,String>}
fn enabled(s:&str)->bool{!matches!(s.trim().to_lowercase().as_str(),"0"|"false"|"no"|"nee")}
impl Config {
    pub fn from_env(root:PathBuf)->Self{Self::from_environment(root,std::env::vars().collect())}
    pub fn from_environment(root:PathBuf,environment:BTreeMap<String,String>)->Self{
        // Python's os.environ normalizes Windows keys to uppercase. Preserve
        // that contract for PATH/Path so system and CUDA paths cannot shadow
        // one another when Command builds its case-insensitive environment.
        #[cfg(windows)]
        let environment=environment.into_iter().map(|(key,value)|(key.to_ascii_uppercase(),value)).collect::<BTreeMap<_,_>>();
        let get=|k:&str,d:&str|environment.get(k).filter(|s|!s.is_empty()).map_or_else(||d.into(),Clone::clone);
        Self{root,base_url:get("SLEUFBASE_VLLM_BASE_URL",DEFAULT_BASE_URL).trim_end_matches('/').into(),model:get("SLEUFBASE_VLLM_MODEL",DEFAULT_MODEL).trim().into(),autostart:enabled(&get("SLEUFBASE_VLLM_AUTOSTART","1")),startup_timeout:Duration::from_secs(420),environment}
    }
}
pub fn is_local_url(url:&str)->bool{reqwest::Url::parse(url).ok().and_then(|u|u.host_str().map(str::to_owned)).is_some_and(|h|matches!(h.as_str(),"127.0.0.1"|"localhost"|"[::1]"|"::1"))}
pub fn quantization(model:&str,override_value:&str)->Option<String>{let explicit=override_value.trim();if !explicit.is_empty(){return Some(explicit.into());}let name=model.to_lowercase();if name.contains("w4a16")||name.contains("gptq"){Some("gptq".into())}else if name.contains("awq"){Some("awq".into())}else{None}}
pub struct Launch {pub executable:PathBuf,pub args:Vec<String>,pub environment:BTreeMap<String,String>,pub log:PathBuf}
pub fn launch_plan(config:&Config)->Result<Launch,String>{
    if !is_local_url(&config.base_url){return Err("vLLM automatisch starten kan alleen lokaal".into());}
    let url=reqwest::Url::parse(&config.base_url).map_err(|_|"Ongeldig vLLM endpoint")?;
    if !matches!(url.scheme(),"http"|"https")||!url.username().is_empty()||url.password().is_some(){return Err("Ongeldig vLLM endpoint".into());}
    if config.model.is_empty(){return Err("Geen vLLM-model ingesteld".into());}
    let tools=config.root.join(".tools").join("vllm");
    let executable=tools.join("Scripts").join("vllm.exe");
    if !executable.is_file(){return Err(format!("vLLM is niet geinstalleerd: {}",executable.display()));}
    let candidate=tools.join("models").join(config.model.replace(['/', '\\'],"-"));
    let model=if Path::new(&config.model).is_dir(){PathBuf::from(&config.model)}else if candidate.is_dir(){candidate}else{PathBuf::from(&config.model)};
    let get=|k:&str,d:&str|config.environment.get(k).map_or_else(||d.into(),Clone::clone);
    let mut args=vec!["serve".into(),model.to_string_lossy().into_owned(),"--host".into(),"127.0.0.1".into(),"--port".into(),url.port().unwrap_or(8000).to_string(),"--served-model-name".into(),config.model.clone()];
    for (arg,key,default) in [("--max-model-len","MAX_MODEL_LEN","4096"),("--dtype","DTYPE","half"),("--gpu-memory-utilization","GPU_MEMORY_UTILIZATION","0.78"),("--cpu-offload-gb","CPU_OFFLOAD_GB","4"),("--max-num-seqs","MAX_NUM_SEQS","1"),("--safetensors-load-strategy","SAFETENSORS_LOAD_STRATEGY","lazy")]{args.extend([arg.into(),get(&format!("SLEUFBASE_VLLM_{key}"),default)]);}
    args.push("--trust-remote-code".into());
    if let Some(q)=quantization(&config.model,&get("SLEUFBASE_VLLM_QUANTIZATION","")){args.extend(["--quantization".into(),q]);}
    if enabled(&get("SLEUFBASE_VLLM_ENFORCE_EAGER","1")){args.push("--enforce-eager".into());}
    args.extend(get("SLEUFBASE_VLLM_EXTRA_ARGS","").split_whitespace().map(str::to_owned));
    let mut environment=config.environment.clone();let cache=tools.join("hf_home");
    for (key,value) in [("HF_HOME",cache.clone()),("HUGGINGFACE_HUB_CACHE",cache.join("hub")),("TRANSFORMERS_CACHE",cache.join("transformers"))]{environment.entry(key.into()).or_insert_with(||value.to_string_lossy().into_owned());}
    for (key,value) in [("VLLM_WORKER_MULTIPROC_METHOD","spawn"),("PYTHONUTF8","1"),("PYTHONIOENCODING","utf-8"),("PYTORCH_CUDA_ALLOC_CONF","expandable_segments:True")]{environment.entry(key.into()).or_insert_with(||value.into());}
    let packages=tools.join("Lib").join("site-packages");let cuda=packages.join("nvidia").join("cu13");
    if cuda.is_dir(){environment.entry("CUDA_LIB_PATH".into()).or_insert_with(||cuda.to_string_lossy().into_owned());}
    let mut paths=vec![executable.parent().unwrap().to_path_buf(),cuda.join("bin"),cuda.join("bin").join("x86_64"),packages.join("torch").join("lib")];
    if let Some(path)=environment.get("PATH"){paths.extend(std::env::split_paths(path));}
    environment.insert("PATH".into(),std::env::join_paths(paths).map_err(|e|e.to_string())?.to_string_lossy().into_owned());
    Ok(Launch{executable,args,environment,log:tools.join("logs").join("vllm-server.log")})
}
fn prepare_files(root:&Path,plan:&Launch)->Result<(),String>{
    let tools=root.join(".tools").join("vllm");
    fs::create_dir_all(plan.log.parent().unwrap()).map_err(|e|e.to_string())?;
    fs::create_dir_all(tools.join("hf_home")).map_err(|e|e.to_string())?;
    let packages=tools.join("Lib").join("site-packages");let target=packages.join("bin").join("cudart64_13.dll");
    if !target.exists(){for source in [packages.join("nvidia").join("cu13").join("bin").join("x86_64").join("cudart64_13.dll"),packages.join("torch").join("lib").join("cudart64_13.dll")]{if source.is_file(){fs::create_dir_all(target.parent().unwrap()).map_err(|e|e.to_string())?;fs::copy(source,&target).map_err(|e|e.to_string())?;break;}}}
    Ok(())
}
pub fn check(cancel:&AtomicBool)->Result<(),String>{if cancel.load(Ordering::Relaxed){Err("vLLM-aanvraag geannuleerd".into())}else{Ok(())}}
// Ownership and cancellation cover this direct executable. A Windows job
// object for independently spawned worker descendants is not yet implemented.
struct OwnedChild(Child);
impl Drop for OwnedChild{fn drop(&mut self){let _=self.0.kill();let _=self.0.wait();}}
#[derive(Default)]
pub struct Runtime {child:Mutex<Option<OwnedChild>>}
impl Runtime {
    pub fn ensure(&self,config:&Config,cancel:&AtomicBool)->Result<String,String>{
        let client=reqwest::blocking::Client::builder().timeout(Duration::from_secs(2)).redirect(reqwest::redirect::Policy::none()).build().map_err(|e|e.to_string())?;
        self.ensure_with(config,cancel,||client.get(format!("{}/models",config.base_url.trim_end_matches('/'))).send().is_ok_and(|r|r.status().is_success()),||{
            let plan=launch_plan(config)?;prepare_files(&config.root,&plan)?;
            let log=OpenOptions::new().create(true).append(true).open(&plan.log).map_err(|e|e.to_string())?;
            let mut command=Command::new(&plan.executable);command.args(&plan.args).current_dir(&config.root).envs(&plan.environment).stdin(Stdio::null()).stderr(log.try_clone().map_err(|e|e.to_string())?).stdout(log);
            #[cfg(windows)] {use std::os::windows::process::CommandExt;command.creation_flags(0x08000000);}
            Ok(OwnedChild(command.spawn().map_err(|e|format!("vLLM kon niet starten: {e}"))?))
        },Duration::from_secs(2))
    }
    fn ensure_with(&self,config:&Config,cancel:&AtomicBool,mut ready:impl FnMut()->bool,spawn:impl FnOnce()->Result<OwnedChild,String>,poll_delay:Duration)->Result<String,String>{
        check(cancel)?;
        if ready(){check(cancel)?;return Ok(config.base_url.clone());}
        if !config.autostart{return Err(format!("vLLM draait niet op {}",config.base_url));}
        let mut owned=self.child.lock().map_err(|_|"vLLM runtime is niet beschikbaar")?;
        check(cancel)?;
        if ready(){check(cancel)?;return Ok(config.base_url.clone());}
        // A temporary /models timeout does not mean the owned GPU process has
        // exited. Reuse it before even considering a second launch.
        if let Some(child)=owned.as_mut(){
            if child.0.try_wait().map_err(|e|e.to_string())?.is_none(){
                self.wait_ready(&mut child.0,config,cancel,&mut ready,poll_delay)?;
                return Ok(config.base_url.clone());
            }
            owned.take();
        }
        let mut child=spawn()?;
        self.wait_ready(&mut child.0,config,cancel,&mut ready,poll_delay)?;
        *owned=Some(child);Ok(config.base_url.clone())
    }
    fn wait_ready(&self,child:&mut Child,config:&Config,cancel:&AtomicBool,ready:&mut impl FnMut()->bool,poll_delay:Duration)->Result<(),String>{
        let log=config.root.join(".tools/vllm/logs/vllm-server.log");
        let deadline=Instant::now()+config.startup_timeout;
        loop {
            check(cancel)?;
            if let Some(status)=child.try_wait().map_err(|e|e.to_string())?{return Err(format!("vLLM is gestopt ({status}). Log: {}",log.display()));}
            if ready(){check(cancel)?;return Ok(());}
            if Instant::now()>=deadline{return Err(format!("vLLM werd niet bereikbaar. Log: {}",log.display()));}
            let wake=Instant::now()+poll_delay;
            while Instant::now()<wake{check(cancel)?;std::thread::sleep(wake.saturating_duration_since(Instant::now()).min(Duration::from_millis(100)));}
        }
    }
}
#[cfg(test)] mod tests{
    use super::*;
    #[test]fn original_quantization_localhost_and_launch_contract(){
        for host in ["127.0.0.1","localhost","[::1]"]{assert!(is_local_url(&format!("http://{host}:8000/v1")));}
        assert!(!is_local_url("http://localhost.example/v1"));
        assert_eq!(quantization(DEFAULT_MODEL,""),Some("gptq".into()));assert_eq!(quantization("awq","override"),Some("override".into()));
        let temp=tempfile::tempdir().unwrap();let tools=temp.path().join(".tools").join("vllm");fs::create_dir_all(tools.join("Scripts")).unwrap();fs::write(tools.join("Scripts").join("vllm.exe"),b"fake").unwrap();
        let config=Config::from_environment(temp.path().into(),BTreeMap::new());assert!(launch_plan(&config).unwrap().args.contains(&DEFAULT_MODEL.to_string()));fs::create_dir_all(tools.join("models").join(DEFAULT_MODEL.replace('/',"-"))).unwrap();
        let plan=launch_plan(&config).unwrap();assert!(plan.args.windows(2).any(|a|a==["--quantization","gptq"]));assert!(Path::new(&plan.args[1]).is_dir());
        let source=tools.join("Lib/site-packages/torch/lib");fs::create_dir_all(&source).unwrap();fs::write(source.join("cudart64_13.dll"),b"fake local CUDA runtime").unwrap();prepare_files(temp.path(),&plan).unwrap();assert_eq!(fs::read(tools.join("Lib/site-packages/bin/cudart64_13.dll")).unwrap(),b"fake local CUDA runtime");
        assert!(Runtime::default().ensure(&config,&AtomicBool::new(true)).is_err());
    }
    #[test]fn fake_child_is_reaped_by_managed_guard(){
        #[cfg(windows)] {let mut command=Command::new("cmd.exe");command.args(["/c","exit","7"]);use std::os::windows::process::CommandExt;command.creation_flags(0x08000000);let mut child=OwnedChild(command.spawn().unwrap());assert_eq!(child.0.wait().unwrap().code(),Some(7));}
    }
    #[cfg(windows)]
    #[test]fn windows_mixed_case_environment_preserves_system_path_and_overrides(){
        let temp=tempfile::tempdir().unwrap();let scripts=temp.path().join(".tools/vllm/Scripts");fs::create_dir_all(&scripts).unwrap();fs::write(scripts.join("vllm.exe"),b"never executed").unwrap();
        let config=Config::from_environment(temp.path().into(),BTreeMap::from([("Path".into(),"C:\\Windows\\System32;C:\\fixture-bin".into()),("hf_home".into(),"custom-cache".into()),("sleufbase_vllm_model".into(),"fixture/AWQ".into())]));
        let plan=launch_plan(&config).unwrap();assert_eq!(config.model,"fixture/AWQ");assert_eq!(plan.environment["HF_HOME"],"custom-cache");assert!(plan.environment["PATH"].ends_with("C:\\Windows\\System32;C:\\fixture-bin"));assert!(!plan.environment.contains_key("Path"));
    }
    #[test]fn existing_live_runtime_is_reused_after_models_timeouts_and_exited_runtime_can_restart(){
        fn fake()->OwnedChild{
            #[cfg(windows)] {let mut c=Command::new("ping.exe");c.args(["-n","30","127.0.0.1"]);use std::os::windows::process::CommandExt;c.creation_flags(0x08000000);c.stdout(Stdio::null()).stderr(Stdio::null());OwnedChild(c.spawn().unwrap())}
            #[cfg(not(windows))] {OwnedChild(Command::new("sleep").arg("30").spawn().unwrap())}
        }
        let runtime=Runtime::default();let mut config=Config::from_environment(PathBuf::new(),BTreeMap::new());config.startup_timeout=Duration::from_millis(50);
        let child=fake();let id=child.0.id();*runtime.child.lock().unwrap()=Some(child);
        let mut calls=0;runtime.ensure_with(&config,&AtomicBool::new(false),||{calls+=1;calls>=3},||panic!("a living GPU process must never be launched twice"),Duration::ZERO).unwrap();assert_eq!(runtime.child.lock().unwrap().as_ref().unwrap().0.id(),id);
        assert!(runtime.ensure_with(&config,&AtomicBool::new(false),||false,||panic!("timeout must retain the existing child"),Duration::ZERO).is_err());assert_eq!(runtime.child.lock().unwrap().as_ref().unwrap().0.id(),id);
        let cancel=AtomicBool::new(false);let mut checks=0;
        assert!(runtime.ensure_with(&config,&cancel,||{checks+=1;if checks==3{cancel.store(true,Ordering::Release);}false},||panic!("cancel must not relaunch a living child"),Duration::ZERO).is_err());assert_eq!(runtime.child.lock().unwrap().as_ref().unwrap().0.id(),id);
        {let mut owned=runtime.child.lock().unwrap();let child=&mut owned.as_mut().unwrap().0;child.kill().unwrap();child.wait().unwrap();}
        let mut checks=0;let mut spawns=0;runtime.ensure_with(&config,&AtomicBool::new(false),||{checks+=1;checks>=3},||{spawns+=1;Ok(fake())},Duration::ZERO).unwrap();assert_eq!(spawns,1);assert!(runtime.child.lock().unwrap().as_mut().unwrap().0.try_wait().unwrap().is_none());
        let fresh=Runtime::default();let cancel=AtomicBool::new(false);let mut checks=0;
        assert!(fresh.ensure_with(&config,&cancel,||{checks+=1;if checks==3{cancel.store(true,Ordering::Release);}false},||Ok(fake()),Duration::ZERO).is_err());assert!(fresh.child.lock().unwrap().is_none(),"a cancelled new launch must not become the owned ready server");
    }
}
