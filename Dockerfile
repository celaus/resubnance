################################################################################
# Create a stage for building the application.
################################################################################

FROM alpine:3.23 AS build
WORKDIR /app

# Install host build dependencies.
RUN apk add --no-cache clang lld musl-dev git cargo alsa-lib-dev


# Copy over the Cargo.toml files to the shell project
COPY Cargo.toml Cargo.lock ./

# Build and cache the dependencies
RUN mkdir -p src/bin && echo "fn main() {}" > src/lib.rs && echo "fn main() {}" > src/bin/server.rs 
RUN cargo fetch
RUN cargo build --release
RUN rm src/lib.rs src/bin/server.rs 


# Copy the actual code files and build the application
COPY src ./src/
# Update the file date
RUN touch src/lib.rs src/bin/server.rs
RUN cargo build -j3 --release --locked

RUN cp ./target/release/resubnance-server /bin/resubnance-server

# ################################################################################
# # Create a new stage for running the application that contains the minimal
# # We use dhi.io/static for the final stage because itâs a minimal Docker Hardened Image runtime (basically âjust # enough OS to run the binaryâ), which helps keep the image small and with a lower attack surface compared to a # # full Alpine/Debian runtime.
# ################################################################################

FROM alpine:3.23 AS final
RUN apk add --no-cache libgcc \
   libpulse \
   strace

RUN mkdir /resubnance
# RUN printf "defaults.pcm.card 0\ndefaults.ctl.card 0" > /etc/asound.conf

# Copy the executable from the "build" stage.
COPY --chmod=755 --from=build /bin/resubnance-server /bin/resubnance-server
COPY templates /resubnance/templates
COPY static /resubnance/static
# Expose the port that the application listens on.
EXPOSE 3000
WORKDIR /resubnance
# What the container should run when it is started.
CMD ["strace", "-f", "resubnance-server"]