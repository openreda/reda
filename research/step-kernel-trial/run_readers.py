#!/usr/bin/env python3
"""Run every reader harness over the model sample. One subprocess per file, with a timeout.

usage: run_readers.py <models-dir> <target-root> <results-dir> [candidate ...]
"""
import json, os, re, subprocess, sys, time
from concurrent.futures import ThreadPoolExecutor

BIN = {"truck-released": "truck-released", "truck-master": "truck-master", "step-io": "step-io-trial",
       "solidcore": "step-trial-solidcore", "foxtrot": "step-trial-foxtrot", "opencascade-rs": "step-trial-occt"}
TIMEOUT = 300
MEM_CAP = 4 * 1024 ** 3  # address-space cap per read; truck-released reached 26 GB resident uncapped

def schema(path):
    head = open(path, "rb").read(20000).decode("latin-1")
    m = re.search(r"FILE_SCHEMA\s*\(\s*\(\s*'([^']*)'", head)
    s = m.group(1).split("{")[0].strip() if m else "?"
    return {"CONFIG_CONTROL_DESIGN": "AP203", "AUTOMOTIVE_DESIGN": "AP214", "AUTOMOTIVE_DESIGN_CC2": "AP214"}.get(
        s, "AP242" if s.startswith("AP242") else s)

def run(exe, path):
    t = time.time()
    try:
        p = subprocess.run(["/usr/bin/prlimit", f"--as={MEM_CAP}", "/usr/bin/time", "-f", "MAXRSS_KB %M",
                            exe, "read", path], capture_output=True, timeout=TIMEOUT)
    except subprocess.TimeoutExpired:
        return {"outcome": "timeout", "wall_s": TIMEOUT}
    wall = round(time.time() - t, 2)
    errl = p.stderr.decode("utf-8", "replace").strip().splitlines()
    rss = next((int(l.split()[1]) for l in reversed(errl) if l.startswith("MAXRSS_KB ")), None)
    errl = [l for l in errl if not l.startswith("MAXRSS_KB ") and not l.startswith("Command ")]
    out = p.stdout.decode("utf-8", "replace").strip().splitlines()
    rec = None
    for line in reversed(out):
        try:
            rec = json.loads(line); break
        except ValueError:
            pass
    if p.returncode != 0 or rec is None:
        err = errl
        msg = next((l for l in err if "panicked" in l or "memory allocation" in l), err[-1] if err else "")
        idx = err.index(msg) if msg in err else -1
        detail = (msg + " " + (err[idx + 1] if 0 <= idx < len(err) - 1 else ""))[:300]
        kind = ("memory cap" if "memory allocation" in msg else "panic" if "panicked" in msg
                else f"exit {p.returncode}")
        return {"outcome": kind, "wall_s": wall, "max_rss_kb": rss, "error": detail}
    rec["outcome"] = "ok" if rec.get("parse_ok") and not rec.get("error") else "error"
    rec["wall_s"] = wall
    rec["max_rss_kb"] = rss
    return rec

def main():
    models, target, results = sys.argv[1:4]
    cands = sys.argv[4:] or list(BIN)
    files = sorted(os.path.join(d, f) for d, _, fs in os.walk(models) for f in fs
                   if f.lower().endswith((".step", ".stp")))
    os.makedirs(results, exist_ok=True)
    for c in cands:
        exe = os.path.join(target, c, "release", BIN[c])
        if not os.path.exists(exe):
            print("missing", exe); continue
        with ThreadPoolExecutor(3) as ex:
            recs = list(ex.map(lambda f: run(exe, f), files))
        with open(os.path.join(results, c + ".jsonl"), "w") as o:
            for f, r in zip(files, recs):
                r.update(file=os.path.relpath(f, models), bytes=os.path.getsize(f), schema=schema(f))
                o.write(json.dumps(r) + "\n")
        n = {}
        for r in recs: n[r["outcome"]] = n.get(r["outcome"], 0) + 1
        print(c, n)

main()
