package config

import (
	"github.com/spf13/viper"
)

type AppConfig struct {
	Port string
}

var Conf AppConfig

func init() {
	viper.SetConfigName("config")
	viper.AddConfigPath(".")

	viper.SetDefault("app.port", "8080")

	viper.ReadInConfig()

	Conf.Port = viper.GetString("app.port")
}
