#!/bin/bash

function LOG {
    echo "$(date -R)": "$0": "$*"
}

if [ -z "$DISPLAY" ]; then
    LOG "FATAL: No DISPLAY environment variable set.  No X."
    exit 13
fi

MAX=120 # About 120 seconds
CT=0
while ! xdpyinfo >/dev/null 2>&1; do
    sleep 1s
    CT=$(( CT + 1 ))
    if [ "$CT" -ge "$MAX" ]; then
        LOG "FATAL: $0: Gave up waiting for X server $DISPLAY"
        exit 11
    fi
done