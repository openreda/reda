# Colours as FreeCAD's GUI STEP import (ImportGui, OpenCascade XDE) sees them.
# run: QT_QPA_PLATFORM=offscreen freecad fc_colours.py   with env FILES, OUT
import os, json
import FreeCAD as App, FreeCADGui as Gui
import ImportGui
out = open(os.environ["OUT"], "w")
for f in os.environ["FILES"].split(":"):
    r = {"file": os.path.basename(f)}
    try:
        doc = App.newDocument("c")
        ImportGui.insert(f, doc.Name)
        cols = {}
        for o in doc.Objects:
            if o.TypeId != "Part::Feature":
                continue
            vo = o.ViewObject
            s = set()
            try:
                for m in vo.ShapeAppearance:
                    s.add(tuple(round(c, 2) for c in m.DiffuseColor[:3]))
            except Exception:
                s.add(tuple(round(c, 2) for c in vo.ShapeColor[:3]))
            cols[o.Label] = sorted(s)
        r["colours"] = cols
        App.closeDocument(doc.Name)
    except Exception as e:
        r["error"] = str(e)[:300]
    out.write(json.dumps(r) + "\n"); out.flush()
Gui.getMainWindow().close()
App.closeDocument if False else None
os._exit(0)
