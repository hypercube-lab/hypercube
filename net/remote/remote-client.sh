#!/bin/bash -e

cd "$(dirname "$0")"/../..

echo "$(date) | $0 $*" > client.log

deployMethod="$1"
entrypointIp="$2"
numNodes="$3"
RUST_LOG="$4"
export RUST_LOG=${RUST_LOG:-hypercube=info} # if RUST_LOG is unset, default to info

missing() {
  echo "Error: $1 not specified"
  exit 1
}

[[ -n $deployMethod ]] || missing deployMethod
[[ -n $entrypointIp ]] || missing entrypointIp
[[ -n $numNodes ]]     || missing numNodes

source net/common.sh
loadConfigFile

threadCount=$(nproc)
if [[ $threadCount -gt 4 ]]; then
  threadCount=4
fi

case $deployMethod in
snap)
  net/scripts/rsync-retry.sh -vPrc "$entrypointIp:~/hypercube/hypercube.snap" .
  sudo snap install hypercube.snap --devmode --dangerous

  xpz_bench_tps=/snap/bin/hypercube.bench-tps
  xpz_keygen=/snap/bin/hypercube.keygen
  ;;
local)
  PATH="$HOME"/.cargo/bin:"$PATH"
  export USE_INSTALL=1
  export XPZ_DEFAULT_METRICS_RATE=1

  net/scripts/rsync-retry.sh -vPrc "$entrypointIp:~/.cargo/bin/hypercube*" ~/.cargo/bin/
  xpz_bench_tps=hypercube-bench-tps
  xpz_keygen=hypercube-keygen
  ;;
*)
  echo "Unknown deployment method: $deployMethod"
  exit 1
esac

scripts/oom-monitor.sh > oom-monitor.log 2>&1 &
scripts/net-stats.sh  > net-stats.log 2>&1 &

! tmux list-sessions || tmux kill-session

clientCommand="\
  $xpz_bench_tps \
    --network $entrypointIp:8001 \
    --identity client.json \
    --num-nodes $numNodes \
    --duration 600 \
    --sustained \
    --threads $threadCount \
"

keygenCommand="$xpz_keygen -o client.json"
tmux new -s hypercube-bench-tps -d "
  [[ -r client.json ]] || {
    echo '$ $keygenCommand'  | tee -a client.log
    $keygenCommand >> client.log 2>&1
  }

  while true; do
    echo === Client start: \$(date) | tee -a client.log
    $metricsWriteDatapoint 'testnet-deploy client-begin=1'
    echo '$ $clientCommand' | tee -a client.log
    $clientCommand >> client.log 2>&1
    $metricsWriteDatapoint 'testnet-deploy client-complete=1'
  done
"
sleep 1
tmux capture-pane -t hypercube-bench-tps -p -S -100
