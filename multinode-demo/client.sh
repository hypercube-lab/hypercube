#!/bin/bash -e

here=$(dirname "$0")
# shellcheck source=multinode-demo/common.sh
source "$here"/common.sh

usage() {
  if [[ -n $1 ]]; then
    echo "$*"
    echo
  fi
  echo "usage: $0 [extra args]"
  echo
  echo " Run bench-tps "
  echo
  echo "   extra args: additional arguments are pass along to hypercube-bench-tps"
  echo
  exit 1
}

if [[ -z $1 ]]; then # default behavior
  $xpz_bench_tps --identity config-private/client-id.json --network 127.0.0.1:8001 --duration 90
else
  $xpz_bench_tps "$@"
fi
