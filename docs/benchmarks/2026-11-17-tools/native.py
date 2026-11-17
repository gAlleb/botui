import importlib.util, json, os, time, re, hashlib, signal, subprocess, shlex, sys
from pathlib import Path
from PIL import Image
BASE=Path(os.environ['BOTUI_BENCH_DIR'])
s=importlib.util.spec_from_file_location('live',Path(__file__).with_name('live.py'));live=importlib.util.module_from_spec(s);s.loader.exec_module(live)
books={key:Path(value) for key,value in json.loads(Path(os.environ['BOTUI_BENCH_CASES']).read_text()).items()}
def procs(root):
 result=[];todo=[root];seen=set()
 while todo:
  pid=todo.pop()
  if pid in seen:continue
  seen.add(pid);p=Path('/proc')/str(pid)
  try:
   status=(p/'status').read_text();rss=re.search(r'^VmRSS:\s+(\d+)',status,re.M)
   pss=re.search(r'^Pss:\s+(\d+)',(p/'smaps_rollup').read_text(),re.M)
   result.append({'pid':pid,'comm':(p/'comm').read_text().strip(),'rss_kib':int(rss[1]) if rss else 0,'pss_kib':int(pss[1]) if pss else 0})
   for childfile in (p/'task').glob('*/children'):
    try:todo.extend(map(int,childfile.read_text().split()))
    except OSError:pass
  except (OSError,ValueError):pass
 return result

def storage(profile):
 result={}
 for base in ['cache/botui','cache/bookokrat','state/botui','state/bookokrat','data/botui','data/bookokrat','home']:
  files=[p for p in (live.ROOT/profile/base).rglob('*') if p.is_file()]
  result[base]={'files':len(files),'bytes':sum(p.stat().st_size for p in files)}
 return result

def sample(profile):
 pid=int((live.ROOT/(profile+'.pid')).read_text());ps=procs(pid)
 return {'processes':ps,'total_rss_kib':sum(p['rss_kib'] for p in ps),'storage':storage(profile)}

def capture(profile,name):
 file=BASE/(name+'.png');live.run(['grim',str(file)],env=live.environment(profile))
 (BASE/(name+'.txt')).write_text(live.text(profile))
 return file

def setup(profile,app,book):
 env=live.environment(profile);root=live.ROOT/profile
 if app!='bookokrat':
  cfg=Path(env['XDG_CONFIG_HOME'])/'botui/config.toml';cfg.parent.mkdir(exist_ok=True)
  cfg.write_text('theme="gruvbox-dark"\ninterface_theme="gruvbox-dark"\nzen=true\nzen_progress=true\nreading_layout="single"\npage_width=110\n')
 else:
  cfg=Path(env['XDG_CONFIG_HOME'])/'bookokrat/config.yaml';cfg.parent.mkdir(exist_ok=True)
  cfg.write_text('version: 3\ntheme: Oceanic Next\ntransparent_background: false\nepub_column_mode: single\npdf_settings_configured: true\npdf_scale: 0.46\n')
 gate=root/'start';gate.unlink(missing_ok=True)
 command=[live.BINS[app],str(book)]+(['--zen-mode'] if app=='bookokrat' else [])
 wrapper=root/'start-reader.sh'
 wrapper.write_text('#!/bin/sh\nprintf "AUDIT_READY\\n"\nwhile [ ! -f '+shlex.quote(str(gate))+' ]; do sleep 0.02; done\nexec '+shlex.join(command)+'\n');wrapper.chmod(0o755)
 live.open_app(profile,str(wrapper),'')
 live.wait(profile,'AUDIT_READY')
 return gate

def ready(profile,pdf=False,timeout=45):
 end=time.monotonic()+timeout
 while time.monotonic()<end:
  frame=live.text(profile)
  if '%' in frame and 'AUDIT_READY' not in frame and 'Rendering page' not in frame:
   if not pdf:return frame
   # A ready PDF must contain actual light paper pixels, not just a footer.
   file=BASE/'probe.pdf.png';live.run(['grim',str(file)],env=live.environment(profile))
   im=Image.open(file).convert('RGB');crop=im.crop((150,60,1450,900));a=list(crop.resize((130,84)).get_flattened_data())
   if sum(max(c)>90 for c in a)>len(a)*.008:return frame
  time.sleep(.05)
 raise AssertionError((profile,'not ready',frame))

def run_case(app,kind,n):
 profile=f'bench-{app}-{kind}-{n}';gate=setup(profile,app,books[kind])
 try:
  start=time.monotonic();gate.touch();frame=ready(profile,kind=='lessons');elapsed=time.monotonic()-start
  time.sleep(.7);result={'app':app,'kind':kind,'repeat':n,'first_ready_ms':round(elapsed*1000,1),'rows':len(frame.splitlines()),'columns':max(map(len,frame.splitlines())),'sample':sample(profile)}
  if n==0:capture(profile,f'native-{app}-{kind}')
  print(json.dumps(result,ensure_ascii=False),flush=True)
  with (BASE/'native-metrics.jsonl').open('a') as out:out.write(json.dumps(result,ensure_ascii=False)+'\n')
 finally:live.stop_pid(live.ROOT/(profile+'.pid'))

if __name__=='__main__':
 if sys.argv[1]=='probe':run_case(sys.argv[2],sys.argv[3],int(sys.argv[4]))
 elif sys.argv[1]=='all':
  for n in range(3):
   for kind in books:
    for app in (['baseline','bookokrat'] if n%2==0 else ['bookokrat','baseline']):run_case(app,kind,n)
