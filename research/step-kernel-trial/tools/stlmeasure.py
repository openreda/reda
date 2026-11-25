import sys, struct, math
def tris(p):
    d = open(p, 'rb').read()
    if d[:5] == b'solid' and b'facet' in d[:400]:
        v = []
        for l in d.decode('latin-1').splitlines():
            l = l.split()
            if l and l[0] == 'vertex': v.append(tuple(map(float, l[1:4])))
        return [v[i:i+3] for i in range(0, len(v), 3)]
    n = struct.unpack('<I', d[80:84])[0]; out = []
    for i in range(n):
        o = 84 + 50*i; f = struct.unpack('<12f', d[o:o+48]); out.append([f[3:6], f[6:9], f[9:12]])
    return out
for p in sys.argv[1:]:
    T = tris(p); vol = 0; key = lambda q: tuple(round(c/1e-4) for c in q); edges = {}
    lo = [1e9]*3; hi = [-1e9]*3
    for a, b, c in T:
        vol += (a[0]*(b[1]*c[2]-b[2]*c[1]) - a[1]*(b[0]*c[2]-b[2]*c[0]) + a[2]*(b[0]*c[1]-b[1]*c[0]))/6
        for q in (a, b, c):
            for i in range(3): lo[i] = min(lo[i], q[i]); hi[i] = max(hi[i], q[i])
        for u, w in ((a, b), (b, c), (c, a)):
            e = tuple(sorted((key(u), key(w)))); edges[e] = edges.get(e, 0) + 1
    print(f"{p}\ttris={len(T)}\tvolume={vol:.1f}\topen_edges={sum(1 for x in edges.values() if x == 1)}\tbbox={[round(x,2) for x in lo]}..{[round(x,2) for x in hi]}")
