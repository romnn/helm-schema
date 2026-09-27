//go:build unix

package main

import (
	"runtime"
	"syscall"
)

// peakRSS is this process's peak resident set size in bytes (getrusage
// reports bytes on Darwin and KiB elsewhere).
func peakRSS() int64 {
	var usage syscall.Rusage
	if syscall.Getrusage(syscall.RUSAGE_SELF, &usage) != nil {
		return 0
	}
	if runtime.GOOS == "darwin" {
		return usage.Maxrss
	}
	return usage.Maxrss * 1024
}
