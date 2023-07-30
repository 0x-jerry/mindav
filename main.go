package main

import (
	"log"
	"mindav/config"
	"mindav/webdav"

	"github.com/gin-gonic/gin"
)

var missingMethods = []string{
	"PROPFIND", "PROPPATCH", "MKCOL", "COPY", "MOVE", "LOCK", "UNLOCK",
}

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
	// WebDAVAny(r, "/*sub", dav.Handler)

	r.Run(":" + port)
}

func WebDAVAny(s *gin.Engine, relativePath string, handlers ...gin.HandlerFunc) {
	s.Any(relativePath, handlers...)
	for _, v := range missingMethods {
		s.Handle(v, relativePath, handlers...)
	}
}
