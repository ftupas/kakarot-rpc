# Define ARG for build platform
FROM --platform=$BUILDPLATFORM rust:1.64 as builder

# Set ARG for build platform
ARG BUILDPLATFORM

# Set working directory
WORKDIR /usr/src/rpc

# Copy source code
COPY . .

# Add targets and install compilers based on build platform
# Then build the application for the target platform
RUN rustup self update && \
    case "$BUILDPLATFORM" in \
        "linux/amd64") \
            rustup target add x86_64-unknown-linux-gnu; \
            apt-get update && apt-get -y install gcc-x86-64-linux-gnu; \
            cargo build --all --release \
              --target=x86_64-unknown-linux-gnu \
              --config target.x86_64-unknown-linux-gnu.linker=\"x86_64-linux-gnu-gcc\"; \
            cp /usr/src/rpc/target/x86_64-unknown-linux-gnu/release/kakarot-rpc /usr/src/rpc/target/release/; \
            ;; \
        "linux/arm64") \
            rustup target add aarch64-unknown-linux-gnu; \
            apt-get update && apt-get -y install gcc-aarch64-linux-gnu; \
            cargo build --all --release \
              --target=aarch64-unknown-linux-gnu \
              --config target.aarch64-unknown-linux-gnu.linker=\"aarch64-linux-gnu-gcc\"; \
            cp /usr/src/rpc/target/aarch64-unknown-linux-gnu/release/kakarot-rpc /usr/src/rpc/target/release/; \
            ;; \
        *) \
            echo "Unknown BUILDPLATFORM: $BUILDPLATFORM"; \
            exit 1; \
            ;; \
    esac

# Create a new container from scratch to reduce image size
FROM debian:bullseye

# Install any necessary dependencies
RUN apt-get update && apt-get install -y libssl-dev ca-certificates tini curl && rm -rf /var/lib/apt/lists/*

# Set the working directory
WORKDIR /usr/src/app

# Copy the built binary from the previous container
COPY --from=builder /usr/src/rpc/target/release/kakarot-rpc /usr/local/bin

# Expose the port that the RPC server will run on
EXPOSE 9545
EXPOSE 3030

# this is required to have exposing ports work from docker, the default is not this.
ENV KAKAROT_HTTP_RPC_ADDRESS="0.0.0.0:9545"

# Add a health check to make sure the service is healthy
HEALTHCHECK --interval=3s --timeout=5s --start-period=1s --retries=5 \
  CMD curl --request POST \
    --header "Content-Type: application/json" \
    --data '{"jsonrpc": "2.0", "method": "eth_chainId", "id": 1}' http://${KAKAROT_HTTP_RPC_ADDRESS} || exit 1

# Seen in https://github.com/eqlabs/pathfinder/blob/4ab915a830953ed6f02af907937b46cb447d9a92/Dockerfile#L120 - 
# Allows for passing args down to the underlying binary easily
ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/kakarot-rpc"]

# empty CMD is needed and cannot be --help because otherwise configuring from
# environment variables only would be impossible and require a workaround.
CMD []
