#!/bin/sh
# Container entrypoint for the declarant-portal nginx image.
#
# Renders the two templated configs (server block + included security
# header set) by substituting CSP_CONNECT_SRC, then hands off to
# nginx -g 'daemon off;'.
#
# Why envsubst on a sh entrypoint rather than nginx's built-in
# `/docker-entrypoint.d/20-envsubst-on-templates.sh`:
#   1. We render TWO files from templates (the server block + the
#      shared include) and want both materialised before nginx starts.
#   2. The shared include is referenced from inside the server config
#      via an absolute path, so it must be in /etc/nginx/ — not in
#      /etc/nginx/conf.d/ where the nginx auto-loader would pick it
#      up as a server block.
#   3. We want strict failure when env interpolation breaks, which the
#      official auto-loader does not provide.

set -eu

# ── Defaults ────────────────────────────────────────────────────────
#
# CSP_CONNECT_SRC defaults to empty (the templates always include
# 'self' explicitly, so an empty value leaves the directive as
# `connect-src 'self'`). Setting this to a deployment-specific origin
# is the supported way to extend the SPA's allowed XHR/fetch targets.
: "${CSP_CONNECT_SRC:=}"

# ── Validation ──────────────────────────────────────────────────────
#
# Refuse to start with a value that contains characters that would
# break out of the CSP directive (semicolons, quotes, newlines). This
# is a defense-in-depth check; the value comes from the deployment's
# environment so it should already be trusted, but a typo that
# silently broadens the policy is the exact class of bug we want to
# catch at startup.
#
# Use printf to materialise a literal newline into a variable, then
# test with `case` against the explicit set. ash's `case` glob doesn't
# support \n natively.
LF="$(printf '\nx')"
LF="${LF%x}"
case "${CSP_CONNECT_SRC}" in
    *\;*)
        echo "FATAL: CSP_CONNECT_SRC contains forbidden character ';': ${CSP_CONNECT_SRC}" >&2
        exit 1
        ;;
    *\'*)
        echo "FATAL: CSP_CONNECT_SRC contains forbidden character single-quote: ${CSP_CONNECT_SRC}" >&2
        exit 1
        ;;
    *\"*)
        echo "FATAL: CSP_CONNECT_SRC contains forbidden character double-quote: ${CSP_CONNECT_SRC}" >&2
        exit 1
        ;;
esac
case "${CSP_CONNECT_SRC}" in
    *"${LF}"*)
        echo "FATAL: CSP_CONNECT_SRC contains a newline" >&2
        exit 1
        ;;
esac

# ── Render ──────────────────────────────────────────────────────────
#
# envsubst expands only the named variables; any other ${...} tokens
# in the template are left untouched. Keep the variable list explicit
# so an env injection (e.g. PATH) can't accidentally land in the
# config.
TEMPLATE_DIR="/etc/nginx/templates"
CONF_DIR="/etc/nginx/conf.d"

envsubst '${CSP_CONNECT_SRC}' \
    < "${TEMPLATE_DIR}/nginx.conf.template" \
    > "${CONF_DIR}/default.conf"

envsubst '${CSP_CONNECT_SRC}' \
    < "${TEMPLATE_DIR}/security-headers.conf.template" \
    > "/etc/nginx/security-headers.conf"

# ── Validate ────────────────────────────────────────────────────────
#
# `nginx -t` would normally exit non-zero on a config error, but we
# also want to surface the FULL effective config for debugging when
# things go wrong. Print -T output when -t fails.
if ! nginx -t 2>/tmp/nginx-test.log; then
    echo "FATAL: nginx config validation failed after envsubst" >&2
    cat /tmp/nginx-test.log >&2
    echo "── effective config ──" >&2
    nginx -T 2>/dev/null || true
    exit 1
fi

echo "── declarant-portal: CSP connect-src origins → 'self' ${CSP_CONNECT_SRC}"
exec nginx -g 'daemon off;'
