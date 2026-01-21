#!/bin/sh
# wait-for-it.sh - Wait for a service to be available
# Usage: wait-for-it.sh host:port [-t timeout] [-- command args]

set -e

TIMEOUT=15
QUIET=0
HOST=""
PORT=""
CMD=""

usage() {
    cat << USAGE >&2
Usage:
    wait-for-it.sh host:port [-t timeout] [-q] [-- command args]

    -h HOST | --host=HOST       Host to wait for
    -p PORT | --port=PORT       TCP port to wait for
    -t TIMEOUT | --timeout=TIMEOUT
                                Timeout in seconds, 0 for no timeout (default: $TIMEOUT)
    -q | --quiet                Don't output any status messages
    -- COMMAND ARGS             Execute command with args after the service is available
USAGE
    exit 1
}

wait_for() {
    if [ "$QUIET" -ne 1 ]; then
        echo "Waiting for $HOST:$PORT..."
    fi

    start_ts=$(date +%s)
    while :
    do
        if nc -z "$HOST" "$PORT" > /dev/null 2>&1; then
            end_ts=$(date +%s)
            if [ "$QUIET" -ne 1 ]; then
                echo "$HOST:$PORT is available after $((end_ts - start_ts)) seconds"
            fi
            break
        fi

        if [ "$TIMEOUT" -gt 0 ]; then
            end_ts=$(date +%s)
            if [ $((end_ts - start_ts)) -ge "$TIMEOUT" ]; then
                echo "Timeout occurred after waiting $TIMEOUT seconds for $HOST:$PORT" >&2
                exit 1
            fi
        fi

        sleep 1
    done
}

wait_for_wrapper() {
    # Support for multiple services
    if [ -z "$HOST" ] || [ -z "$PORT" ]; then
        # Try to parse from HOST_PORT format
        if [ -n "$1" ]; then
            HOST=$(echo "$1" | cut -d: -f1)
            PORT=$(echo "$1" | cut -d: -f2)
        else
            usage
        fi
    fi

    wait_for

    # Execute command if provided
    if [ -n "$CMD" ]; then
        if [ "$QUIET" -ne 1 ]; then
            echo "Executing: $CMD"
        fi
        exec $CMD
    fi
}

# Parse arguments
while [ $# -gt 0 ]
do
    case "$1" in
        *:* )
            HOST=$(echo "$1" | cut -d: -f1)
            PORT=$(echo "$1" | cut -d: -f2)
            shift 1
            ;;
        -h)
            HOST="$2"
            if [ "$HOST" = "" ]; then usage; fi
            shift 2
            ;;
        --host=*)
            HOST="${1#*=}"
            shift 1
            ;;
        -p)
            PORT="$2"
            if [ "$PORT" = "" ]; then usage; fi
            shift 2
            ;;
        --port=*)
            PORT="${1#*=}"
            shift 1
            ;;
        -t)
            TIMEOUT="$2"
            if [ "$TIMEOUT" = "" ]; then usage; fi
            shift 2
            ;;
        --timeout=*)
            TIMEOUT="${1#*=}"
            shift 1
            ;;
        -q | --quiet)
            QUIET=1
            shift 1
            ;;
        --)
            shift
            CMD="$*"
            break
            ;;
        --help)
            usage
            ;;
        *)
            echo "Unknown argument: $1" >&2
            usage
            ;;
    esac
done

wait_for_wrapper
