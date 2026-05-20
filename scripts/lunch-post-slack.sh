#!/usr/bin/env sh
set -eu

if [ -z "${SLACK_WEBHOOK_URL:-}" ]; then
    echo "SLACK_WEBHOOK_URL is required" >&2
    exit 2
fi

/opt/lunch/lunch slack \
    | curl -fsS -X POST \
        -H 'Content-type: application/json' \
        --data-binary @- \
        "$SLACK_WEBHOOK_URL"
