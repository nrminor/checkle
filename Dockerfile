# Build stage using Nix
FROM nixos/nix:latest AS builder

# Enable flakes
RUN echo "experimental-features = nix-command flakes" >> /etc/nix/nix.conf

# Copy the entire project
WORKDIR /build
COPY . .

# Build the project using the flake and copy to a known location
RUN nix build .#default --no-link --print-out-paths > /tmp/build-path && \
    cp "$(cat /tmp/build-path)/bin/checkle" /tmp/checkle

# Final stage - minimal runtime image
FROM gcr.io/distroless/cc-debian12:nonroot

# Copy the built binary from the builder stage
COPY --from=builder /tmp/checkle /usr/local/bin/checkle

# Set the entrypoint
ENTRYPOINT ["/usr/local/bin/checkle"]