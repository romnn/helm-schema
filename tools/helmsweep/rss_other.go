//go:build !unix

package main

import "runtime"

// peakRSS approximates the peak resident set size where getrusage does not
// exist: the memory the Go runtime obtained from the operating system.
func peakRSS() int64 {
	var stats runtime.MemStats
	runtime.ReadMemStats(&stats)
	return int64(stats.Sys)
}
