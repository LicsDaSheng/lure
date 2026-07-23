# 多阶段构建：编译 release 二进制，再拷入精简运行镜像。
# 说明：Phase 11 提供发布产物结构；真实镜像构建/推送属外部工具，按需在 CI 执行。

FROM rust:1-slim AS builder
WORKDIR /app

# 先拷贝清单以利用依赖缓存层。
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

RUN cargo build --release --bin lure

FROM debian:stable-slim AS runtime
WORKDIR /app

# 非 root 运行。
RUN useradd --create-home --uid 10001 lure
USER lure

COPY --from=builder /app/target/release/lure /usr/local/bin/lure

# 默认 workspace 挂载点。
ENV LURE_WORKSPACE=/home/lure/.nanobot/workspace
VOLUME ["/home/lure/.nanobot"]

ENTRYPOINT ["lure"]
CMD ["--version"]
