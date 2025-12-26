# ====== 构建阶段 ======
FROM harbor.jinqidongli.com/library/rust:1.88.0 AS builder

WORKDIR /app
COPY . .

RUN cargo build --release

# ====== 运行阶段 ======
FROM harbor.jinqidongli.com/library/debian:bookworm-slim

WORKDIR /app

COPY --from=builder /app/target/release/y-server /app/server

COPY .env /app/.env

ENV RUST_LOG=info

CMD ["./server"]