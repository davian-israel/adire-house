# Rust service. Askama compiles templates into the binary; only /static is needed at runtime.
FROM rust:1-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY templates ./templates
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=build /src/target/release/adire-house /app/adire-house
COPY static ./static
ENV BIND=0.0.0.0:3000
EXPOSE 3000
USER nobody
CMD ["/app/adire-house"]
