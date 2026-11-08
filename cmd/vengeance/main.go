// Command vengeance runs the server (`vengeance serve`) and uploads reports (`vengeance push`).
package main

import (
	"fmt"
	"net/http"
	"os"
	"time"
)

var version = "dev"

const usage = `Vengeance, a self-hosted security inbox.

Usage:
  vengeance serve              run the server (configured by VENGEANCE_* environment variables)
  vengeance push [flags] FILE  upload a scanner report (run "vengeance push -h" for flags)
  vengeance healthcheck        exit 0 if the local server answers /healthz (for container health checks)
  vengeance version            print the version
`

func main() {
	if len(os.Args) < 2 {
		fmt.Fprint(os.Stderr, usage)
		os.Exit(2)
	}
	var err error
	switch os.Args[1] {
	case "serve":
		err = serve()
	case "push":
		err = push(os.Args[2:])
	case "healthcheck":
		err = healthcheck()
	case "version", "--version", "-v":
		fmt.Println(version)
	case "help", "-h", "--help":
		fmt.Print(usage)
	default:
		fmt.Fprintf(os.Stderr, "unknown command %q\n\n%s", os.Args[1], usage)
		os.Exit(2)
	}
	if err != nil {
		fmt.Fprintln(os.Stderr, "error:", err)
		os.Exit(1)
	}
}

func healthcheck() error {
	addr := os.Getenv("VENGEANCE_LISTEN")
	if addr == "" {
		addr = ":8080"
	}
	if addr[0] == ':' {
		addr = "127.0.0.1" + addr
	}
	c := &http.Client{Timeout: 3 * time.Second}
	resp, err := c.Get("http://" + addr + "/healthz")
	if err != nil {
		return err
	}
	resp.Body.Close()
	if resp.StatusCode != 200 {
		return fmt.Errorf("healthz answered %d", resp.StatusCode)
	}
	return nil
}
