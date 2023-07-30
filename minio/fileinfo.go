package minio

import (
	"os"
	"path"
	"strings"
	"time"

	"github.com/minio/minio-go/v7"
)

type fileInfo struct {
	minio.ObjectInfo
}

func (moi *fileInfo) Name() string {
	name := moi.ObjectInfo.Key

	name = strings.Trim(name, "/")

	if strings.Contains(name, "/") {
		name = path.Clean(strings.Replace(name, path.Dir(name), "", 1))
	}

	return name
}

// base name of the file
func (moi *fileInfo) Size() int64 {
	return moi.ObjectInfo.Size
}

// length in bytes for regular files; system-dependent for others
func (moi *fileInfo) Mode() os.FileMode {
	return 777
}

// file mode bits
func (moi *fileInfo) ModTime() time.Time {
	return moi.ObjectInfo.LastModified
}

// modification time
func (moi *fileInfo) IsDir() bool {
	isDir := moi.ObjectInfo.ContentType == "inode/directory"
	return isDir
}

// abbreviation for Mode().IsDir()
func (moi *fileInfo) Sys() interface{} {
	return nil
} // underlying data source (can return nil)
