#!/usr/bin/env bash
# The simulation's tests (property tests too) as WebAssembly in wasmtime, part PART of PARTS:
#
#   tools/wasm-sim-tests.sh 1 3
#
# wasm32-wasip1 has no threads, so each test binary runs its tests one at a time. This builds every test binary
# once, deals them out over the parts (the slowest first, so each part gets one of the long ones) and runs this
# part's binaries side by side, one per CPU. Part 1 also runs the doc tests. Locally it needs wasmtime on PATH.
set -euo pipefail
cd "$(dirname "$0")/.."
part=${1:?part}
parts=${2:?parts}

# The slowest binaries in CI, slowest first (minutes under wasmtime on 13 Nov). Only the order of the deal: a
# binary missing here is still run, after these.
slow="slope_caves shallows open_canyon crabsim economy_pacing dive_budget economy_every_way_pays first_hours
plate_d_proofs lower_canyon octopus eastern_upper_route dig_gates canyon coop properties paging_progress"

built=()
while IFS= read -r line; do built+=("$line"); done < <(
  cargo test -p crabsim --locked --target wasm32-wasip1 --no-run --message-format=json |
    jq -r 'select(.reason == "compiler-artifact" and .executable != null) | "\(.target.name) \(.executable)"' | sort -u)
[ "${#built[@]}" -gt 0 ] || { echo "no test binaries built" >&2; exit 1; }

ordered=()
for name in $slow; do
  for line in "${built[@]}"; do
    [ "${line%% *}" = "$name" ] && ordered+=("$line")
  done
done
for line in "${built[@]}"; do
  case " $(echo $slow) " in *" ${line%% *} "*) ;; *) ordered+=("$line") ;; esac
done

mine=()
for i in "${!ordered[@]}"; do
  [ $((i % parts + 1)) -eq "$part" ] && mine+=("${ordered[$i]##* }")
done
echo "part $part of $parts: ${#mine[@]} of ${#ordered[@]} test binaries"

logs=$(mktemp -d)
run() {
  local exe=$1 log
  log="$logs/$(basename "$exe").log"
  if wasmtime "$exe" > "$log" 2>&1; then
    echo "ok   $(basename "$exe") ($(grep -m1 '^test result' "$log" | sed 's/.*finished in/in/'))"
  else
    echo "FAIL $(basename "$exe")"
    cat "$log"
    return 1
  fi
}
export -f run
export logs
status=0
printf '%s\n' "${mine[@]}" | xargs -P "$(nproc 2>/dev/null || sysctl -n hw.ncpu)" -I{} bash -c 'run "$@"' _ {} || status=1
if [ "$part" -eq 1 ]; then
  cargo test -p crabsim --locked --target wasm32-wasip1 --doc || status=1
fi
exit $status
