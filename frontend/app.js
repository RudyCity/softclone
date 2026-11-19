"use strict";
const invoke=window.__TAURI__?.core?.invoke;
const $=(q)=>document.querySelector(q);
const $$=(q)=>[...document.querySelectorAll(q)];
const e=(v)=>String(v??"").replace(/[&<>"']/g,c=>({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c]));
const bytes=(v)=>{let n=Number(v)||0,i=0;const u=["B","KB","MB","GB","TB"];while(n>=1024&&i<4){n/=1024;i++}return (i?n.toFixed(1):n.toFixed(0))+" "+u[i]};
const date=(v)=>new Date(Number(v)*1000).toLocaleString();
const S={page:"library",clones:[],apps:[],logs:[],logErr:null,query:"",filter:"all",busy:false,creating:false,progress:null,scan:false,scanned:false,modal:null,preflight:null,info:{version:"0.6.0",os:"desktop",license:"Apache-2.0",runtime:"Tauri 2"},form:{source:"",name:"",entrypoint:"",isolatedProfile:true,profileOnly:false}};
const btn=(label,action,klass="secondary",attrs="")=>'<button class="btn btn-'+klass+'" type="button" data-action="'+action+'" '+attrs+' '+(S.busy?"disabled":"")+'>'+label+'</button>';
const header=(tag,title,desc,actions="")=>'<div class="page-heading"><div><p class="eyebrow">'+e(tag)+'</p><h1>'+e(title)+'</h1><p class="page-lead">'+e(desc)+'</p></div>'+actions+'</div>';
const empty=(name,desc,action="")=>'<div class="empty"><div class="empty-emblem">□</div><h3>'+e(name)+'</h3><p>'+e(desc)+'</p>'+action+'</div>';
const pill=(value,kind="")=>'<span class="pill '+kind+'">'+e(value)+'</span>';
const alert=(msg,bad=false)=>{const node=document.createElement("div");node.className="toast"+(bad?" error":"");node.textContent=String(msg);$("#toast-root").append(node);setTimeout(()=>node.remove(),6000)};
const api=async(name,args={})=>{if(!invoke)throw Error("Native Tauri bridge is unavailable");return invoke(name,args)};
const item=(id)=>S.clones.find(c=>c.id===id);
async function load(){S.clones=await api("list_instances");$("#sidebar-count").textContent=S.clones.length;if(S.page==="library")render()}
async function logs(){const result=await api("get_logs");S.logs=result.entries||[];S.logErr=result.storageError;if(S.page==="logs")render()}
async function discovery(){S.scan=true;render();try{S.apps=await api("discover_apps");S.scanned=true;alert("Application inventory refreshed")}catch(err){alert(err,true)}finally{S.scan=false;render()}}
const metric=(name,value,detail)=>'<div class="metric"><div class="metric-label">'+name+'</div><div class="metric-value">'+e(value)+'</div><div class="metric-hint">'+detail+'</div></div>';
const actionButton=(label,name,id,kind="secondary")=>btn(label,name,kind,'data-id="'+e(id)+'"');
function library(){
 const found=S.clones.filter(c=>(!S.query||[c.name,c.source,c.entrypoint].join(" ").toLowerCase().includes(S.query.toLowerCase()))&&(S.filter==="all"||(S.filter==="running"?c.running:!c.running)));
 const running=S.clones.filter(c=>c.running).length;
 return header("Workspace overview","My clones","Manage local application copies, dedicated profiles and backups from one place.",btn("＋ Create clone","go-create","primary"))+
 '<div class="metrics">'+metric("Managed clones",S.clones.length,"Stored locally")+metric("Running now",running,"Tracked processes only")+metric("Separate profiles",S.clones.filter(c=>c.run_mode==="profile_only").length,"Best-effort isolation")+'</div>'+
 (S.clones.length?"":'<section class="hero"><div><h2>Start with a new clone</h2><p>Discover locally installed apps or choose your own portable executable.</p></div>'+btn("Discover apps","go-discover","primary")+'</section>')+
 '<div class="toolbar"><input aria-label="Search clones" id="library-search" class="input search" value="'+e(S.query)+'" placeholder="Search clones and source paths…"><select class="select" id="library-filter"><option value="all">All clones</option><option value="running" '+(S.filter==="running"?"selected":"")+'>Running</option><option value="stopped" '+(S.filter==="stopped"?"selected":"")+'>Not running</option></select>'+btn("Refresh","refresh","quiet")+'</div>'+
 (found.length?'<div class="cards-grid">'+found.map(c=>'<article class="clone-card"><div class="clone-top"><div class="app-logo">'+e(c.name.slice(0,1).toUpperCase())+'</div><div style="min-width:0"><h3 class="clone-title">'+e(c.name)+'</h3><div class="clone-subtitle">'+e(c.source)+'</div></div></div><div class="clone-status">'+pill(c.running?"● Running · PID "+c.pid:"○ Not running",c.running?"success":"")+' '+pill(c.run_mode==="profile_only"?"Profile-only":"Portable copy")+'</div><div class="clone-actions">'+(c.running?actionButton("Stop","stop",c.id)+actionButton("Restart","restart",c.id):actionButton("Launch","launch",c.id,"primary"))+actionButton("Open folder","folder",c.id)+actionButton("Rename","rename",c.id)+actionButton("Duplicate","duplicate",c.id)+actionButton("Backup","backup",c.id)+actionButton("Restore","restore",c.id)+actionButton("Delete","delete",c.id,"danger")+'</div></article>').join("")+'</div>':empty("No matching clones","Create a new clone or adjust your search.",btn("Create clone","go-create","primary")));
}
function create(){
 const f=S.form;
 const field=(label,id,placeholder,extra="")=>'<div><label class="field-label" for="'+id+'">'+label+'</label><div class="input-row"><input class="input" id="'+id+'" value="'+e(f[id])+'" placeholder="'+e(placeholder)+'">'+extra+'</div></div>';
 const choice=(value,title,desc)=>'<label class="choice '+((f.profileOnly?"profile":"copy")===value?"selected":"")+'"><input type="radio" name="strategy" value="'+value+'" '+((f.profileOnly?"profile":"copy")===value?"checked":"")+'><strong>'+title+'</strong><small>'+desc+'</small></label>';
 const progress=S.creating?'<div class="progress-shell"><strong>Creating instance…</strong><div class="progress-track"><div class="progress-fill" id="progress-fill"></div></div><div class="progress-meta"><span id="progress-file">Preparing source…</span><span id="progress-value">Working…</span></div><div style="margin-top:14px"><button class="btn btn-danger" type="button" data-action="cancel-clone">Cancel clone</button></div></div>':"";
 return header("New instance","Create clone","Select a local executable or folder and choose an instance strategy.")+
 '<div class="form-grid"><section class="form-card"><h2>1. Instance strategy</h2><div class="choice-grid">'+choice("copy","Portable copy","Copy files to a separate managed directory.")+choice("profile","Profile-only","Use installed binaries with a dedicated profile where supported.")+'</div><div style="margin-top:22px"><label class="checkbox-row"><input type="checkbox" id="isolated" '+(f.isolatedProfile||f.profileOnly?"checked":"")+(f.profileOnly?" disabled":"")+'>Prepare separate profile data folders</label><p class="hint">Environment-level isolation is not an OS security sandbox.</p></div></section>'+
 '<section class="form-card"><h2>2. Application details</h2><div class="fields">'+
 field("Source executable or folder","source","C:\\Program Files\\App\\App.exe",btn("File…","pick-file","quiet")+btn("Folder…","pick-folder","quiet"))+
 field("Display name","name","My app — work profile",btn("Suggest","suggest","quiet"))+
 field("Relative entrypoint (optional)","entrypoint","Launcher relative to source folder")+'</div></section></div>'+
 '<section class="panel" style="margin-top:20px"><div class="panel-body"><div class="toolbar" style="margin:0">'+btn("Preflight check","preflight")+btn(S.creating?"Creating…":"Create clone","create","primary",S.creating?"disabled":"")+'</div>'+
 (S.preflight?'<div class="notice" style="margin-top:16px">Preflight passed — '+bytes(S.preflight.totalBytes)+', '+S.preflight.totalFiles+' file(s).</div>':"")+progress+'</div></section>'+
 '<div class="notice warning">Cloning does not bypass licensing, activation, DRM, Store sandboxes, privileged services or vendor restrictions. Profile-only mode works best with documented application profile adapters.</div>';
}
function discover(){
 const match=S.apps.filter(a=>(!S.query||[a.name,a.publisher,a.version,a.path,a.storeId].join(" ").toLowerCase().includes(S.query.toLowerCase()))&&(S.filter==="all"||(S.filter==="profile"?a.support==="Independent profile":S.filter==="review"?a.support==="Requires review":!a.launchable&&!a.storeId)));
 const options=[["all","All apps"],["profile","Profile ready"],["review","Needs review"],["missing","Missing executable"]].map(([v,l])=>'<option value="'+v+'" '+(S.filter===v?"selected":"")+'>'+l+'</option>').join("");
 return header("Application inventory","Discover apps","Read-only discovery of installed applications. Supported profile adapters are identified separately from manual-review cases.",btn(S.scan?"Scanning…":"Rescan apps","rescan","primary",S.scan?"disabled":""))+
 '<div class="toolbar"><input class="input search" id="discover-search" placeholder="Search by name, publisher or executable…" value="'+e(S.query)+'"><select class="select" id="discover-filter">'+options+'</select></div>'+
 '<section class="panel"><div class="panel-header"><h2>Available applications</h2><span class="muted">'+(S.scan?"Scanning installed applications…":match.length+" matching of "+S.apps.length)+'</span></div>'+
 (match.length?'<div style="overflow-x:auto"><table class="app-table"><thead><tr><th>Application</th><th>Source & notes</th><th>Compatibility</th><th>Action</th></tr></thead><tbody>'+
 match.slice(0,150).map(a=>{const index=S.apps.indexOf(a);return '<tr><td><div class="app-main">'+e(a.name)+'</div><div class="app-meta">'+e(a.publisher||a.origin)+(a.version?" · "+e(a.version):"")+'</div></td><td><div class="app-meta">'+e(a.path||a.storeId||"No executable")+'</div><div class="app-note">'+e(a.note)+'</div></td><td>'+pill(a.support,a.support==="Independent profile"?"success":a.support==="Requires review"?"warn":"")+'</td><td>'+btn(a.storeId?"Open app":a.launchable?"Use app":"Locate…",a.storeId?"store":a.launchable?"use":"locate","secondary",'data-index="'+index+'"')+'</td></tr>'}).join("")+'</tbody></table></div>':empty(S.scan?"Scanning applications":S.scanned?"No matching apps":"Scan installed apps",S.scanned?"Try another filter.":"Discover applications without executing them.",btn("Scan now","rescan","primary")))+'</section><div class="notice warning">Windows MSIX/Store packages are activated normally; they cannot be cloned through binary copying.</div>';
}
const ERR=["RegistryLoadFailed","DiscoveryFailed","PreflightFailed","CloneFailed","LaunchFailed","ProcessStopFailed","ManagerFailed"];
function logPage(){
 const items=[...S.logs].reverse().filter(x=>(S.logFilter==="all"||(ERR.includes(x.event)?"error":x.event==="CloneCancelled"?"warning":"info")===S.logFilter)&&String(x.event).toLowerCase().includes(S.logQuery.toLowerCase()));
 return header("Diagnostic history","Debug logs","Privacy-safe local journal. User input, app paths and credentials are excluded.",btn("Export report","export","secondary"))+
 '<div class="toolbar"><input class="input search" id="logs-search" value="'+e(S.logQuery)+'" placeholder="Search events…"><select class="select" id="logs-filter">'+["all","info","warning","error"].map(v=>'<option value="'+v+'" '+(S.logFilter===v?"selected":"")+'>'+v.toUpperCase()+'</option>').join("")+'</select>'+btn("Refresh","refresh-logs","quiet")+btn("Clear logs","clear-logs","danger")+'</div>'+
 (S.logErr?'<div class="notice warning">'+e(S.logErr)+'</div>':"")+
 '<section class="panel"><div class="panel-header"><h2>Local events</h2><span class="muted">'+items.length+' records</span></div>'+
 (items.length?items.slice(0,300).map(x=>'<div class="log-entry"><span class="log-time">'+e(date(x.timestamp_secs))+'</span>'+pill(ERR.includes(x.event)?"ERROR":x.event==="CloneCancelled"?"WARNING":"INFO",ERR.includes(x.event)?"error":x.event==="CloneCancelled"?"warn":"success")+'<div class="log-message">'+e(String(x.event).replace(/([a-z])([A-Z])/g,"$1 $2"))+'</div></div>').join(""):empty("No events to show","Operations will appear here as you use SoftClone."))+'</section>';
}
function guide(){
 const cards=[["01","Discover","Scan installed apps or manually choose a local executable, directory, AppImage or .app bundle."],["02","Choose strategy","Portable copy duplicates source files; profile-only retains installed binaries and prepares dedicated data folders."],["03","Preflight","Validate executable, source links, file types and estimated bytes without modifying the source."],["04","Create and run","Create a managed instance using the existing Rust clone engine, with copy progress and cancellation."],["05","Maintain","Launch, stop tracked children, rename, duplicate, back up, restore and delete managed clones."],["06","Respect boundaries","Vendor restrictions, account licenses, signatures, services and operating-system sandboxes continue to apply."]];
 return header("Getting started","How it works","SoftClone is not a virtual machine or a licensing bypass.")+'<div class="guide-grid">'+cards.map(x=>'<article class="guide-card"><div class="guide-number">'+x[0]+'</div><h2>'+x[1]+'</h2><p>'+x[2]+'</p></article>').join("")+'</div>';
}
function about(){
 const line=(k,v)=>'<div>'+e(k)+'</div><div>'+e(v)+'</div>';
 return header("Application information","About SoftClone","An open-source local application cloning workspace.")+
 '<section class="panel"><div class="panel-body"><div class="fact-table">'+line("Application","SoftClone")+line("Version",S.info.version)+line("Desktop runtime",S.info.runtime)+line("Platform",S.info.os)+line("License",S.info.license)+line("Cloning engine","Rust + Rune")+line("Data","Local only")+'</div><div class="toolbar" style="margin-top:22px;margin-bottom:0">'+btn("GitHub source","source-link","primary")+btn("Support development","support-link")+btn("Open local data","data-folder","quiet")+'</div></div></section><div class="notice">SoftClone is licensed under Apache-2.0. Third-party software and dependencies keep their own license terms. Donations are optional.</div>';
}
function render(){
 $$(".nav-item").forEach(n=>n.classList.toggle("active",n.dataset.page===S.page));
 $("#breadcrumb").textContent=({library:"MY CLONES",create:"CREATE CLONE",discover:"DISCOVER APPS",logs:"DEBUG LOGS",guide:"HOW IT WORKS",about:"ABOUT"})[S.page];
 $("#view").innerHTML=({library,create,discover,logs:logPage,guide,about})[S.page]();
 modalRender();
}
function modalRender(){
 const root=$("#modal-root"),m=S.modal;
 if(!m){root.innerHTML="";return}
 const name=item(m.id)?.name||"";
 let body="",title="";
 if(m.type==="rename"||m.type==="duplicate"){
  title=m.type==="rename"?"Rename clone":"Duplicate clone";
  body='<p>'+(m.type==="rename"?"Only the display label changes.":"A separate managed copy is created.")+'</p><label class="field-label" for="modal-name">New name</label><input class="input" id="modal-name" maxlength="64" value="'+e(m.name)+'">';
 } else if(m.type==="delete"){
  title="Delete managed clone";
  body='<p>This removes managed files for <strong>'+e(name)+'</strong>, not the original application. Existing backups remain. Type the exact name to confirm.</p><input class="input" id="modal-name" placeholder="'+e(name)+'" value="'+e(m.name)+'">';
 } else if(m.type==="restore"){
  title="Restore backup";
  body='<p>This will replace managed clone files with an older snapshot.</p>'+(m.loading?"Loading backups…":m.backups.length?'<select id="restore-select" class="select" style="width:100%">'+m.backups.map(b=>'<option value="'+e(b.id)+'" '+(m.backupId===b.id?"selected":"")+'>'+e(date(b.created_epoch_secs))+' · '+bytes(b.copied_bytes)+'</option>').join("")+'</select>':'<div class="notice warning">No valid backups exist.</div>');
 } else {title="Clear diagnostic history";body="<p>Delete both active and archived diagnostic journals? Your clones are unaffected.</p>"}
 root.innerHTML='<div class="modal-backdrop"><div class="modal-dialog" role="dialog" aria-modal="true" aria-labelledby="modal-title"><div class="modal-head"><h2 id="modal-title">'+title+'</h2></div><div class="modal-body">'+body+'</div><div class="modal-actions">'+btn("Cancel","close-modal")+btn(m.type==="delete"?"Delete permanently":m.type==="restore"?"Restore snapshot":m.type==="clear"?"Clear logs":"Confirm","confirm",m.type==="delete"||m.type==="clear"?"danger":"primary",(m.type==="restore"&&(m.loading||!m.backups.length))?"disabled":"")+'</div></div></div>';
}
function page(p){
 S.page=p;S.query="";S.filter="all";S.modal=null;
 if(p==="discover"&&!S.scanned&&!S.scan)discovery();
 else if(p==="logs")logs().catch(err=>alert(err,true));
 else render();
}
function fromApp(a,path){
 if(a.storeId)return alert("Store applications cannot be cloned directly",true);
 S.form={source:path||a.path,name:a.name+" copy",entrypoint:"",isolatedProfile:true,profileOnly:a.support==="Independent profile"||a.origin.includes("Installed")};
 S.preflight=null;page("create");
}
async function task(fn,message){
 if(S.busy)return;
 S.busy=true;render();
 try{await fn();if(message)alert(message);await load()}catch(err){alert(err,true)}finally{S.busy=false;render()}
}
function openModal(type,id){
 const current=item(id);
 S.modal={type,id,name:type==="rename"?current?.name||"":"",backups:[],loading:type==="restore"};
 modalRender();
 if(type==="restore")api("list_backups",{id}).then(backups=>{if(S.modal?.id!==id)return;S.modal.backups=backups;S.modal.backupId=backups[0]?.id||"";S.modal.loading=false;modalRender()}).catch(err=>{alert(err,true);S.modal=null;modalRender()});
}
async function confirm(){
 const m=S.modal;if(!m)return;
 if(m.type==="delete"&&m.name!==item(m.id)?.name)throw Error("Type the exact clone name");
 if(["rename","duplicate"].includes(m.type)&&!m.name.trim())throw Error("Name is required");
 if(m.type==="restore"&&!m.backups.length)throw Error("No backup selected");
 await task(async()=>{
  if(m.type==="rename")await api("rename_instance",{id:m.id,name:m.name});
  if(m.type==="duplicate")await api("duplicate_instance",{id:m.id,name:m.name});
  if(m.type==="delete")await api("delete_instance",{id:m.id});
  if(m.type==="restore")await api("restore_instance",{id:m.id,backupId:m.backupId});
  if(m.type==="clear"){await api("clear_logs");await logs()}
  S.modal=null;
 },"Operation completed");
}
async function clone(){
 if(S.creating)return;
 S.creating=true;S.busy=true;render();
 const t=setInterval(async()=>{try{
  const p=await api("clone_progress");if(!p)return;
  const b=$("#progress-fill"),f=$("#progress-file"),v=$("#progress-value");
  if(b)b.style.width=Math.min(100,Math.round(p.copiedBytes/Math.max(1,p.totalBytes)*100))+"%";
  if(f)f.textContent=p.currentFile||"Preparing";
  if(v)v.textContent=bytes(p.copiedBytes)+" / "+bytes(p.totalBytes);
 }catch{}},200);
 try{await api("create_instance",{input:{...S.form}});alert("Clone created");S.preflight=null;S.form={source:"",name:"",entrypoint:"",isolatedProfile:true,profileOnly:false};await load();page("library")}
 catch(err){alert(err,true)}
 finally{clearInterval(t);S.creating=false;S.busy=false;render()}
}
async function action(a,el){
 const id=el.dataset.id, c=item(id);
 switch(a){
  case "go-create":page("create");break;
  case "go-discover":page("discover");break;
  case "refresh":await load();break;
  case "refresh-logs":await logs();break;
  case "rescan":if(!S.scan)await discovery();break;
  case "pick-file":case "pick-folder":{const path=await api("pick_source",{kind:a==="pick-file"?"file":"folder"});if(path){S.form.source=path;if(!S.form.name)S.form.name=path.split(/[\\/]/).pop().replace(/\.(exe|app|AppImage)$/i,"")+" copy";S.preflight=null;render()}break}
  case "suggest":S.form.name=await api("suggest_label",{base:S.form.name||"Application",index:2});render();break;
  case "preflight":await task(async()=>{S.preflight=await api("preflight",{input:{...S.form}})},"Preflight passed");break;
  case "create":await clone();break;
  case "cancel-clone":await api("cancel_clone");alert("Cancellation requested");break;
  case "launch":await task(()=>api("launch_instance",{id}),"Launch requested");break;
  case "stop":await task(()=>api("stop_instance",{id}),"Process stopped");break;
  case "restart":await task(async()=>{await api("stop_instance",{id});await api("launch_instance",{id})},"Process restarted");break;
  case "folder":await api("open_instance_folder",{id});break;
  case "data-folder":await api("open_data_folder");break;
  case "rename":case "duplicate":case "delete":case "restore":if(c?.running&&a!=="rename")throw Error("Stop the managed process first");openModal(a,id);break;
  case "backup":if(c?.running)throw Error("Stop the managed process first");await task(()=>api("backup_instance",{id}),"Backup completed");break;
  case "use":fromApp(S.apps[Number(el.dataset.index)]);break;
  case "locate":{const path=await api("pick_source",{kind:"file"});if(path)fromApp(S.apps[Number(el.dataset.index)],path);break}
  case "store":await api("open_store_app",{aumid:S.apps[Number(el.dataset.index)].storeId});alert("App activation requested");break;
  case "source-link":await api("open_external",{target:"source"});break;
  case "support-link":await api("open_external",{target:"support"});break;
  case "export":{const p=await api("export_logs");if(p)alert("Report saved");break}
  case "clear-logs":openModal("clear","");break;
  case "confirm":await confirm();break;
  case "close-modal":S.modal=null;modalRender();break;
 }
}
document.addEventListener("click",async ev=>{
 const target=ev.target.closest("button[data-page],button[data-action]");if(!target||target.disabled)return;
 if(target.dataset.page)return page(target.dataset.page);
 try{await action(target.dataset.action,target)}catch(err){alert(err,true)}
});
document.addEventListener("input",ev=>{
 const id=ev.target.id,value=ev.target.value;
 if(["source","name","entrypoint"].includes(id)){S.form[id]=value;S.preflight=null}
 else if(id==="modal-name"&&S.modal)S.modal.name=value;
 else if(["library-search","discover-search","logs-search"].includes(id)){
  if(id==="logs-search")S.logQuery=value;else S.query=value;
  const pos=ev.target.selectionStart;
  render();const next=$("#"+id);if(next){next.focus({preventScroll:true});next.setSelectionRange(pos,pos)}
 }
});
document.addEventListener("change",ev=>{
 const id=ev.target.id;
 if(ev.target.name==="strategy"){S.form.profileOnly=ev.target.value==="profile";S.preflight=null;render()}
 else if(id==="isolated"){S.form.isolatedProfile=ev.target.checked;S.preflight=null}
 else if(["library-filter","discover-filter"].includes(id)){S.filter=ev.target.value;render()}
 else if(id==="logs-filter"){S.logFilter=ev.target.value;render()}
 else if(id==="restore-select"&&S.modal){S.modal.backupId=ev.target.value}
});
document.addEventListener("keydown",ev=>{if(ev.key==="Escape"&&S.modal&&!S.busy){S.modal=null;modalRender()}});
(async()=>{
 if(!invoke){$("#view").innerHTML=empty("Tauri desktop required","Start SoftClone through its native desktop application.");return}
 try{const [info,items]=await Promise.all([api("app_info"),api("list_instances")]);S.info=info;S.clones=items;$("#sidebar-count").textContent=items.length;$("#sidebar-version").textContent="VERSION "+info.version+" · TAURI 2";$("#platform-tag").textContent=info.os.toUpperCase();render()}
 catch(err){$("#view").innerHTML=empty("Workspace unavailable",String(err));alert(err,true)}
})();
