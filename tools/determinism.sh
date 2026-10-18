#!/usr/bin/env bash
# Proves the simulation plays out bit for bit the same natively and as WebAssembly (the browser's kind of code):
# each script below runs in the native crabsim and in crabsim compiled to wasm32-wasip1 (run by wasmtime), and the
# full tick-by-tick state dumps and the final state hashes must match exactly. CI runs it; locally it needs
# wasmtime on PATH.
#
#   tools/determinism.sh
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --release --locked -q -p crabsim
cargo build --release --locked -q -p crabsim --target wasm32-wasip1
native=target/release/crabsim
wasm="wasmtime target/wasm32-wasip1/release/crabsim.wasm"

while IFS= read -r script; do
  a=$($native --script "$script" --seconds 60 --dump - --quiet | sha256sum)
  b=$($wasm --script "$script" --seconds 60 --dump - --quiet | sha256sum)
  ha=$($native --script "$script" --seconds 60 | grep '^state hash')
  hb=$($wasm --script "$script" --seconds 60 | grep '^state hash')
  if [ "$a" != "$b" ] || [ "$ha" != "$hb" ]; then
    echo "DIFFERENT: native ${a:0:16} ($ha), wasm ${b:0:16} ($hb) for: $script" >&2
    exit 1
  fi
  echo "same: ${ha%% (*} (dump ${a:0:16}) for: $script"
done << 'SCRIPTS'
t=0 walk=right; t=0.4 jump; t=0.7 jump; t=0.9 release; t=1.6 dodge; t=1.8 walk=-0.4; t=2.2 walk=right tap; t=3 walk=right jump; t=3.4 jump; t=4 dodge; t=4.1 jump; t=5.5 walk=left; t=6 jump; t=6.1 release; t=6.6 sprint; t=7.4 slow; t=8 walk=right
t=0 walk=right; t=1 jump; t=1.45 jump; t=3 walk=right sprint; t=6 slow; t=9 jump; t=12 walk=left; t=20 walk=right down=on; t=30 down=off jump
t=0 walk=left; t=0.5 jump; t=2 walk=right; t=2.5 dodge; t=10 walk=right sprint; t=40 walk=stop
t=0 walk=right; t=0.5 smash; t=0.6 jab; t=1 jab; t=1.05 jab; t=2 walk=left smash; t=2.2 dodge; t=3 jump; t=3.2 smash
t=0 spawn=fixture@320,0; t=0 walk=right; t=0.2 walk=stop; t=0.4 block; t=0.6 unblock; t=0.65 parry; t=0.8 smash; t=1.3 jab; t=1.6 walk=left dodge; t=2 walk=right; t=2.3 smash; t=3 guard; t=5 unblock walk=right smash; t=8 spawn=fixture@900,0
t=0 feel=momentum spawn=fixture@320,0; t=0 walk=right sprint; t=0.3 charge; t=0.6 loose; t=1 feel=silksong walk=stop; t=1.2 smash; t=1.4 jab; t=2 feel=heavy smash; t=2.3 jab; t=2.35 jab; t=3 feel=expedition parry; t=3.2 smash; t=4 feel=snappy jab; t=4.05 jab; t=4.1 smash
t=0 feel=tank spawn=fixture@240,0; t=0.22 parry; t=0.27 smash; t=2 feel=hybrid; t=3 spawn=mantis@700,0; t=3 walk=right; t=4 walk=stop; t=6 charge; t=6.4 loose; t=8 walk=left sprint
SCRIPTS
echo "native and wasm agree"
