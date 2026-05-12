#!/usr/bin/env bash
# Portal security-headers smoke (OPS-3).
#
# Builds the declarant-portal image, starts it with a known
# CSP_CONNECT_SRC, then asserts each required security header is
# present and carries the expected value. Exits non-zero on the first
# missing/wrong header.
#
# Invariants asserted (in order):
#   1.  Content-Security-Policy present
#   2.  CSP includes the templated CONNECT_SRC origin
#   3.  CSP refuses inline scripts (no 'unsafe-inline' in script-src)
#   4.  Strict-Transport-Security present + correct
#   5.  X-Content-Type-Options: nosniff
#   6.  X-Frame-Options: DENY
#   7.  Referrer-Policy: strict-origin-when-cross-origin
#   8.  Permissions-Policy denies geolocation/camera/microphone/payment
#   9.  Cross-Origin-Opener-Policy: same-origin
#   10. Cross-Origin-Resource-Policy: same-origin
#   11. Headers also present on /assets/ (location-block inheritance)
#   12. Headers also present on /healthz
#   13. SPA bundle still loads (curl GET / returns 200 with non-empty body
#       containing <div id="root"> from index.html)
#
# Cleanup on EXIT (success or failure) tears down the container and
# image tag.

set -euo pipefail
cd "$(dirname "$0")/.."

IMAGE_TAG="recor/declarant-portal:headers-smoke-$$"
CONTAINER_NAME="recor-portal-headers-smoke-$$"
HOST_PORT="18082"
TEST_API_ORIGIN="https://api.test.recor.cm"

TMPFILES=""
cleanup() {
    local exit_code=$?
    echo ""
    echo "── cleanup ──"
    docker rm -f "${CONTAINER_NAME}" >/dev/null 2>&1 || true
    docker rmi -f "${IMAGE_TAG}" >/dev/null 2>&1 || true
    # shellcheck disable=SC2086
    [ -n "${TMPFILES}" ] && rm -f ${TMPFILES}
    if [ "${exit_code}" -ne 0 ]; then
        echo "❌ FAIL (exit ${exit_code})"
    fi
    exit "${exit_code}"
}
trap cleanup EXIT INT TERM

echo "── building portal image (${IMAGE_TAG}) ──"
docker build --quiet -t "${IMAGE_TAG}" . 2>&1 | tail -3

echo ""
echo "── starting container on 127.0.0.1:${HOST_PORT} with CSP_CONNECT_SRC=${TEST_API_ORIGIN} ──"
docker run -d --rm \
    --name "${CONTAINER_NAME}" \
    -e "CSP_CONNECT_SRC=${TEST_API_ORIGIN}" \
    -p "127.0.0.1:${HOST_PORT}:8082" \
    "${IMAGE_TAG}" >/dev/null

echo "── waiting for nginx to accept connections ──"
for i in $(seq 1 30); do
    if curl -fsS "http://127.0.0.1:${HOST_PORT}/healthz" >/dev/null 2>&1; then
        echo "  ✅ healthy after ${i}s"
        break
    fi
    sleep 1
done

if ! curl -fsS "http://127.0.0.1:${HOST_PORT}/healthz" >/dev/null 2>&1; then
    echo "❌ nginx never came up; container logs:"
    docker logs "${CONTAINER_NAME}" 2>&1 | tail -40
    exit 1
fi

# Capture curl -I once; assert against the file repeatedly.
HEADERS_FILE=$(mktemp)
ASSETS_HEADERS=$(mktemp)
HEALTHZ_HEADERS=$(mktemp)
INDEX_BODY=$(mktemp)
INDEX_STATUS_FILE=$(mktemp)
TMPFILES="${HEADERS_FILE} ${ASSETS_HEADERS} ${HEALTHZ_HEADERS} ${INDEX_BODY} ${INDEX_STATUS_FILE}"

curl -sI "http://127.0.0.1:${HOST_PORT}/" > "${HEADERS_FILE}"
curl -s -o "${INDEX_BODY}" -w "%{http_code}" "http://127.0.0.1:${HOST_PORT}/" > "${INDEX_STATUS_FILE}"
INDEX_STATUS=$(cat "${INDEX_STATUS_FILE}")

echo ""
echo "── curl -I http://127.0.0.1:${HOST_PORT}/ ──"
cat "${HEADERS_FILE}"

assert_header() {
    local name="$1"
    local expected_substr="$2"
    local file="${3:-${HEADERS_FILE}}"
    local label="${4:-/}"
    local line
    line=$(grep -i "^${name}:" "${file}" || true)
    if [ -z "${line}" ]; then
        echo "❌ FAIL: header '${name}' missing on ${label}"
        echo "── full headers ──"
        cat "${file}"
        exit 1
    fi
    if ! echo "${line}" | grep -qF "${expected_substr}"; then
        echo "❌ FAIL: header '${name}' on ${label} did not contain expected substring"
        echo "  expected substring: ${expected_substr}"
        echo "  actual line:        ${line}"
        exit 1
    fi
    echo "  ✅ ${name} on ${label}"
}

assert_not_in_header() {
    local name="$1"
    local forbidden_substr="$2"
    local file="${3:-${HEADERS_FILE}}"
    local label="${4:-/}"
    local line
    line=$(grep -i "^${name}:" "${file}" || true)
    if [ -z "${line}" ]; then
        echo "❌ FAIL: header '${name}' missing on ${label} (assert_not_in_header pre-check)"
        exit 1
    fi
    if echo "${line}" | grep -qF "${forbidden_substr}"; then
        echo "❌ FAIL: header '${name}' on ${label} unexpectedly contained '${forbidden_substr}'"
        echo "  actual line: ${line}"
        exit 1
    fi
    echo "  ✅ ${name} on ${label} does not contain '${forbidden_substr}'"
}

