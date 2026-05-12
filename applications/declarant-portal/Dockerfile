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

FROM nginx:1.27-alpine AS runtime
COPY --from=builder /build/dist /usr/share/nginx/html
COPY nginx.conf /etc/nginx/conf.d/default.conf
EXPOSE 8082
HEALTHCHECK --interval=10s --timeout=3s --start-period=5s --retries=3 \
  CMD wget -q -O- http://localhost:8082/healthz || exit 1
