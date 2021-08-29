#!/bin/bash -ex

cd "$(dirname "$0")"

docker build -t hypercubelabs/snapcraft .
docker push hypercubelabs/snapcraft