echo ""
echo "── asserting required security headers on / ──"
assert_header "Content-Security-Policy" "default-src 'self'"
assert_header "Content-Security-Policy" "frame-ancestors 'none'"
assert_header "Content-Security-Policy" "base-uri 'self'"
assert_header "Content-Security-Policy" "form-action 'self'"
assert_header "Content-Security-Policy" "object-src 'none'"
assert_header "Content-Security-Policy" "${TEST_API_ORIGIN}"
# script-src must NOT contain unsafe-inline / unsafe-eval.
csp_line=$(grep -i '^content-security-policy:' "${HEADERS_FILE}" || true)
script_src_segment=$(echo "${csp_line}" | grep -oE "script-src [^;]+" || true)
if echo "${script_src_segment}" | grep -qE "unsafe-inline|unsafe-eval"; then
    echo "❌ FAIL: script-src must not contain 'unsafe-inline' or 'unsafe-eval'"
    echo "  actual: ${script_src_segment}"
    exit 1
fi
echo "  ✅ script-src is strict (no unsafe-inline/eval)"

assert_header "Strict-Transport-Security" "max-age=63072000"
assert_header "Strict-Transport-Security" "includeSubDomains"
assert_header "Strict-Transport-Security" "preload"
assert_header "X-Content-Type-Options" "nosniff"
assert_header "X-Frame-Options" "DENY"
assert_header "Referrer-Policy" "strict-origin-when-cross-origin"
assert_header "Permissions-Policy" "geolocation=()"
assert_header "Permissions-Policy" "camera=()"
assert_header "Permissions-Policy" "microphone=()"
assert_header "Permissions-Policy" "payment=()"
assert_header "Permissions-Policy" "usb=()"
assert_header "Cross-Origin-Opener-Policy" "same-origin"
assert_header "Cross-Origin-Resource-Policy" "same-origin"

# server_tokens off — Server header must not carry a version.
server_line=$(grep -i '^server:' "${HEADERS_FILE}" || true)
if echo "${server_line}" | grep -qE 'nginx/[0-9]'; then
    echo "❌ FAIL: Server header leaks nginx version: ${server_line}"
    exit 1
fi
echo "  ✅ Server header does not leak version"

echo ""
echo "── asserting headers on /healthz ──"
curl -sI "http://127.0.0.1:${HOST_PORT}/healthz" > "${HEALTHZ_HEADERS}"
assert_header "Content-Security-Policy" "default-src 'self'" "${HEALTHZ_HEADERS}" "/healthz"
assert_header "X-Frame-Options" "DENY" "${HEALTHZ_HEADERS}" "/healthz"
assert_header "X-Content-Type-Options" "nosniff" "${HEALTHZ_HEADERS}" "/healthz"

echo ""
echo "── asserting headers on /assets/ (location-block inheritance) ──"
# Use a non-existent asset; nginx returns 404 but with headers still
# applied. add_header `always` ensures the headers attach to error
# responses too.
curl -sI "http://127.0.0.1:${HOST_PORT}/assets/__not-a-real-asset__.js" > "${ASSETS_HEADERS}"
assert_header "Content-Security-Policy" "default-src 'self'" "${ASSETS_HEADERS}" "/assets/"
assert_header "Cache-Control" "immutable" "${ASSETS_HEADERS}" "/assets/"
assert_header "X-Frame-Options" "DENY" "${ASSETS_HEADERS}" "/assets/"

echo ""
echo "── asserting SPA bundle still loads ──"
if [ "${INDEX_STATUS}" != "200" ]; then
    echo "❌ FAIL: GET / returned ${INDEX_STATUS}, expected 200"
    exit 1
fi
if ! grep -q '<div id="root">' "${INDEX_BODY}"; then
    echo "❌ FAIL: index.html missing the React mount-point"
    head -40 "${INDEX_BODY}"
    exit 1
fi
echo "  ✅ GET / → 200 and carries the React mount point"

# The built JS bundle file path comes from the vite manifest; just
# verify SOME file under /assets/ exists.
asset_path=$(grep -oE '/assets/[A-Za-z0-9._-]+\.js' "${INDEX_BODY}" | head -1)
if [ -n "${asset_path}" ]; then
    asset_status=$(curl -s -o /dev/null -w "%{http_code}" "http://127.0.0.1:${HOST_PORT}${asset_path}")
    if [ "${asset_status}" != "200" ]; then
        echo "❌ FAIL: bundle ${asset_path} returned ${asset_status}"
        exit 1
    fi
    echo "  ✅ bundle ${asset_path} → 200"
fi

echo ""
echo "✅ OPS-3 HEADERS SMOKE: PASS"
echo "   • 10+ security headers present on / (server-level set)"
echo "   • script-src is strict (no unsafe-inline/eval)"
echo "   • CSP includes the templated connect-src origin (${TEST_API_ORIGIN})"
echo "   • Headers inherit into /assets/ and /healthz location blocks"
echo "   • SPA bundle (index.html + /assets/*.js) loads under the locked-down CSP"
echo "   • Server header does not leak the nginx version"
