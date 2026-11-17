"""Private, bounded Wayland/Kitty UI audit; never uses the active desktop."""
import argparse, json, os, signal, subprocess, time
from pathlib import Path

ROOT = Path(os.environ['BOTUI_BENCH_DIR']) / 'live'
ROOT.mkdir(exist_ok=True)
RUNTIME = ROOT / 'run'
RUNTIME.mkdir(mode=0o700, exist_ok=True)
BINS = {'baseline': os.environ['BOTUI_BENCH_BOTUI'],
        'current': os.environ.get('BOTUI_BENCH_CURRENT', os.environ['BOTUI_BENCH_BOTUI']),
        'bookokrat': os.environ['BOTUI_BENCH_BOOKOKRAT']}

def environment(profile='compositor'):
    env = dict(os.environ)
    for name in ('DISPLAY', 'MANGO_INSTANCE_SIGNATURE', 'TMUX', 'NO_COLOR', 'COLORTERM'):
        env.pop(name, None)
    env.update(XDG_RUNTIME_DIR=str(RUNTIME), WAYLAND_DISPLAY='wayland-0',
               LIBGL_ALWAYS_SOFTWARE='1', LP_NUM_THREADS='2')
    base = ROOT / profile
    for name, directory in [('HOME','home'), ('XDG_CONFIG_HOME','config'),
                            ('XDG_DATA_HOME','data'), ('XDG_CACHE_HOME','cache'),
                            ('XDG_STATE_HOME','state')]:
        env[name] = str(base/directory)
        Path(env[name]).mkdir(parents=True, exist_ok=True)
    return env

def run(args, **kw):
    return subprocess.run(args, check=True, capture_output=True, text=True,
                          timeout=20, **kw).stdout

def remote(profile, *args):
    return run(['kitty','@','--to','unix:'+str(ROOT/(profile+'.sock')),*args],
               env=environment(profile))

def stop_pid(path):
    if path.exists():
        try:
            pid=int(path.read_text())
            command=(Path('/proc')/str(pid)/'cmdline').read_bytes()
            if str(ROOT).encode() not in command or os.getpgid(pid)!=pid:
                raise RuntimeError('Refusing to stop an unrelated process from a stale PID file: '+str(path))
            os.killpg(pid, signal.SIGTERM)
        except ProcessLookupError: pass
        except FileNotFoundError: pass
        path.unlink()

def init():
    if (ROOT/'mango.pid').exists():
        raise RuntimeError('Private compositor already recorded; reuse or stop it explicitly.')
    config=ROOT/'mango.conf'
    config.write_text('animations=0\n'
                      'monitor_rule=name:HEADLESS-.*,width:1600,height:1000,refresh:30,x:0,y:0,scale:1\n')
    env=environment()
    env.update(WLR_BACKENDS='headless', WLR_RENDERER='pixman', WLR_HEADLESS_OUTPUTS='1')
    env.pop('WAYLAND_DISPLAY',None)
    with (ROOT/'mango.log').open('w') as log:
        proc=subprocess.Popen(['nice','-n','10','taskset','-c','0-1','mango','-c',str(config)],
                              env=env,stdout=log,stderr=log,start_new_session=True)
    (ROOT/'mango.pid').write_text(str(proc.pid))
    for _ in range(100):
        if (RUNTIME/'wayland-0').exists(): return
        if proc.poll() is not None: break
        time.sleep(.1)
    stop_pid(ROOT/'mango.pid')
    raise RuntimeError((ROOT/'mango.log').read_text())

def open_app(profile, app, book, extra=()):
    env=environment(profile)
    if (ROOT/(profile+'.pid')).exists():
        raise RuntimeError('Profile already open; close it explicitly.')
    sock=ROOT/(profile+'.sock');sock.unlink(missing_ok=True)
    command=['nice','-n','10','taskset','-c','0-1','kitty','--config','NONE',
             '--override','allow_remote_control=yes','--override','linux_display_server=wayland',
             '--override','font_family=CaskaydiaMono Nerd Font','--override','font_size=14',
             '--override','background=#150f09','--override','foreground=#c4c3c1',
             '--override','color0=#150f09','--override','background_opacity=1',
             '--override','enable_audio_bell=no','--override','confirm_os_window_close=0',
             '--hold','--start-as','fullscreen','--listen-on','unix:'+str(sock),
             BINS.get(app,app),str(book),*extra]
    with (ROOT/(profile+'.log')).open('w') as log:
        proc=subprocess.Popen(command,env=env,cwd=str(ROOT/profile),stdout=log,stderr=log,
                              start_new_session=True)
    (ROOT/(profile+'.pid')).write_text(str(proc.pid))
    for _ in range(200):
        if sock.exists():
            try:
                frame=remote(profile,'get-text','--extent','screen')
                if frame.strip(): return frame
            except subprocess.SubprocessError: pass
        if proc.poll() is not None: break
        time.sleep(.1)
    stop_pid(ROOT/(profile+'.pid'))
    raise RuntimeError((ROOT/(profile+'.log')).read_text())

def text(profile, ansi=False):
    args=['get-text','--extent','screen']
    if ansi: args.append('--ansi')
    return remote(profile,*args)

def key(profile, *keys):
    for key in keys:
        seq={'enter':'\r','escape':'\x1b','left':'\x1b[D','right':'\x1b[C',
             'page_up':'\x1b[5~','page_down':'\x1b[6~'}.get(key)
        if seq: remote(profile,'send-text','--',seq)
        else: remote(profile,'send-key',key)
        time.sleep(.12)

def wait(profile, label, timeout=10):
    end=time.monotonic()+timeout
    while time.monotonic()<end:
        frame=text(profile)
        if label in frame: return frame
        time.sleep(.1)
    raise AssertionError((label,frame))

def shot(profile,name):
    path=ROOT/(name+'.png')
    run(['grim',str(path)],env=environment(profile))
    path.with_suffix('.txt').write_text(text(profile))
    path.with_suffix('.ansi').write_text(text(profile,True))
    return str(path)

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('action');p.add_argument('profile',nargs='?',default='');p.add_argument('values',nargs='*')
    args=p.parse_args()
    if args.action=='init': init();print(run(['wlr-randr'],env=environment()))
    elif args.action=='open': print(open_app(args.profile,*args.values))
    elif args.action=='text': print(text(args.profile))
    elif args.action=='key': key(args.profile,*args.values)
    elif args.action=='type': print(remote(args.profile,'send-text','--',args.values[0]))
    elif args.action=='shot': print(shot(args.profile,args.values[0]))
    elif args.action=='close': stop_pid(ROOT/(args.profile+'.pid'))
    elif args.action=='stop':
        for pid in ROOT.glob('*.pid'): stop_pid(pid)
    else: raise ValueError(args.action)
