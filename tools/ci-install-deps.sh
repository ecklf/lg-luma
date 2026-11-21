#!/usr/bin/env bash
set -euo pipefail

# Hosted runners can prefer an unreachable regional mirror for every index and
# package request. Use the official HTTPS archive before apt resolves mirror URLs.
mirror_list="${APT_MIRROR_LIST:-/etc/apt/apt-mirrors.txt}"
if [ -f "$mirror_list" ]; then
  printf 'https://archive.ubuntu.com/ubuntu/\n' | sudo tee "$mirror_list" > /dev/null
fi

apt_options=(
  -o Acquire::Retries=3
  -o Acquire::http::Timeout=30
  -o Acquire::https::Timeout=30
  -o APT::Update::Error-Mode=any
)
sudo apt-get "${apt_options[@]}" update
sudo apt-get "${apt_options[@]}" install -y --no-install-recommends "$@"
