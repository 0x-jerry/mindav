package config

import (
	"log"

	"github.com/spf13/viper"
)

type AppConfig struct {
	Port          string
	AdminName     string
	AdminPassword string

	// memory or file
	UploadMode string

	Minio Minio
}

var Conf AppConfig

func init() {
	viper.SetConfigName("config")
	viper.AddConfigPath(".")

	viper.SetDefault("app.port", "8080")
	viper.SetDefault("app.admin.username", "admin")
	viper.SetDefault("app.admin.password", "password")
	viper.SetDefault("app.uploadMode", "file")

	viper.ReadInConfig()

	Conf.Port = viper.GetString("app.port")
	Conf.AdminName = viper.GetString("app.admin.username")
	Conf.AdminPassword = viper.GetString("app.admin.password")
	Conf.UploadMode = viper.GetString("app.uploadMode")

	Conf.Minio = Minio{
		Endpoint:        viper.GetString("minio.endpoint"),
		BucketName:      viper.GetString("minio.bucketName"),
		SSL:             viper.GetBool("minio.ssl"),
		AccessKey:       viper.GetString("minio.accessKey"),
		SecretAccessKey: viper.GetString("minio.secretAccessKey"),
	}

	log.Println("Load config", Conf)
}
