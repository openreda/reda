# FreeCAD (OpenCascade XDE) acceptance check for written STEP files.
# run: freecadcmd fc_check.py   with env FILES=<a.step:b.step:...> OUT=<file.jsonl>
import os, json
import FreeCAD as App, Part, Import
out = open(os.environ["OUT"], "w")
for f in os.environ["FILES"].split(":"):
    r = {"file": os.path.basename(f)}
    try:
        s = Part.Shape(); s.read(f)
        r.update(solids=len(s.Solids), faces=len(s.Faces), valid=s.isValid(),
                 volume=round(sum(x.Volume for x in s.Solids), 3),
                 bbox=[round(v, 3) for v in (s.BoundBox.XMin, s.BoundBox.YMin, s.BoundBox.ZMin,
                                              s.BoundBox.XMax, s.BoundBox.YMax, s.BoundBox.ZMax)])
    except Exception as e:
        r["shape_error"] = str(e)[:300]
    try:
        doc = App.newDocument("chk")
        Import.insert(f, doc.Name)
        objs = []
        for o in doc.Objects:
            e = {"label": o.Label, "type": o.TypeId}
            if hasattr(o, "Placement"):
                p = o.Placement
                e["pos"] = [round(v, 3) for v in p.Base]
                e["rot_z_deg"] = round(p.Rotation.toEuler()[0], 2)
            for attr in ("ShapeAppearance", "ShapeMaterial"):
                if hasattr(o, attr):
                    try:
                        m = getattr(o, attr)
                        m = m[0] if isinstance(m, (list, tuple)) else m
                        e["colour"] = [round(c, 3) for c in m.DiffuseColor[:3]]
                    except Exception:
                        pass
            if hasattr(o, "ViewObject") and o.ViewObject is not None and hasattr(o.ViewObject, "DiffuseColor"):
                e["face_colours"] = len(set(tuple(round(c, 3) for c in x[:3]) for x in o.ViewObject.DiffuseColor))
            objs.append(e)
        r["objects"] = objs
        App.closeDocument(doc.Name)
    except Exception as e:
        r["import_error"] = str(e)[:300]
    out.write(json.dumps(r) + "\n"); out.flush()
