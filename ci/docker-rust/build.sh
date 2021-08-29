#!/bin/bash -ex

cd "$(dirname "$0")"

docker build -t hypercubelabs/rust .

read -r rustc version _ < <(docker run hypercubelabs/rust rustc --version)
[[ $rustc = rustc ]]
docker tag hypercubelabs/rust:latest hypercubelabs/rust:"$version"

docker push hypercubelabs/rust
