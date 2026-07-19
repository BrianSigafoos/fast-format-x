#!/usr/bin/env bash

set -euo pipefail

REPO_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
INSTALLER="$REPO_ROOT/install.sh"
PASSED=0

fail() {
    echo "FAIL: $1" >&2
    return 1
}

assert_contains() {
    local haystack="$1"
    local needle="$2"

    [[ "$haystack" == *"$needle"* ]] || fail "expected output to contain: $needle"
}

assert_file_contains() {
    local file="$1"
    local needle="$2"

    grep -Fq "$needle" "$file" || fail "expected $file to contain: $needle"
}

fixture_sha256() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    else
        shasum -a 256 "$1" | awk '{print $1}'
    fi
}

setup_fixture() {
    local target="$1"

    TEST_DIR=$(mktemp -d)
    export TEST_DIR
    trap 'rm -rf "$TEST_DIR"' EXIT

    mkdir -p "$TEST_DIR/bin" "$TEST_DIR/home/.local/bin" "$TEST_DIR/package"
    CURL_LOG="$TEST_DIR/curl.log"
    FIXTURE_ARCHIVE="$TEST_DIR/archive.tar.gz"
    FIXTURE_SUMS="$TEST_DIR/SHA256SUMS.txt"
    ASSET_NAME="ffx-${target}.tar.gz"
    export CURL_LOG FIXTURE_ARCHIVE FIXTURE_SUMS ASSET_NAME

    printf '#!/usr/bin/env bash\necho "ffx fixture"\n' > "$TEST_DIR/package/ffx"
    chmod +x "$TEST_DIR/package/ffx"
    tar -czf "$FIXTURE_ARCHIVE" -C "$TEST_DIR/package" ffx
    printf '%s  %s\n' "$(fixture_sha256 "$FIXTURE_ARCHIVE")" "$ASSET_NAME" > "$FIXTURE_SUMS"

    cat > "$TEST_DIR/bin/uname" <<'EOF'
#!/usr/bin/env bash
case "$1" in
    -s) printf '%s\n' "$FAKE_UNAME_S" ;;
    -m) printf '%s\n' "$FAKE_UNAME_M" ;;
    *) exit 1 ;;
esac
EOF

    cat > "$TEST_DIR/bin/curl" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

output=""
headers=""
url=""

while [ "$#" -gt 0 ]; do
    case "$1" in
        -o|-D|--proto)
            if [ "$1" = "-o" ]; then
                output="$2"
            elif [ "$1" = "-D" ]; then
                headers="$2"
            fi
            shift 2
            ;;
        http*)
            url="$1"
            shift
            ;;
        *)
            shift
            ;;
    esac
done

printf '%s\n' "$url" >> "$CURL_LOG"

if [ -n "${CURL_FAIL_PATTERN:-}" ] && [[ "$url" == *"$CURL_FAIL_PATTERN"* ]]; then
    exit 22
fi

