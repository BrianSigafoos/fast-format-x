#!/usr/bin/env bash

set -euo pipefail

REPO_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
SOURCE="$REPO_ROOT/install.sh"
PAGES_COPY="$REPO_ROOT/docs/install.sh"

case "${1:-}" in
    "")
        cp "$SOURCE" "$PAGES_COPY"
        chmod +x "$PAGES_COPY"
        ;;
    --check)
        if ! cmp -s "$SOURCE" "$PAGES_COPY"; then
            echo "error: docs/install.sh is out of sync with install.sh" >&2
            echo "Run scripts/sync-install.sh after editing install.sh." >&2
            diff -u "$SOURCE" "$PAGES_COPY" || true
            exit 1
        fi
        ;;
    *)
        echo "Usage: scripts/sync-install.sh [--check]" >&2
        exit 2
        ;;
esac
