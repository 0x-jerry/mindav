FROM golang:1.20 AS builder
COPY go.mod /src/go.mod
COPY go.sum /src/go.sum

# ENV GOPROXY=https://mirrors.aliyun.com/goproxy/
# ENV CGO_ENABLED=0
WORKDIR /src/

RUN go mod download

COPY . /src/
RUN go build -o /src/mindav /src/main.go

FROM debian:stable-slim
# Copy our static executable.
COPY --from=builder /src/mindav /mindav/mindav

WORKDIR /mindav/

# Run the server binary.

ENV GIN_MODE=release

ENTRYPOINT ["/mindav/mindav"]

EXPOSE 8080
