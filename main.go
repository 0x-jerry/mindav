package main

import (
	"log"
	"mindav/config"

	"github.com/gin-gonic/gin"
)

func main() {
	port := config.Conf.Port

	r := gin.Default()

	accounts := gin.Accounts{
		config.Conf.AdminName: config.Conf.AdminPassword,
	}

	log.Println("auth accounts", accounts)

	r.Use(gin.BasicAuth(accounts))

	r.Run(":" + port)
}
