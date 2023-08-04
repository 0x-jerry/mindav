# mindav

Inspired by [mindav]

minio + webdav

[mindav]: https://github.com/totoval/mindav


## Usage

Use docker compose:

```
version: "3"
services:

  mindav:
    build:
      context: .
      dockerfile: Dockerfile
    volumes:
      - ./config.json:/mindav/config.json
    ports:
      - "9000:8080"
```
