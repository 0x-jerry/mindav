package minio

import (
	"path"
	"strings"
)

func cleanPathName(name string) string {
	name = path.Clean(name)

	name = strings.TrimPrefix(name, "/")

	if name == "" {
		return "/"
	}

	return name
}
