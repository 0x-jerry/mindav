package minio

import (
	"context"
	"os"

	"golang.org/x/net/webdav"
)

type MinioFS struct {
}

func New() *MinioFS {
	m := MinioFS{}

	return &m
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
