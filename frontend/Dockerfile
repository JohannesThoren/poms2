FROM node:22-slim AS builder
WORKDIR /build
COPY frontend/package*.json ./
RUN npm ci
COPY frontend/ ./
ARG DATABASE_URL
ENV DATABASE_URL=${DATABASE_URL}
RUN npm run build

FROM node:22-slim
WORKDIR /app
COPY --from=builder /build/.next ./.next
COPY --from=builder /build/public ./public
COPY --from=builder /build/package*.json ./
COPY --from=builder /build/node_modules ./node_modules
EXPOSE 3000
CMD ["npm", "start"]
