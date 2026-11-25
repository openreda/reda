# FreeCAD (OpenCascade) reference figures for each sample model.
# run: freecadcmd fc_ref.py   with env MODELS=<dir> OUT=<file.jsonl>
import os, json, time
import Part
models, out = os.environ["MODELS"], os.environ["OUT"]
files = sorted(os.path.join(d, f) for d, _, fs in os.walk(models) for f in fs if f.lower().endswith((".step", ".stp")))
with open(out, "w") as o:
    for f in files:
        r = {"file": os.path.relpath(f, models)}
        t = time.time()
        try:
            s = Part.Shape(); s.read(f)
            b = s.BoundBox
            r.update(ok=True, read_s=round(time.time() - t, 2), solids=len(s.Solids), shells=len(s.Shells), faces=len(s.Faces),
                     volume=sum(x.Volume for x in s.Solids), area=s.Area, valid=s.isValid(),
                     bbox_min=[b.XMin, b.YMin, b.ZMin], bbox_max=[b.XMax, b.YMax, b.ZMax])
        except Exception as e:
            r.update(ok=False, error=str(e)[:300])
        o.write(json.dumps(r) + "\n"); o.flush()
