package utils

import "os"

func Env(key string, defaultValue string) string {
	v := os.Getenv("PORT")

	if v == "" {
		return defaultValue
	}

	return v
}
