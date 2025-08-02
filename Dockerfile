# Build stage using Nix
FROM nixos/nix:latest AS builder

# Enable flakes
RUN echo "experimental-features = nix-command flakes" >> /etc/nix/nix.conf

# Copy the entire project
WORKDIR /build
COPY . .

# Build the project using the flake
RUN nix build .#default --no-link --print-out-paths > /tmp/build-path

# Final stage - minimal runtime image
FROM gcr.io/distroless/cc-debian12:nonroot

# Copy the built binary from Nix store
COPY --from=builder /nix/store/*-checkle-*/bin/checkle /usr/local/bin/checkle

# Set the entrypoint
ENTRYPOINT ["/usr/local/bin/checkle"]