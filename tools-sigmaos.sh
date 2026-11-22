#!/bin/sh
# SigmaOS component interconnect helper.
# Usage: ./tools-sigmaos.sh <command>
# Commands: info, test, build, sync
set -eu
cmd="${1:-info}"
case "$cmd" in
  info)  cargo metadata --no-deps --format-version 1 | head -c 2000; echo ;;
  test)  cargo test ;;
  build) cargo build --release ;;
  sync)  git pull --rebase origin main && git push origin main ;;
  *) echo "usage: $0 [info|test|build|sync]" >&2; exit 2 ;;
esac
