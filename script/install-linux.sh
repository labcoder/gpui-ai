#!/usr/bin/env bash
# Linux build dependencies for GPUI, adapted from gpui-component's
# script/install-linux.sh (tested there on Ubuntu 24.04).
set -euo pipefail

sudo apt-get update
sudo apt-get install -y \
  gcc g++ clang libfontconfig-dev libwayland-dev \
  libxkbcommon-x11-dev libx11-xcb-dev \
  libssl-dev libzstd-dev \
  vulkan-validationlayers libvulkan1
