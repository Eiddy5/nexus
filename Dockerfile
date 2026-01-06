# ====== 构建阶段 ======
FROM harbor.jinqidongli.com/library/rust:1.88.0 AS builder

WORKDIR /app
COPY . .

RUN cargo build --release

# ====== 运行阶段 ======
FROM harbor.jinqidongli.com/library/debian:bookworm-slim

WORKDIR /app

EXPOSE 1234

COPY --from=builder /app/target/release/nexus /app/nexus

COPY .env /app/.env

CMD ["./nexus"]