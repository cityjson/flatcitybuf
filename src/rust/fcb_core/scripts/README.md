# FlatCityBuf Benchmarking Tools

This directory contains scripts for benchmarking FlatCityBuf compared to other data formats.

## Requirements

For visualization script:

```
pip install pandas matplotlib seaborn
```

## Running Benchmarks

The `run_benchmarks.sh` script automates running all benchmarks:

```bash
# Run standard benchmarks
./run_benchmarks.sh

# Run standard benchmarks + heap profiling (slower)
./run_benchmarks.sh --with-heap
```

The benchmarks will:

1. Measure read performance for multiple data formats
2. Record read time, peak memory usage, and CPU usage
3. Save detailed results in the `benchmark_results/` directory

## Visualizing Results

After running benchmarks, use the visualization script:

```bash
# Generate charts from benchmark results
./visualize_benchmarks.py benchmark_results/[DATE]/standard_bench.txt
```

This generates:

- Bar charts comparing formats across datasets
- CSV files with detailed metrics
- Summary statistics showing the best format for each metric

## Metrics Collected

The benchmarks measure:

- **Read time**: Average time to read and process each file
- **Peak RSS**: Maximum memory usage during processing
- **CPU usage**: Average CPU utilization percentage
- **Heap allocation**: Memory allocation patterns (when using `--with-heap`)

## Supported Formats

The benchmark compares:

- FlatCityBuf (.fcb)
- CityJSONTextSequence (.jsonl)
- CBOR (.cbor)
- BSON (.bson)

## Adding New Datasets

To benchmark new datasets, add the files to the `benchmark_data` directory and update the `DATASETS` constant in `benches/read.rs`.
