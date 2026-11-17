# Reproduce the native audit

These are the scripts used for the 17 November comparison, with machine-local
paths replaced by environment variables. They target the three named editions
in the report and are not part of CI. Requirements: Linux `/proc`, Mango,
Kitty with remote control, grim, taskset, Python 3 and Pillow. CPUs 0–1 must
exist. A private headless compositor is used; the active desktop is untouched.

Copy `cases.example.json` and fill in absolute paths to your copies of the
three editions. Use the hashes in the report to check that the editions match.
Set these variables to absolute paths, keeping them in the same shell:

```sh
export BOTUI_BENCH_DIR="$(mktemp -d /tmp/botui-bench.XXXXXX)"
export BOTUI_BENCH_BOTUI=/absolute/path/to/botui-baseline
export BOTUI_BENCH_CURRENT=/absolute/path/to/botui-current
export BOTUI_BENCH_BOOKOKRAT=/absolute/path/to/bookokrat
export BOTUI_BENCH_CASES=/absolute/path/to/cases.json
python3 docs/benchmarks/2026-11-17-tools/live.py init
python3 docs/benchmarks/2026-11-17-tools/native.py all
python3 docs/benchmarks/2026-11-17-tools/growth.py baseline amber
python3 docs/benchmarks/2026-11-17-tools/growth.py bookokrat amber
python3 docs/benchmarks/2026-11-17-tools/growth.py baseline lessons
python3 docs/benchmarks/2026-11-17-tools/growth.py bookokrat lessons
python3 docs/benchmarks/2026-11-17-tools/kitty-lifetime.py current "$BOTUI_BENCH_CURRENT"
python3 docs/benchmarks/2026-11-17-tools/live.py stop
```

Run sequentially, without a build or another graphics test in progress. Each
application run closes its own terminal in `finally`; `live.py stop` also closes
the private compositor. If interrupted, run that final command explicitly.
Every launch has its own profile. Use a fresh output directory for a fresh
series: reusing profiles also reuses their caches and positions.

The growth test stops if the owned terminal/process tree exceeds 1.3 GiB RSS.
The Kitty lifetime test also checks pixel equality after returning to page 1.
The timing test waits at most 45 seconds for readiness. Measurements, screenshots,
profiles and logs stay under `BOTUI_BENCH_DIR`; preserve the JSON and any wanted
screenshots before removing that temporary directory. The input books are read
in place and are not modified. Search and saved positions belong to the isolated
profiles, not the normal application profile.

The existing PTY check can be reproduced with
`scripts/smoke-reader-frame.py --help`; its `--metrics` option records the
interaction/reflow measurements separately from native graphics timing.
