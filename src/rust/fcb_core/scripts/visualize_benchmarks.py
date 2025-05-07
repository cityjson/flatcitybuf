#!/usr/bin/env python3
import os
import re
import sys
import pandas as pd
import matplotlib.pyplot as plt
import seaborn as sns


def parse_benchmark_output(file_path):
    """Parse the benchmark output file and extract metrics."""
    with open(file_path, "r") as f:
        content = f.read()

    # Extract the detailed results table
    pattern = (
        r"Detailed Benchmark Results:.*?Dataset.*?Format.*?Mean Time.*?"
        r"Peak Memory.*?CPU Usage.*?([-]+)\n(.*?)Summary"
    )
    detailed_match = re.search(pattern, content, re.DOTALL)

    if not detailed_match:
        print(f"Could not find detailed results in {file_path}")
        return None

    # Process the detailed results
    results = []
    result_lines = detailed_match.group(2).strip().split("\n")

    for line in result_lines:
        line = line.strip()
        if line.startswith("---"):
            continue

        parts = re.split(r"\s{2,}", line)
        if len(parts) >= 5:
            dataset, format_name, time, memory, cpu = parts[:5]

            # Clean the values
            time = time.strip()
            memory = memory.strip()
            cpu = cpu.strip().rstrip("%")

            # Convert time to milliseconds
            if "s" in time and "ms" not in time:
                time_value = float(time.rstrip("s")) * 1000
            else:
                time_value = float(time.rstrip("ms"))

            # Convert memory to MB
            if "GB" in memory:
                memory_value = float(memory.rstrip("GB")) * 1024
            elif "MB" in memory:
                memory_value = float(memory.rstrip("MB"))
            elif "KB" in memory:
                memory_value = float(memory.rstrip("KB")) / 1024
            else:
                memory_value = float(memory.rstrip("B")) / (1024 * 1024)

            # Convert CPU percentage to float
            try:
                cpu_value = float(cpu)
            except ValueError:
                cpu_value = 0.0

            results.append(
                {
                    "dataset": dataset,
                    "format": format_name,
                    "time_ms": time_value,
                    "memory_mb": memory_value,
                    "cpu_percent": cpu_value,
                }
            )

    return pd.DataFrame(results)


def generate_visualizations(df, output_dir):
    """Generate visualization charts from the benchmark data."""
    if df is None or df.empty:
        print("No data to visualize")
        return

    os.makedirs(output_dir, exist_ok=True)

    # Set style
    plt.style.use("ggplot")
    sns.set(style="whitegrid")

    # Time comparison chart
    plt.figure(figsize=(14, 8))
    chart = sns.barplot(x="dataset", y="time_ms", hue="format", data=df)
    chart.set_title("Reading Time Comparison (lower is better)", fontsize=16)
    chart.set_xlabel("Dataset", fontsize=14)
    chart.set_ylabel("Time (ms)", fontsize=14)
    chart.set_xticklabels(
        chart.get_xticklabels(), rotation=45, horizontalalignment="right"
    )
    plt.tight_layout()
    plt.savefig(os.path.join(output_dir, "time_comparison.png"), dpi=300)
    plt.close()

    # Memory usage chart
    plt.figure(figsize=(14, 8))
    chart = sns.barplot(x="dataset", y="memory_mb", hue="format", data=df)
    chart.set_title("Peak Memory Usage (lower is better)", fontsize=16)
    chart.set_xlabel("Dataset", fontsize=14)
    chart.set_ylabel("Memory (MB)", fontsize=14)
    chart.set_xticklabels(
        chart.get_xticklabels(), rotation=45, horizontalalignment="right"
    )
    plt.tight_layout()
    plt.savefig(os.path.join(output_dir, "memory_comparison.png"), dpi=300)
    plt.close()

    # CPU usage chart
    plt.figure(figsize=(14, 8))
    chart = sns.barplot(x="dataset", y="cpu_percent", hue="format", data=df)
    chart.set_title("CPU Usage (lower is better)", fontsize=16)
    chart.set_xlabel("Dataset", fontsize=14)
    chart.set_ylabel("CPU Usage (%)", fontsize=14)
    chart.set_xticklabels(
        chart.get_xticklabels(), rotation=45, horizontalalignment="right"
    )
    plt.tight_layout()
    plt.savefig(os.path.join(output_dir, "cpu_comparison.png"), dpi=300)
    plt.close()

    # Get unique formats and datasets
    formats = df["format"].unique()
    datasets = df["dataset"].unique()

    # Export the data as CSV for further analysis
    df.to_csv(os.path.join(output_dir, "benchmark_results.csv"), index=False)

    # Generate a summary table
    summary = []
    for dataset in datasets:
        dataset_df = df[df["dataset"] == dataset]

        # Find the best format for each metric
        best_time = dataset_df.loc[dataset_df["time_ms"].idxmin()]
        best_memory = dataset_df.loc[dataset_df["memory_mb"].idxmin()]
        best_cpu = dataset_df.loc[dataset_df["cpu_percent"].idxmin()]

        summary.append(
            {
                "dataset": dataset,
                "fastest_format": best_time["format"],
                "fastest_time_ms": best_time["time_ms"],
                "lowest_memory_format": best_memory["format"],
                "lowest_memory_mb": best_memory["memory_mb"],
                "lowest_cpu_format": best_cpu["format"],
                "lowest_cpu_percent": best_cpu["cpu_percent"],
            }
        )

    summary_df = pd.DataFrame(summary)
    summary_df.to_csv(os.path.join(output_dir, "benchmark_summary.csv"), index=False)

    # Print a summary
    print("\nBenchmark Summary:")
    print(f"Total datasets: {len(datasets)}")
    print(f"Formats compared: {', '.join(formats)}")

    # Count wins by format
    wins = {format_name: {"time": 0, "memory": 0, "cpu": 0} for format_name in formats}
    for _, row in summary_df.iterrows():
        wins[row["fastest_format"]]["time"] += 1
        wins[row["lowest_memory_format"]]["memory"] += 1
        wins[row["lowest_cpu_format"]]["cpu"] += 1

    print("\nWins by format:")
    for format_name, metrics in wins.items():
        print(f"  {format_name}:")
        print(f"    Fastest: {metrics['time']} datasets")
        print(f"    Lowest memory: {metrics['memory']} datasets")
        print(f"    Lowest CPU: {metrics['cpu']} datasets")


def main():
    if len(sys.argv) < 2:
        print(
            "Usage: python visualize_benchmarks.py <benchmark_result_file> "
            "[output_directory]"
        )
        sys.exit(1)

    input_file = sys.argv[1]
    output_dir = sys.argv[2] if len(sys.argv) > 2 else "benchmark_visualizations"

    df = parse_benchmark_output(input_file)
    generate_visualizations(df, output_dir)


if __name__ == "__main__":
    main()
