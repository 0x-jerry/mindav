package minio

import (
	"bytes"
	"context"
	"log"
	"mindav/config"
	"os"
	"path"
	"strings"
	"time"

	"github.com/minio/minio-go/v7"
	"github.com/minio/minio-go/v7/pkg/credentials"
	"golang.org/x/net/webdav"
)

const KEEP_FILE_NAME = ".mindavkeep"
const KEEP_FILE_CONTENT_TYPE = "application/mindav-folder-keeper"

type MinioFS struct {
	config.Minio
	client *minio.Client
	root   *fileInfo
}

func New() *MinioFS {
	m := MinioFS{
		Minio: config.Conf.Minio,
		root: &fileInfo{minio.ObjectInfo{
			Key:          "/",
			Size:         0,
			LastModified: time.Now(),
			ContentType:  "inode/directory",
			ETag:         "",
			StorageClass: "",
		}},
	}

	m.initialize()

	return &m
}

func (m *MinioFS) initialize() error {
	s3, err := minio.New(m.Endpoint, &minio.Options{
		Creds:  credentials.NewStaticV4(m.AccessKey, m.SecretAccessKey, ""),
		Secure: m.SSL,
	})

	log.Println("[minio] Login to", m.Endpoint)

	if err != nil {
		return err
	}

	m.client = s3

	return nil
}

func (m *MinioFS) Mkdir(ctx context.Context, name string, perm os.FileMode) error {
	name = path.Clean(name)

	emptyFile := bytes.NewBuffer([]byte{})

	_, err := m.client.PutObject(ctx, m.BucketName, name, emptyFile, int64(emptyFile.Len()), minio.PutObjectOptions{
		ContentType: KEEP_FILE_CONTENT_TYPE,
	})

	if err != nil {
		log.Println("Mkdir failed", err)
		return err
	}

	log.Println("Mkdir success", name)
	return nil
}

func (m *MinioFS) OpenFile(ctx context.Context, name string, flag int, perm os.FileMode) (webdav.File, error) {
	name = path.Clean(name)

	object, err := m.client.GetObject(ctx, m.BucketName, name, minio.GetObjectOptions{})

	if err != nil {
		log.Println("OpenFile failed", name)
		return nil, err
	}

	log.Println("OpenFile success", name)
	return &file{fs: m, Object: object, name: name}, nil
}

func (m *MinioFS) RemoveAll(ctx context.Context, name string) error {
	name = path.Clean(name)

	objectChan := make(chan minio.ObjectInfo)

	go func() {
		opts := minio.ListObjectsOptions{
			Prefix:    name,
			Recursive: false,
		}

		for object := range m.client.ListObjects(ctx, m.BucketName, opts) {
			if object.Err != nil {
				log.Println("[RemoveAll] ListObjects failed", name)
			}
			objectChan <- object
		}
	}()

	for err := range m.client.RemoveObjects(ctx, m.BucketName, objectChan, minio.RemoveObjectsOptions{}) {
		if err.Err != nil {
			log.Println("RemoveAll failed", err)
			return err.Err
		}
	}

	if err := m.client.RemoveObject(ctx, m.BucketName, name, minio.RemoveObjectOptions{}); err != nil {
		log.Println("RemoveAll failed", err)
		return err
	}

	log.Println("RemoveAll success")
	return nil
}

func (m *MinioFS) Rename(ctx context.Context, oldName, newName string) error {
	oldName = path.Clean(oldName)
	newName = path.Clean(newName)

	dest := minio.CopyDestOptions{
		Bucket: m.BucketName,
		Object: newName,
	}

	src := minio.CopySrcOptions{
		Bucket: m.BucketName,
		Object: oldName,
	}

	if _, err := m.client.CopyObject(ctx, dest, src); err != nil {
		log.Println("Rename failed", err)
		return err
	}

	if err := m.client.RemoveObject(ctx, m.BucketName, oldName, minio.RemoveObjectOptions{}); err != nil {
		log.Println("Rename failed", err)
		return err
	}

	log.Println("Rename success")
	return nil
}

func (m *MinioFS) Stat(ctx context.Context, name string) (os.FileInfo, error) {
	name = path.Clean(name)

	if name == "/" {
		return m.root, nil
	}

	name = strings.TrimPrefix(name, "/")

	stat, err := m.client.StatObject(ctx, m.BucketName, name, minio.StatObjectOptions{})

	if err != nil {
		log.Println("Stat failed", err, name)

		if _, ok := err.(minio.ErrorResponse); ok {
			return &fileInfo{minio.ObjectInfo{
				Key:          name,
				Size:         0,
				LastModified: time.Now(),
				ContentType:  "inode/directory",
				ETag:         "",
				StorageClass: "",
			}}, nil
		}

		return nil, err
	}

	return &fileInfo{stat}, nil
}
