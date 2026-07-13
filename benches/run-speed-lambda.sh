#!/usr/bin/env bash
# Lambda cannot yet bind a deployed host observation to retained, source-sealed
# release evidence.
set -euo pipefail

echo "Lambda speed runner cannot currently produce retained release evidence: a Lambda host-observation/source-seal protocol is required. Refusing before AWS or network commands." >&2
exit 2
