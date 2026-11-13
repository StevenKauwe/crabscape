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
# Timing tests: run alone after the others, as `cargo test` would, so the side-by-side runs don't slow them.
alone="headless_speed"

built=()
while IFS= read -r line; do built+=("$line"); done < <(
  cargo test -p crabsim --locked --target wasm32-wasip1 --no-run --message-format=json |
    jq -r 'select(.reason == "compiler-artifact" and .executable != null and .profile.test) | "\(.target.name) \(.executable)"' |
    sort -u)
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

# The longest binaries (one took 6 to 8 minutes, longer than the rest of a part) are cut into this many runs of
# their tests each (`--exact` names), so no single run bounds a part.
cuts=4
heavy="slope_caves shallows open_canyon crabsim economy_pacing dive_budget economy_every_way_pays first_hours"
units=()
for line in "${ordered[@]}"; do
  name=${line%% *} exe=${line##* }
  case " $(echo $heavy) " in
    *" $name "*)
      tests=()
      while IFS= read -r t; do tests+=("$t"); done < <(wasmtime "$exe" --list --format terse 2>/dev/null |
        sed -n 's/: test$//p')
      if [ "${#tests[@]}" -lt 2 ]; then
        units+=("$exe")
        continue
      fi
      for c in $(seq 0 $((cuts - 1))); do
        chunk=()
        for j in "${!tests[@]}"; do
          [ $((j % cuts)) -eq "$c" ] && chunk+=("${tests[$j]}")
        done
        [ "${#chunk[@]}" -eq 0 ] || units+=("$exe --exact ${chunk[*]}")
      done
      ;;
    *) units+=("$exe") ;;
  esac
done

mine=()
solo=()
for i in "${!units[@]}"; do
  [ $((i % parts + 1)) -eq "$part" ] || continue
  exe=${units[$i]%% *}
  name=$(basename "$exe" .wasm)
  case " $alone " in
    *" ${name%-*} "*) solo+=("${units[$i]}") ;;
    *) mine+=("${units[$i]}") ;;
  esac
done
echo "part $part of $parts: $((${#mine[@]} + ${#solo[@]})) of ${#units[@]} runs (${#ordered[@]} test binaries)"

logs=$(mktemp -d)
run() {
  local exe=${1%% *} args="" log
  [ "$1" = "$exe" ] || args=${1#* }
  log=$(mktemp "$logs/$(basename "$exe").XXXX")
  # shellcheck disable=SC2086 # test names have no spaces
  if wasmtime "$exe" $args > "$log" 2>&1; then
    echo "ok   $(basename "$exe")${args:+ (part of its tests)} ($(grep -m1 '^test result' "$log" | sed 's/.*finished in/in/'))"
  else
    echo "FAIL $(basename "$exe") $args"
    cat "$log"
    return 1
  fi
}
export -f run
export logs
status=0
[ "${#mine[@]}" -eq 0 ] || printf '%s\n' "${mine[@]}" | xargs -P "$(nproc 2>/dev/null || sysctl -n hw.ncpu)" -I{} bash -c 'run "$@"' _ {} || status=1
for exe in ${solo[@]+"${solo[@]}"}; do
  run "$exe" || status=1
done
if [ "$part" -eq 1 ]; then
  cargo test -p crabsim --locked --target wasm32-wasip1 --doc || status=1
fi
exit $status