case "$url" in
    */releases/latest/download/SHA256SUMS.txt)
        cp "$FIXTURE_SUMS" "$output"
        printf 'HTTP/2 302\r\nlocation: https://github.com/BrianSigafoos/fast-format-x/releases/download/%s/SHA256SUMS.txt\r\n\r\n' "$LATEST_VERSION" > "$headers"
        ;;
    */releases/download/*/SHA256SUMS.txt)
        cp "$FIXTURE_SUMS" "$output"
        ;;
    */releases/download/*/ffx-*.tar.gz)
        cp "$FIXTURE_ARCHIVE" "$output"
        ;;
    *)
        echo "unexpected URL: $url" >&2
        exit 22
        ;;
esac
EOF

    chmod +x "$TEST_DIR/bin/uname" "$TEST_DIR/bin/curl"
    export PATH="$TEST_DIR/bin:$PATH"
    export HOME="$TEST_DIR/home"
    export LATEST_VERSION="v9.9.9"
    unset FFX_VERSION CURL_FAIL_PATTERN
}

run_installer() {
    set +e
    INSTALL_OUTPUT=$(bash "$INSTALLER" 2>&1)
    INSTALL_STATUS=$?
    set -e
}

test_default_latest_install() {
    setup_fixture "x86_64-unknown-linux-gnu"
    export FAKE_UNAME_S="Linux" FAKE_UNAME_M="x86_64"

    run_installer

    [ "$INSTALL_STATUS" -eq 0 ] || fail "latest install failed: $INSTALL_OUTPUT"
    [ -x "$HOME/.local/bin/ffx" ] || fail "latest install did not create an executable"
    assert_contains "$INSTALL_OUTPUT" "Installing ffx v9.9.9"
    assert_contains "$INSTALL_OUTPUT" "Verified SHA-256 checksum"
    assert_file_contains "$CURL_LOG" "https://github.com/BrianSigafoos/fast-format-x/releases/latest/download/SHA256SUMS.txt"
    assert_file_contains "$CURL_LOG" "https://github.com/BrianSigafoos/fast-format-x/releases/download/v9.9.9/ffx-x86_64-unknown-linux-gnu.tar.gz"
    if grep -Fq "api.github.com" "$CURL_LOG"; then
        fail "latest install called the GitHub REST API"
    fi
}

test_explicit_version_install() {
    setup_fixture "aarch64-apple-darwin"
    export FAKE_UNAME_S="Darwin" FAKE_UNAME_M="arm64"
    export FFX_VERSION="v1.2.3"

    run_installer

    [ "$INSTALL_STATUS" -eq 0 ] || fail "pinned install failed: $INSTALL_OUTPUT"
    assert_contains "$INSTALL_OUTPUT" "Installing ffx v1.2.3"
    assert_file_contains "$CURL_LOG" "https://github.com/BrianSigafoos/fast-format-x/releases/download/v1.2.3/SHA256SUMS.txt"
    assert_file_contains "$CURL_LOG" "https://github.com/BrianSigafoos/fast-format-x/releases/download/v1.2.3/ffx-aarch64-apple-darwin.tar.gz"
    if grep -Fq "/releases/latest/" "$CURL_LOG"; then
        fail "pinned install used the latest-release URL"
    fi
}

test_archive_download_failure() {
    setup_fixture "x86_64-unknown-linux-gnu"
    export FAKE_UNAME_S="Linux" FAKE_UNAME_M="x86_64"
    export CURL_FAIL_PATTERN=".tar.gz"

    run_installer

    [ "$INSTALL_STATUS" -ne 0 ] || fail "installer succeeded after archive download failure"
    assert_contains "$INSTALL_OUTPUT" "Failed to download ffx v9.9.9 for x86_64-unknown-linux-gnu"
    [ ! -e "$HOME/.local/bin/ffx" ] || fail "failed download installed a binary"
}

test_checksum_mismatch() {
    setup_fixture "x86_64-unknown-linux-gnu"
    export FAKE_UNAME_S="Linux" FAKE_UNAME_M="x86_64"
    printf '%064d  %s\n' 0 "$ASSET_NAME" > "$FIXTURE_SUMS"

    run_installer

    [ "$INSTALL_STATUS" -ne 0 ] || fail "installer accepted an invalid checksum"
    assert_contains "$INSTALL_OUTPUT" "Checksum verification failed for $ASSET_NAME"
    [ ! -e "$HOME/.local/bin/ffx" ] || fail "checksum failure installed a binary"
}

test_missing_checksum() {
    setup_fixture "x86_64-unknown-linux-gnu"
    export FAKE_UNAME_S="Linux" FAKE_UNAME_M="x86_64"
    printf '%064d  other-asset.tar.gz\n' 0 > "$FIXTURE_SUMS"

    run_installer

    [ "$INSTALL_STATUS" -ne 0 ] || fail "installer accepted a missing checksum entry"
    assert_contains "$INSTALL_OUTPUT" "No checksum found for $ASSET_NAME"
    [ ! -e "$HOME/.local/bin/ffx" ] || fail "missing checksum installed a binary"
}

run_test() {
    local name="$1"

    if ("$name"); then
        echo "ok - $name"
        PASSED=$((PASSED + 1))
    else
        echo "not ok - $name" >&2
        exit 1
    fi
}

run_test test_default_latest_install
run_test test_explicit_version_install
run_test test_archive_download_failure
run_test test_checksum_mismatch
run_test test_missing_checksum

echo "$PASSED installer tests passed"
