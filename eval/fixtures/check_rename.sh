#!/bin/sh
# Judge for rename-save: a persisted .bndb must exist in the workspace (beside
# the target, the target itself, or wherever the tool under test saves it)
# and its function names must include decrypt_me.
dbs=$(find . -path ./.claude -prune -o -path ./.grok -prune -o -path ./.agents -prune -o \
  -name '*.bndb' -type f -print)
[ -n "$dbs" ] || { echo "no .bndb database was saved" >&2; exit 1; }
for db in $dbs; do
  out=$(bnx exec "$db" --timeout 300 'print("\n".join(f.name for f in bv.functions))' 2>&1)
  # 2: analysis still running (a rename can trigger it); the names are there.
  echo "$out" | grep -q decrypt_me && exit 0
done
echo "decrypt_me not found in $(echo $dbs)" >&2
exit 1
