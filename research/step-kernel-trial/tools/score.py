import json, sys, os
ref = {}
for l in open(sys.argv[1]):
    r = json.loads(l); ref[r["file"]] = r
def close(a, b, rel=0.01):
    return a is not None and b is not None and abs(a - b) <= rel * max(abs(b), 1e-6)
for c in sys.argv[2:]:
    rows = [json.loads(l) for l in open(c)]
    name = os.path.basename(c)[:-6]
    tally = {}
    bad = []
    for r in rows:
        f = ref.get(r["file"], {})
        o = r["outcome"]
        if o == "ok":
            ft, fm = r.get("faces_total"), r.get("faces_meshed")
            if r.get("triangles") is None:
                o = "parsed (no mesh)"
            elif fm is not None and ft and fm < ft:
                o = "partial mesh"
            elif r.get("open_edges"):
                o = "mesh not closed"
            elif f.get("volume") and not close(r.get("volume"), f["volume"]):
                o = "volume off >1%"
            else:
                o = "good"
        tally[o] = tally.get(o, 0) + 1
        if o not in ("good", "parsed (no mesh)"):
            bad.append((r["file"], o, (r.get("error") or "")[:90], r.get("max_rss_kb")))
        r["grade"] = o
    rss = sorted(x.get("max_rss_kb") or 0 for x in rows)
    col = sum(1 for r in rows if (r.get("colours_distinct") or 0) > 0)
    srccol = None
    print(f"== {name}: {tally}  files-with-colours={col}  rss-median={rss[len(rss)//2]//1024}MB rss-max={rss[-1]//1024}MB  wall-total={sum(r.get('wall_s',0) for r in rows):.1f}s")
    for b in bad: print("   ", *b)
