#!/bin/bash
#
# Snap daemons have no access to the environment so |snap set hypercube ...| is
# used to set runtime configuration.
#
# This script exports the snap runtime configuration options back as
# environment variables before invoking the specified program
#

if [[ -d $SNAP ]]; then # Running inside a Linux Snap?
  RUST_LOG="$(snapctl get rust-log)"
  XPZ_CUDA="$(snapctl get enable-cuda)"
  XPZ_DEFAULT_METRICS_RATE="$(snapctl get default-metrics-rate)"
  XPZ_METRICS_CONFIG="$(snapctl get metrics-config)"

  export RUST_LOG
  export XPZ_CUDA
  export XPZ_DEFAULT_METRICS_RATE
  export XPZ_METRICS_CONFIG
fi

exec "$@"
