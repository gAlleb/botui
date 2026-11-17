import importlib.util,json,re,time,sys,os
from pathlib import Path
B=Path(os.environ['BOTUI_BENCH_DIR']);s=importlib.util.spec_from_file_location('bench',Path(__file__).with_name('native.py'));b=importlib.util.module_from_spec(s);s.loader.exec_module(b);l=b.live
app=sys.argv[1];kind=sys.argv[2];profile=f'growth-{app}-{kind}';gate=b.setup(profile,app,b.books[kind]);rows=[]
def snap(name):
 a=b.sample(profile);a['step']=name;rows.append(a);print(json.dumps(a),flush=True)
 if a['total_rss_kib']>1300000:raise RuntimeError('Owned test exceeds 1.3GiB RSS bound')
 (B/f'{profile}.json').write_text(json.dumps(rows,indent=2))
def wait_change(before):
 end=time.monotonic()+12
 while time.monotonic()<end:
  f=l.text(profile)
  if f!=before and ('%' in f):return f
  time.sleep(.08)
 raise AssertionError(('no turn',before))
try:
 gate.touch();b.ready(profile,kind=='lessons');time.sleep(1);snap('opened')
 if kind=='lessons':
  for cycle in range(5):
   for key in (['right']*8+['left']*8 if app=='bookokrat' else ['page_down']*8+['page_up']*8):
    before=l.text(profile);l.key(profile,key);wait_change(before);time.sleep(.2)
   time.sleep(1);snap('cycle_'+str(cycle+1))
  b.capture(profile,profile+'-final')
 else:
  l.remote(profile,'send-text','--',' F' if app=='bookokrat' else ' f');time.sleep(.3)
  l.remote(profile,'send-text','--','лунных')
  if app=='bookokrat':l.key(profile,'enter')
  l.wait(profile,'3 results' if app=='bookokrat' else 'matches');time.sleep(1);(B/(profile+'-search.txt')).write_text(l.text(profile));snap('global_search')
  l.key(profile,'enter');l.wait(profile,'лунных');time.sleep(1);b.capture(profile,profile+'-result');snap('open_result')
  for cycle in range(5):
   for key in ['page_down']*3+['page_up']*3:
    before=l.text(profile);l.key(profile,key);wait_change(before);time.sleep(.12)
   time.sleep(.5);snap('cycle_'+str(cycle+1))
finally:l.stop_pid(l.ROOT/(profile+'.pid'))
