#!/usr/bin/env bash
# tools/pr-merge-when-green.sh <pr> <branch> — poll a Gitea PR's gate once a minute (up to 40 min), re-queue a run the concurrency group cancelled, merge with tea on success + mergeable, and confirm through `git fetch origin master`. Launch detached (setsid nohup … > log # merge-when-green.sh <pr> <branch> — poll the gate, merge once success, confirm via fetch.) and read the log's last line: MERGED / REAL FAILURE / TIMEOUT.
PR=$1; BR=$2; TOKEN=$(grep -oP 'token:\s*\K\S+' ~/.config/tea/config.yml | head -1)
API=https://git.neet.dev/api/v1/repos/zuckerberg/luxel; TEA=$(ls -d /nix/store/*-tea-*/bin/tea 2>/dev/null | tail -1)
cd "$(git rev-parse --show-toplevel)"
for i in $(seq 1 40); do
  git fetch -q origin "$BR" 2>/dev/null; SHA=$(git rev-parse "origin/$BR")
  ST=$(curl -s -H "Authorization: token $TOKEN" "$API/commits/$SHA/status" | grep -o '"state":"[a-z]*"' | head -1 | cut -d'"' -f4)
  M=$(curl -s -H "Authorization: token $TOKEN" "$API/pulls/$PR" | grep -o '"mergeable":[a-z]*' | head -1)
  echo "$(date -u +%T) PR#$PR $SHA state=$ST $M"
  if [ "$ST" = success ] && [ "$M" = '"mergeable":true' ]; then
    $TEA pr merge --repo zuckerberg/luxel "$PR" 2>&1 | tail -1; sleep 4; git fetch -q origin master
    if git merge-base --is-ancestor "$SHA" origin/master; then echo "MERGED PR#$PR -> $(git rev-parse --short origin/master)"; exit 0; else echo "merge did not land yet"; fi
  elif [ "$ST" = failure ]; then
    RUN=$(curl -s -H "Authorization: token $TOKEN" "$API/commits/$SHA/status" | grep -o 'actions/runs/[0-9]*' | head -1 | cut -d/ -f3)
    CONC=$(curl -s -H "Authorization: token $TOKEN" "$API/actions/runs/$RUN/jobs" | grep -o '"conclusion":"[a-z]*"' | head -1)
    echo "  failure: run $RUN $CONC"; if echo "$CONC" | grep -q cancelled; then curl -s -X POST -H "Authorization: token $TOKEN" "$API/actions/runs/$RUN/rerun" >/dev/null; echo "  re-queued"; else echo "REAL FAILURE PR#$PR"; exit 1; fi
  fi
  sleep 60
done; echo "TIMEOUT PR#$PR"; exit 2
