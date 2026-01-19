# Build stage: compile the Go application
FROM golang:1.21-alpine AS builder

WORKDIR /build

# Copy go module files first for better caching
COPY go.mod ./
# Download dependencies (cached if go.mod hasn't changed)
RUN go mod download

# Copy source code
COPY *.go ./

# Build the application
# CGO_ENABLED=0 creates a statically linked binary
# -ldflags="-w -s" strips debug info to reduce size
RUN CGO_ENABLED=0 GOOS=linux go build -ldflags="-w -s" -o message-board

# Runtime stage: minimal image with just the binary
FROM alpine:latest

# Add ca-certificates for HTTPS requests
RUN apk --no-cache add ca-certificates

WORKDIR /app

# Copy the binary from builder stage
COPY --from=builder /build/message-board .

# Expose the default port
EXPOSE 8080

# Run the application
CMD ["./message-board"]
