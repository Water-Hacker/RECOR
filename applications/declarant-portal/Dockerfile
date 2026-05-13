# Multi-stage build for recor-declarant-portal.

FROM node:22-bookworm AS builder
WORKDIR /build
# Install pnpm globally (matches V3 P12 pin convention).
RUN corepack enable && corepack prepare pnpm@9.12.3 --activate
COPY package.json ./
# We don't ship a lockfile in v1 (pnpm install resolves fresh each time);
# follow-up adds the lockfile + sets --frozen-lockfile.
RUN pnpm install --no-frozen-lockfile
COPY tsconfig.json vite.config.ts index.html ./
COPY src ./src
COPY tests ./tests
RUN pnpm build

FROM nginx:1.29-alpine AS runtime
# Bump from nginx:1.27-alpine (Alpine 3.21.3) → 1.29-alpine (Alpine
# 3.22) — the older base picked up HIGH/CRITICAL CVEs that Trivy
# fail-closes the publish-images workflow on (CI-2 #70). The 1.29
# line tracks newer Alpine patches.
# envsubst lives in the gettext package on alpine.
RUN apk add --no-cache gettext

COPY --from=builder /build/dist /usr/share/nginx/html

# Templates are rendered at container startup by docker-entrypoint.sh
# so the CSP can carry the deployment's API origin without rebuilding
# the image. See applications/declarant-portal/CLAUDE.md § security
# headers for the policy rationale.
COPY nginx.conf.template /etc/nginx/templates/nginx.conf.template
COPY security-headers.conf.template /etc/nginx/templates/security-headers.conf.template
COPY docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
RUN chmod +x /usr/local/bin/docker-entrypoint.sh

# Remove the default nginx server block; our entrypoint writes the
# real one to /etc/nginx/conf.d/default.conf from the template.
RUN rm -f /etc/nginx/conf.d/default.conf

EXPOSE 8082
HEALTHCHECK --interval=10s --timeout=3s --start-period=5s --retries=3 \
  CMD wget -q -O- http://localhost:8082/healthz || exit 1

ENTRYPOINT ["/usr/local/bin/docker-entrypoint.sh"]
