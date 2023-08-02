package minio

import (
	"context"
	"fmt"
	"io"
	"log"
	"os"
	"path"
	"strings"

	"github.com/minio/minio-go/v7"
)

type file struct {
	*minio.Object

	fs   *MinioFS
	name string
}

func (mo *file) Stat() (os.FileInfo, error) {
	return mo.fs.Stat(context.Background(), mo.name)
}

func (mo *file) ReadFrom(r io.Reader) (n int64, err error) {
	ctx := context.Background()

	_, err = mo.fs.client.PutObject(ctx, mo.fs.BucketName, strings.TrimPrefix(mo.name, "/"), r, -1, minio.PutObjectOptions{ContentType: "application/octet-stream"})

	if err != nil {
		log.Println("Upload failed", err)
		return 0, err
	}

	log.Println("Successfully uploaded bytes: ", n)

	return n, nil
}

func (mo *file) Write(p []byte) (n int, err error) {
	return len(p), nil // useless
}

func (mo *file) Readdir(count int) (fileInfoList []os.FileInfo, err error) {
	ctx := context.Background()
	name := path.Clean(mo.name)

	if !strings.HasSuffix(name, "/") {
		name = name + "/"
	}

	name = strings.TrimLeft(name, "/")

	// List all objects from a bucket-name with a matching prefix.
	for object := range mo.fs.client.ListObjects(ctx, mo.fs.BucketName, minio.ListObjectsOptions{
		Prefix: name,
	}) {
		err = object.Err
		if err != nil {
			fmt.Println(object.Err)
			// return
			break
		}

		if object.StorageClass == "" && object.ETag == "" && object.Size == 0 {
			object.ContentType = "inode/directory"
		}

		fileInfoList = append(fileInfoList, &fileInfo{object})
	}

	return fileInfoList, err
}
