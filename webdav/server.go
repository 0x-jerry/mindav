package webdav

import (
	"mindav/minio"

	"github.com/gin-gonic/gin"
	"golang.org/x/net/webdav"
)

type Server struct {
	server *webdav.Handler
}

func New() *Server {

	handler := webdav.Handler{
		FileSystem: minio.New(),
		LockSystem: webdav.NewMemLS(),
	}

	s := Server{
		server: &handler,
	}

	return &s
}

func (s *Server) Handler(ctx *gin.Context) {
	s.server.ServeHTTP(ctx.Writer, ctx.Request)
}
