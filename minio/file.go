package minio

import (
	"context"
	"crypto/md5"
	"fmt"
	"io"
	"log"
	"mindav/config"
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

	log.Println("Start uploading:", mo.name)

	n, err = mo.upload(r)

	if err != nil {
		log.Println("upload failed", err)
		return 0, nil
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

func (mo *file) upload(r io.Reader) (n int64, err error) {

	if config.Conf.UploadMode == "memory" {
		return mo.uploadInMemoryMode(r)
	}

	return mo.uploadInFileMode(r)
}

func (mo *file) uploadInMemoryMode(r io.Reader) (n int64, err error) {
	ctx := context.Background()

	info, err := mo.fs.client.PutObject(ctx, mo.fs.BucketName, strings.TrimPrefix(mo.name, "/"), r, -1, minio.PutObjectOptions{ContentType: "application/octet-stream"})

	if err != nil {
		return 0, err
	}

	return info.Size, nil

}

func (mo *file) uploadInFileMode(r io.Reader) (n int64, err error) {
	ctx := context.Background()

	md5 := fmt.Sprintf("%x", md5.Sum([]byte(mo.name)))

	tmpFilePath := path.Join("./tmp", md5)

	err = os.MkdirAll("./tmp", os.ModePerm)

	if err != nil {
		return 0, err
	}

	f, err := os.Create(tmpFilePath)
	if err != nil {
		return 0, err
	}

	defer f.Close()
	defer func(p string) {
		err = os.RemoveAll(p)
		if err != nil {
			log.Println("remove file failed", tmpFilePath)
		}
	}(tmpFilePath)

	buf := make([]byte, 1024)
	for {
		// read a chunk
		n, err := r.Read(buf)
		if err != nil && err != io.EOF {
			return 0, err
		}
		if n == 0 {
			break
		}

		// write a chunk
		if _, err := f.Write(buf[:n]); err != nil {
			return 0, err
		}
	}

	info, err := mo.fs.client.FPutObject(ctx, mo.fs.BucketName, strings.TrimPrefix(mo.name, "/"), tmpFilePath, minio.PutObjectOptions{ContentType: "application/octet-stream"})

	if err != nil {
		return 0, nil
	}

	return info.Size, nil
}
