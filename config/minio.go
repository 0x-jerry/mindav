package config

type Minio struct {
	Endpoint        string
	BucketName      string
	SSL             bool
	AccessKey       string
	SecretAccessKey string
}
