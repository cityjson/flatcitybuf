#!/bin/bash

# Check if directory argument is provided
if [ $# -ne 1 ]; then
  echo "usage: $0 <directory>"
  exit 1
fi

# Get the directory path
dir_path="$1"

# Check if directory exists
if [ ! -d "$dir_path" ]; then
  echo "error: directory '$dir_path' does not exist"
  exit 1
fi

# Find all .json files and process them
find "$dir_path" -type f -name "*.json" | while read -r file; do
  # Remove .json extension regardless of other dots in filename
  base_name="${file%.json}"

  echo "=== ${file} is being converted ==="

  # Run the conversion command
  cargo run -p fcb_cli cbor -i "$file" -o "${base_name}.cbor"

  echo "conversion completed for ${file}"
  echo
done

echo "all conversions completed"
