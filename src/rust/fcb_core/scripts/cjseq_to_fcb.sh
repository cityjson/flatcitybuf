#!/bin/bash

# Check if input and output directory arguments are provided
if [ $# -ne 2 ]; then
  echo "usage: $0 <input_dir> <output_dir>"
  exit 1
fi

# Get the directory paths, dropping any trailing slashes
input_dir="${1%/}"
output_dir="${2%/}"

# Check if input directory exists
if [ ! -d "$input_dir" ]; then
  echo "error: directory '$input_dir' does not exist"
  exit 1
fi

# Create the output directory if it does not exist
mkdir -p "$output_dir"

# Find all .jsonl files and process them
find "$input_dir" -type f -name "*.jsonl" | while read -r file; do
  # Path of the input file relative to the input directory
  rel_path="${file#"$input_dir"/}"

  # Remove .jsonl extension regardless of other dots in filename
  base_name="${rel_path%.jsonl}"

  # Mirror the relative path under the output directory
  out_file="${output_dir}/${base_name}.fcb"

  echo "=== ${file} is being converted ==="

  # Make sure the destination directory exists
  mkdir -p "$(dirname "$out_file")"

  # Run the conversion command (-A indexes all attributes)
  cargo run -p fcb_cli -- ser -A "$file" "$out_file"

  echo "conversion completed for ${file}"
  echo
done

echo "all conversions completed"
