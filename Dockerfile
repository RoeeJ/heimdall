# Use the official Elixir image as a builder
FROM elixir:1.17.0-alpine AS builder

ENV MIX_ENV=prod

# Install build dependencies
RUN apk add --no-cache build-base git

# Set the working directory
WORKDIR /app

# Install hex and rebar
RUN mix local.hex --force && \
    mix local.rebar --force

# Copy mix files
COPY mix.exs mix.lock ./

# Copy the config directory
COPY config config

# Install dependencies
RUN mix do deps.get, deps.compile

# Copy the rest of the application code
COPY . .

# Compile the application
RUN mix do compile, release

# Start a new build stage
FROM alpine:3.18

# Install runtime dependencies
RUN apk add --no-cache libstdc++ openssl ncurses-libs

# Set the working directory
WORKDIR /app

# Copy the release from the builder stage
COPY --from=builder /app/_build/prod/rel/heimdall ./

# Set the environment to production
ENV MIX_ENV=prod

# Expose the port the app runs on
EXPOSE 4000

# Set the entry point for the container
CMD ["bin/heimdall", "start"]