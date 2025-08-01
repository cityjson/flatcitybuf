#!/bin/bash

# Script to fetch the latest OpenAPI schema from 3DBAG API

set -e

SCHEMA_DIR="schema"
SCHEMA_FILE="3dbagapi_spec.yaml"
SCHEMA_URL="https://raw.githubusercontent.com/3DBAG/3dbag-api/master/app/schemas/3dbagapi_spec.yaml"

# Create schema directory if it doesn't exist
mkdir -p "$SCHEMA_DIR"

echo "Fetching OpenAPI schema from 3DBAG..."
curl -L -o "$SCHEMA_DIR/$SCHEMA_FILE" "$SCHEMA_URL"

if [ $? -eq 0 ]; then
    echo "Schema downloaded successfully to $SCHEMA_DIR/$SCHEMA_FILE"
else
    echo "Failed to download schema"
    exit 1
fi

# Check if openapi-generator-cli is installed
if ! command -v openapi-generator-cli &> /dev/null; then
    echo "Warning: openapi-generator-cli is not installed"
    echo "To install it, run: npm install -g @openapitools/openapi-generator-cli"
    echo "Or use Docker: docker pull openapitools/openapi-generator-cli"
fi