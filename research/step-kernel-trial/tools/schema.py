import sys,re
for f in sys.argv[1:]:
    try:
        h=open(f,'rb').read(20000).decode('latin-1')
    except Exception as e:
        continue
    m=re.search(r"FILE_SCHEMA\s*\(\s*\(\s*'([^']*)'",h)
    s=m.group(1).split('{')[0].strip() if m else '?'
    print(f"{s[:45]}\t{f}")
