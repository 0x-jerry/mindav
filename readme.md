# mindav

MinIO + WebDAV bridge. Exposes a WebDAV server backed by MinIO/S3 storage.

Inspired by [totoval/mindav](https://github.com/totoval/mindav).

## Usage

### Docker

```yaml
version: '3'
services:
  mindav:
    image: ghcr.io/0xjerry/mindav:latest
    volumes:
      - ./config.json:/mindav/config.json
    ports:
      - '9000:8080'
```

### Example `config.json`

```json
{
  "app": {
    "port": "8080",
    "admin": {
      "username": "admin",
      "password": "password"
    },
    "uploadMode": "file"
  },
  "minio": {
    "endpoint": "web.server.com",
    "ssl": true,
    "bucketName": "webdav",
    "accessKey": "accessKey",
    "secretAccessKey": "secretAccessKey"
  }
}
```

## Development

```sh
cargo run
```

For hot-reload during development:

```sh
cargo install cargo-watch
cargo watch -x run
```
