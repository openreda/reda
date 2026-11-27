#!/usr/bin/env bash
# Downloads the openly fetchable models of the STEP model corpus (docs/spec/v0.3/testing.md#model-corpus):
# every row of step-corpus.tsv marked `fetch` yes, from its pinned commit, into <folder>/<name>,
# each checked against its SHA-256. A model already there with the right hash is kept.
#
#   fetch.sh <folder> [manifest]
#
# The manifest defaults to the step-corpus.tsv beside this script. Needs bash, curl and
# sha256sum (or shasum). Exits non-zero when any model could not be fetched or its hash differs.
set -u

if [ $# -lt 1 ] || [ $# -gt 2 ]; then
    echo "usage: $0 <folder> [manifest]" >&2
    exit 2
fi
folder=$1
manifest=${2:-"$(cd "$(dirname "$0")" && pwd)/step-corpus.tsv"}

if command -v sha256sum >/dev/null 2>&1; then
    hash_of() { sha256sum "$1" | cut -d' ' -f1; }
else
    hash_of() { shasum -a 256 "$1" | cut -d' ' -f1; }
fi

fetched=0
kept=0
failed=0
tab=$(printf '\t')
# Columns: name url commit sha256 schema fetch colours volume bbox_min bbox_max
while IFS="$tab" read -r name url _commit sha256 _schema fetch _rest; do
    [ "$name" = "name" ] && continue
    [ "$fetch" = "yes" ] || continue
    dest="$folder/$name"
    if [ -f "$dest" ] && [ "$(hash_of "$dest")" = "$sha256" ]; then
        kept=$((kept + 1))
        continue
    fi
    mkdir -p "$(dirname "$dest")"
    if ! curl -fsSL --retry 3 --retry-delay 5 -o "$dest.part" "$url"; then
        echo "failed: $name ($url)" >&2
        rm -f "$dest.part"
        failed=$((failed + 1))
        continue
    fi
    got=$(hash_of "$dest.part")
    if [ "$got" != "$sha256" ]; then
        echo "failed: $name has SHA-256 $got, not $sha256" >&2
        rm -f "$dest.part"
        failed=$((failed + 1))
        continue
    fi
    mv "$dest.part" "$dest"
    fetched=$((fetched + 1))
done < "$manifest"

echo "fetched $fetched, kept $kept, failed $failed"
[ "$failed" -eq 0 ]
