package minio

import (
	"context"
	"io"
	"os"

	"github.com/minio/minio-go/v7"
)

type file struct {
	*minio.Object

	client *MinioFS
	name   string
}

func (mo *file) Stat() (os.FileInfo, error) {
	return mo.client.Stat(context.Background(), mo.name)
}

func (mo *file) ReadFrom(r io.Reader) (n int64, err error) {
	// ctx := context.Background()

	// // memory mode
	// if config.GetBool("webdav.memory_upload_mode") {
	// 	info, err := mo.m.client.PutObject(ctx, mo.m.bucketName, strings.TrimPrefix(mo.name, "/"), r, -1, minio.PutObjectOptions{ContentType: "application/octet-stream"})
	// 	if err != nil {
	// 		return 0, log.Error(err, toto.V{"op": "ReadFrom", "name": mo.name})
	// 	}
	// 	n = info.Size
	// 	fmt.Println("Successfully uploaded bytes: ", n)
	// 	return n, nil
	// }

	// // file mode
	// tmpFilePath := path.Join(mo.m.uploadTmpPath, hash.Md5(mo.name))
	// f, err := os.Create(tmpFilePath)
	// if err != nil {
	// 	return 0, err
	// }
	// defer f.Close()
	// defer func(p string) {
	// 	err = os.RemoveAll(p)
	// 	if err != nil {
	// 		_ = log.Error(err, toto.V{"op": "upload", "name": mo.name, "tempName": p})
	// 	}
	// }(tmpFilePath)

	// buf := make([]byte, 1024)
	// for {
	// 	// read a chunk
	// 	n, err := r.Read(buf)
	// 	if err != nil && err != io.EOF {
	// 		return 0, err
	// 	}
	// 	if n == 0 {
	// 		break
	// 	}

	// 	// write a chunk
	// 	if _, err := f.Write(buf[:n]); err != nil {
	// 		return 0, err
	// 	}
	// }
	// info, err := mo.m.client.FPutObject(ctx, mo.m.bucketName, strings.TrimPrefix(mo.name, "/"), tmpFilePath, minio.PutObjectOptions{ContentType: "application/octet-stream"})
	// if err != nil {
	// 	return 0, log.Error(err, toto.V{"op": "ReadFrom", "name": mo.name})
	// }
	// n = info.Size

	// log.Trace(hash.Md5(mo.name), toto.V{"op": "upload", "name": mo.name})

	// fmt.Println("Successfully uploaded bytes: ", n)
	// return n, nil
	return 0, nil
}

func (mo *file) Write(p []byte) (n int, err error) {
	return len(p), nil // useless
}

func (mo *file) Readdir(count int) (fileInfoList []os.FileInfo, err error) {
	// ctx := context.Background()
	// log.Trace("file readDir", toto.V{"name": mo.name})

	// name, err := clearName(mo.name)
	// if err != nil {
	// 	return nil, err
	// }

	// if name != "" {
	// 	if !strings.HasSuffix(name, "/") {
	// 		name = name + "/"
	// 	}
	// }

	// // List all objects from a bucket-name with a matching prefix.
	// for object := range mo.m.client.ListObjects(ctx, mo.m.bucketName, minio.ListObjectsOptions{
	// 	Prefix: name,
	// }) {
	// 	err = object.Err
	// 	if err != nil {
	// 		fmt.Println(object.Err)
	// 		// return
	// 		break
	// 	}

	// 	if object.StorageClass == "" && object.ETag == "" && object.Size == 0 {
	// 		object.ContentType = "inode/directory"
	// 	}

	// 	fileInfoList = append(fileInfoList, &fileInfo{object})
	// }

	// return fileInfoList, err
	return nil, nil
}
