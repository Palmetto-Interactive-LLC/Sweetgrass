#!/usr/bin/env bash
set -euo pipefail

for port in 3007 3008; do
  pids="$(lsof -ti "tcp:${port}" || true)"
  if [ -n "$pids" ]; then
    kill $pids
  fi
done

npm run build
npm run dev:full
