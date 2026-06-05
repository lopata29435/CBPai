# syntax=docker/dockerfile:1

# --- 1) сборка фронтенда (Vite) ---
FROM node:20-alpine AS web
WORKDIR /web
COPY web/package.json web/package-lock.json* ./
RUN npm install
COPY web/ ./
RUN npm run build

# --- 2) сборка бэкенда (Rust) ---
FROM rust:1-slim AS build
WORKDIR /app
# Сначала только манифесты — кэшируем слой зависимостей.
COPY Cargo.toml Cargo.lock* ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && cargo build --release && rm -rf src
COPY src ./src
# Тронуть main.rs, чтобы пересобрался именно наш код, а не закэшированная заглушка.
RUN touch src/main.rs && cargo build --release

# --- 3) runtime ---
FROM debian:bookworm-slim AS runtime
WORKDIR /app
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /app/target/release/kitchen-app /app/kitchen-app
COPY --from=web /web/dist /app/static
ENV PORT=3000 STATIC_DIR=/app/static
EXPOSE 3000
CMD ["/app/kitchen-app"]
