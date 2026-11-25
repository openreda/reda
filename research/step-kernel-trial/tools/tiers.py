import json, glob, os, sys
S = sys.argv[1]
ref = {json.loads(l)['file']: json.loads(l) for l in open(f'{S}/out/fc_ref.jsonl')}
src_col = {json.loads(l)['file']: (json.loads(l).get('colours_distinct') or 0) for l in open(f'{S}/out/results/step-io.jsonl')}
print('reader | parsed | all faces meshed | +volume within 1% | +closed | colours shown (of', sum(1 for v in src_col.values() if v), 'with colours) | crashes')
for c in sorted(glob.glob(f'{S}/out/results/*.jsonl')):
    rows = [json.loads(l) for l in open(c)]
    n = os.path.basename(c)[:-6]
    parsed = sum(r['outcome'] in ('ok', 'error') and r.get('parse_ok') is not False for r in rows)
    allf = [r for r in rows if r['outcome'] == 'ok' and r.get('triangles') and (r.get('faces_meshed') or 0) >= (r.get('faces_total') or 0)]
    vol = [r for r in allf if ref[r['file']].get('volume') and r.get('volume') and abs(r['volume'] - ref[r['file']]['volume']) <= 0.01 * ref[r['file']]['volume']]
    closed = [r for r in vol if not r.get('open_edges')]
    col = sum(1 for r in rows if (r.get('colours_distinct') or 0) > 0 and src_col[r['file']])
    hard = sum(r['outcome'] in ('panic', 'memory cap', 'timeout') or r['outcome'].startswith('exit') for r in rows)
    print(f"{n} | {parsed} | {len(allf)} | {len(vol)} | {len(closed)} | {col} | {hard}")
