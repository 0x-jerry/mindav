package main

import (
	"mindav/config"

	"github.com/gin-gonic/gin"
)

func main() {
	port := config.Conf.Port

	r := gin.New()

	r.Run(":" + port)
}
