# mindav

Inspired by [mindav]

minio + webdav

[mindav]: https://github.com/totoval/mindav

## Usage

Use docker compose:

```yaml
version: '3'
services:
  mindav:
    build:
      context: .
      dockerfile: Dockerfile
    volumes:
      - ./config.json:/mindav/config.json
    ports:
      - '9000:8080'
```

Example `./config.json`:

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
go install github.com/air-verse/air@latest
air
```