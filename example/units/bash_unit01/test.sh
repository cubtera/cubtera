#!/bin/bash
set -euo pipefail

echo "=== cubtera bash_unit01 ==="
echo "org=${CUBTERA_ORG:-<unset>} unit=${CUBTERA_UNIT:-<unset>} dim_tree=${CUBTERA_DIM_TREE:-<unset>}"
echo "command args: $*"
echo

echo "--- dc dimension (cubtera_dim_dc.json) ---"
cat cubtera_dim_dc.json
echo

if grep -q '"dim_service_name": null' cubtera_dim_service.json 2>/dev/null; then
  echo "--- service dimension: declared as an optional dim (optDims), not supplied this run ---"
  cat cubtera_dim_service.json
elif [ -f cubtera_dim_service.json ]; then
  echo "--- service dimension (cubtera_dim_service.json), resolved via an extra -d ---"
  cat cubtera_dim_service.json
else
  echo "--- service dimension: no cubtera_dim_service.json materialized ---"
fi
echo

echo "--- required include (greeting.txt) ---"
cat greeting.txt
echo

echo "--- optional include (remote_optional.txt, from example/remote_folder) ---"
cat remote_optional.txt 2>/dev/null || echo "(not present)"
echo

echo "--- optional include that was skipped (skipped.txt) ---"
if [ -f skipped.txt ]; then
  echo "unexpectedly present"
else
  echo "(missing, as expected - its source file doesn't exist)"
fi
echo

echo "=== done ==="
