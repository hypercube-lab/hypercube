#!/bin/bash -ex

[[ $(uname) = Linux ]] || exit 1
[[ $USER = root ]] || exit 1

adduser hypercube --gecos "" --disabled-password --quiet
adduser hypercube sudo
echo "hypercube ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers
id hypercube

[[ -r /hypercube-id_ecdsa ]] || exit 1
[[ -r /hypercube-id_ecdsa.pub ]] || exit 1

sudo -u hypercube bash -c "
  mkdir -p /home/hypercube/.ssh/
  cd /home/hypercube/.ssh/
  cp /hypercube-id_ecdsa.pub authorized_keys
  umask 377
  cp /hypercube-id_ecdsa id_ecdsa
  echo \"
    Host *
    BatchMode yes
    IdentityFile ~/.ssh/id_ecdsa
    StrictHostKeyChecking no
  \" > config
"
