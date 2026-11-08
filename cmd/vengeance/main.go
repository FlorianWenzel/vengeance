// Command vengeance runs the server (`vengeance serve`) and uploads reports (`vengeance push`).
package main

import (
	"fmt"
	"os"
)

var version = "dev"

const usage = `Vengeance, a self-hosted security inbox.

Usage:
  vengeance serve              run the server (configured by VENGEANCE_* environment variables)
  vengeance push [flags] FILE  upload a scanner report (run "vengeance push -h" for flags)
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
