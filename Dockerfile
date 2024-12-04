FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y libssl3 && rm -rf /var/lib/apt/lists/*
COPY target/release/heimdall /usr/local/bin/
ENTRYPOINT ["/usr/local/bin/heimdall"]