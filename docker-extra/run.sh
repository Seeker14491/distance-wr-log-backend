#!/bin/bash
set -e

function LOG {
    echo "$(date -R)": "$0": "$*"
}

LOG "Starting dummy X Server"
X "$DISPLAY" -config dummy-1920x1080.conf &>/dev/null &

LOG "Waiting for X Server $DISPLAY to be available"
./wait-x11.sh

LOG "Starting distance-wr-log-manager"
./distance-wr-log-manager