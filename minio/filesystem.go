package minio

import (
	"context"
	"log"
	"mindav/config"
	"os"

	"github.com/minio/minio-go/v7"
	"github.com/minio/minio-go/v7/pkg/credentials"
	"golang.org/x/net/webdav"
)

type MinioFS struct {
	config.Minio
	client *minio.Client
}

func New() *MinioFS {
	m := MinioFS{
		Minio: config.Conf.Minio,
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

	return nil
}

func (m *MinioFS) OpenFile(ctx context.Context, name string, flag int, perm os.FileMode) (webdav.File, error) {
	return nil, nil
}

func (m *MinioFS) RemoveAll(ctx context.Context, name string) error {
	return nil
}

func (m *MinioFS) Rename(ctx context.Context, oldName, newName string) error {
	return nil
}

func (m *MinioFS) Stat(ctx context.Context, name string) (os.FileInfo, error) {
	return nil, nil
}
