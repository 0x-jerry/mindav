package main

import (
	"log"
	"mindav/config"
	"mindav/webdav"

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

	dav := webdav.New()

	WebDAVAny(r, "/", dav.Handler)

	r.Run(":" + port)
}

func WebDAVAny(s *gin.Engine, relativePath string, handlers ...gin.HandlerFunc) {
	s.Any(relativePath, handlers...)
	s.Handle("PROPFIND", relativePath, handlers...)
	s.Handle("PROPPATCH", relativePath, handlers...)
	s.Handle("MKCOL", relativePath, handlers...)
	s.Handle("COPY", relativePath, handlers...)
	s.Handle("MOVE", relativePath, handlers...)
	s.Handle("LOCK", relativePath, handlers...)
	s.Handle("UNLOCK", relativePath, handlers...)
}
