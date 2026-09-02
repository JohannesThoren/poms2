FROM node:22-slim AS builder
WORKDIR /build
# libpam0g-dev + build tools are needed to compile authenticate-pam's
# native addon during npm ci.
RUN apt-get update && apt-get install -y python3 make g++ libpam0g-dev && rm -rf /var/lib/apt/lists/*
COPY frontend/package*.json ./
RUN npm ci
COPY frontend/ ./
ARG DATABASE_URL
ENV DATABASE_URL=${DATABASE_URL}
RUN npm run build

FROM node:22-slim
WORKDIR /app
# Runtime PAM libraries/modules (pam_unix.so etc.) - needed for the
# authenticate-pam addon to actually authenticate against /etc/passwd +
# /etc/shadow, which docker-compose.yml mounts in read-only from the host.
RUN apt-get update && apt-get install -y libpam-modules libpam0g libpam-runtime && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/.next ./.next
COPY --from=builder /build/public ./public
COPY --from=builder /build/package*.json ./
COPY --from=builder /build/node_modules ./node_modules
EXPOSE 3000
# Deliberately runs as root (the node:22-slim default), not a stripped-down
# "node" user: pam_unix needs to read the host's mounted-in /etc/shadow,
# which is only root-readable. This is the same tradeoff as the
# docker-manager project's host-user auth.
CMD ["npm", "start"]
