#!/bin/sh
# pre_render_hook shim for the meeting templates.
#
# The template engine invokes hooks as `sh <script>` (see
# notesmith-templates::run_pre_render_hook), so the configured hook has to be
# shell. The logic lives in the sibling Python file, matching the connectors.
#
# `exec` keeps the render context flowing from our stdin to Python's, and lets
# the engine's timeout kill the real work rather than a wrapper.
#
# A missing python3 must not break note creation: emit the no-match shape the
# templates expect, so a meeting note still renders with the typed title.
set -u

# `${0%/*}` rather than `dirname`: parameter expansion is a shell builtin, so
# the shim needs nothing on PATH to find its own directory.
case "$0" in
    */*) dir=${0%/*} ;;
    *) dir=. ;;
esac

if ! command -v python3 >/dev/null 2>&1; then
    echo "meeting-prefill: python3 not on PATH; skipping calendar prefill" >&2
    echo '{"event_matched": false}'
    exit 0
fi

exec python3 "$dir/meeting-prefill.py"
