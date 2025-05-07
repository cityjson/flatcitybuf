#!/bin/bash
set -e

# Create output directory
OUTDIR="benchmark_results/$(date +%Y-%m-%d_%H-%M-%S)"
mkdir -p $OUTDIR

echo "running standard benchmarks..."
cargo bench --bench read -- --verbose >"$OUTDIR/standard_bench.txt" 2>&1

echo "running memory profile benchmarks..."
RUSTFLAGS="-C force-frame-pointers=yes" cargo bench --bench read >"$OUTDIR/memory_bench.txt" 2>&1

# Only run heap profiling if explicitly requested
if [ "$1" == "--with-heap" ]; then
  echo "running heap profiling benchmarks..."
  DHAT_OUT_FILE="$OUTDIR/dhat-heap.json" cargo bench --bench read --features dhat-heap -- --verbose >"$OUTDIR/heap_bench.txt" 2>&1
fi

echo "benchmarks completed, results saved to $OUTDIR"
