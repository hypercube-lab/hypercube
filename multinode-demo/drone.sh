#!/bin/bash
#
# Starts an instance of hypercube-drone
#
here=$(dirname "$0")

# shellcheck source=multinode-demo/common.sh
source "$here"/common.sh

usage() {
  if [[ -n $1 ]]; then
    echo "$*"
    echo
  fi
  echo "usage: $0 [network entry point]"
  echo
  echo " Run an airdrop drone for the specified network"
  echo
  exit 1
}

read -r _ leader_address shift < <(find_leader "${@:1:1}")
shift "$shift"

[[ -f "$XPZ_CONFIG_PRIVATE_DIR"/mint.json ]] || {
  echo "$XPZ_CONFIG_PRIVATE_DIR/mint.json not found, create it by running:"
  echo
  echo "  ${here}/setup.sh -t leader"
  exit 1
}

set -ex

trap 'kill "$pid" && wait "$pid"' INT TERM
$xpz_drone \
  --keypair "$XPZ_CONFIG_PRIVATE_DIR"/mint.json \
  --network "$leader_address" \
  > >($drone_logger) 2>&1 &
pid=$!
wait "$pid"
