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
	dirs   map[string]bool
}

func NewFS() *MinioFS {
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
		dirs: make(map[string]bool),
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
	name = cleanPathName(name)

	emptyFile := bytes.NewBuffer([]byte{})

	emptyFilePath := path.Join(name, KEEP_FILE_NAME)

	_, err := m.client.PutObject(ctx, m.BucketName, emptyFilePath, emptyFile, int64(emptyFile.Len()), minio.PutObjectOptions{
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
	name = cleanPathName(name)

	object, err := m.client.GetObject(ctx, m.BucketName, name, minio.GetObjectOptions{})

	if err != nil {
		log.Println("OpenFile failed", name)
		return nil, err
	}

	log.Println("OpenFile success", name)
	return &file{fs: m, Object: object, name: name}, nil
}

func (m *MinioFS) RemoveAll(ctx context.Context, name string) error {
	name = cleanPathName(name)

	objectChan := m.getObjectsByPrefix(ctx, name)

	for err := range m.client.RemoveObjects(ctx, m.BucketName, objectChan, minio.RemoveObjectsOptions{}) {
		if err.Err != nil {
			log.Println("RemoveAll failed", err)
			return err.Err
		}
	}

	log.Println("RemoveAll all sub object")

	if err := m.client.RemoveObject(ctx, m.BucketName, name, minio.RemoveObjectOptions{}); err != nil {
		log.Println("RemoveAll failed", err)
		return err
	}

	m.resetDirCheck(name)

	log.Println("RemoveAll success")
	return nil
}

func (m *MinioFS) Rename(ctx context.Context, oldName, newName string) error {
	oldName = cleanPathName(oldName)
	newName = cleanPathName(newName)

	log.Println("Rename", oldName, newName)

	objectChan := m.getObjectsByPrefix(ctx, oldName)

	for obj := range objectChan {
		oldKeyPath := obj.Key
		newKeyPath := strings.Replace(oldKeyPath, oldName, newName, 1)

		dest := minio.CopyDestOptions{
			Bucket: m.BucketName,
			Object: newKeyPath,
		}

		src := minio.CopySrcOptions{
			Bucket: m.BucketName,
			Object: oldKeyPath,
		}

		if _, err := m.client.CopyObject(ctx, dest, src); err != nil {
			log.Println("Copy file failed", err)
		}

		log.Println("Copy file success.", oldKeyPath, "to", newKeyPath)
	}

	m.resetDirCheck(newName)
	m.RemoveAll(ctx, oldName)

	log.Println("Rename success")
	return nil
}

func (m *MinioFS) Stat(ctx context.Context, name string) (os.FileInfo, error) {
	name = cleanPathName(name)

	if name == "/" || m.isDir(ctx, name) {
		return &fileInfo{minio.ObjectInfo{
			Key:          name,
			Size:         0,
			LastModified: time.Now(),
			ContentType:  "inode/directory",
			ETag:         "",
			StorageClass: "",
		}}, nil
	}

	stat, err := m.client.StatObject(ctx, m.BucketName, name, minio.StatObjectOptions{})

	if err != nil {
		log.Println("Stat failed", err, name)
		return nil, os.ErrNotExist
	}

	return &fileInfo{stat}, nil
}

func (m *MinioFS) getObjectsByPrefix(ctx context.Context, prefix string) <-chan minio.ObjectInfo {
	opts := minio.ListObjectsOptions{
		Prefix:    prefix,
		Recursive: true,
	}

	return m.client.ListObjects(ctx, m.BucketName, opts)

}

func (m *MinioFS) isDir(ctx context.Context, name string) bool {

	if val, exists := m.dirs[name]; exists {
		return val
	}

	objectChan := make(chan minio.ObjectInfo)

	go func() {
		defer close(objectChan)

		opts := minio.ListObjectsOptions{
			Prefix:       name,
			Recursive:    false,
			WithVersions: false,
		}

		for object := range m.client.ListObjects(ctx, m.BucketName, opts) {
			if object.Err != nil {
				log.Println("ListObjects failed", name)
			}

			log.Println("Get object", object.Key)
			objectChan <- object
		}
	}()

	count := 0
	for obj := range objectChan {
		log.Println("Key:", obj.Key, "size:", obj.Size, "type:", obj.ContentType, "owner:", obj.Owner.ID)

		if obj.Owner.ID == "" {
			count++
		}

		if obj.Err != nil {
			log.Println("list dir error")
		}
	}

	isDir := count != 0

	m.dirs[name] = isDir

	return isDir
}

func (m *MinioFS) resetDirCheck(keyName string) {
	for key := range m.dirs {
		if strings.HasPrefix(key, keyName) {
			delete(m.dirs, key)
		}
	}
}
