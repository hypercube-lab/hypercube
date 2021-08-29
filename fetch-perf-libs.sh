#!/bin/bash -e

if [[ $(uname) != Linux ]]; then
  echo Performance libraries are only available for Linux
  exit 1
fi

if [[ $(uname -m) != x86_64 ]]; then
  echo Performance libraries are only available for x86_64 architecture
  exit 1
fi

mkdir -p target/perf-libs
(
  cd target/perf-libs
  (
    set -x
    curl https://hypercube-perf.s3.amazonaws.com/v0.9.0/x86_64-unknown-linux-gnu/hypercube-perf.tgz | tar zxvf -
  )

  if [[ -r /usr/local/cuda/version.txt && -r cuda-version.txt ]]; then
    if ! diff /usr/local/cuda/version.txt cuda-version.txt > /dev/null; then
        echo ==============================================
        echo Warning: possible CUDA version mismatch
        echo
        echo "Expected version: $(cat cuda-version.txt)"
        echo "Detected version: $(cat /usr/local/cuda/version.txt)"
        echo ==============================================
    fi
  else
    echo ==============================================
    echo Warning: unable to validate CUDA version
    echo ==============================================
  fi

  echo "Downloaded hypercube-perf version: $(cat hypercube-perf-HEAD.txt)"
)

exit 0
