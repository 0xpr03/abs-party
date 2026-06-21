# Stage 1: Build Vue frontend
FROM node:20-slim AS frontend-builder
WORKDIR /app
COPY client/package.json client/package-lock.json* ./client/
# Create the server/static dir so vite outDir exists
RUN mkdir -p server/static
RUN cd client && npm install
COPY client/ ./client/
# vite.config.js sets outDir: '../server/static' relative to client/
RUN cd client && npm run build

# Stage 2: Build Rust backend (needs ≥ 1.85 for edition 2024 in deps)
FROM rust:slim AS backend-builder
WORKDIR /app/server
# Cache dependencies separately
COPY server/Cargo.toml server/Cargo.lock* ./
RUN mkdir src \
    && echo 'fn main(){}' > src/main.rs \
    && echo '' > src/lib.rs \
    && cargo build --release \
    && rm -rf src
COPY server/src ./src
RUN find src -name "*.rs" -exec touch {} + && cargo build --release

# Stage 3: Runtime image
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=backend-builder /app/server/target/release/abs-party ./
COPY --from=frontend-builder /app/server/static/ ./static/

ENV STATIC_DIR=/app/static
EXPOSE 3456

CMD ["./abs-party"]
