#!/bin/bash

# Script to generate Rust models from OpenAPI schema

set -e

SCHEMA_DIR="schema"
SCHEMA_FILE="3dbagapi_spec.yaml"
OUTPUT_DIR="src/openapi"
GENERATOR_CONFIG="openapi-generator-config.yaml"

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Create OpenAPI Generator configuration
cat > "$GENERATOR_CONFIG" << 'EOF'
# OpenAPI Generator configuration for Rust
generatorName: rust
inputSpec: schema/3dbagapi_spec.yaml
outputDir: src/openapi
additionalProperties:
  packageName: openapi
  packageVersion: "0.1.0"
  library: reqwest
  supportAsync: true
  avoidBoxedModels: true
  preferUnsignedInt: false
  bestFitInt: true
  hideGenerationTimestamp: true
  useSingleRequestParameter: false
globalProperties:
  models: true
  apis: false
  supportingFiles: true
  modelDocs: false
  apiDocs: false
  modelTests: true
  apiTests: true
EOF

echo "Generating Rust models from OpenAPI schema..."

# Check if running with Docker or NPM
if command -v docker &> /dev/null && docker info &> /dev/null; then
    echo "Using Docker to run openapi-generator..."
    docker run --rm \
        -v "${PWD}:/local" \
        openapitools/openapi-generator-cli:latest generate \
        -c "/local/${GENERATOR_CONFIG}"
elif command -v openapi-generator-cli &> /dev/null; then
    echo "Using openapi-generator-cli..."
    openapi-generator-cli generate \
        -i "${SCHEMA_DIR}/${SCHEMA_FILE}" \
        -g rust \
        -o "${OUTPUT_DIR}" \
        --package-name openapi \
        --library reqwest \
        --additional-properties=supportAsync=true,avoidBoxedModels=true \
        --global-property=models,apis,supportingFiles \
        --global-property=modelDocs=false,apiDocs=false
else
    echo "Error: Neither Docker nor openapi-generator-cli is available"
    echo "Please install one of them:"
    echo "  - Docker: https://docs.docker.com/get-docker/"
    echo "  - openapi-generator-cli: npm install -g @openapitools/openapi-generator-cli"
    exit 1
fi

# Post-processing: the generator writes a whole crate; fcb_api keeps only
# the models, as its own `openapi::models` module.
if [ -d "$OUTPUT_DIR/src/models" ]; then
    echo "Setting up module structure..."
    rm -rf "$OUTPUT_DIR/models"
    mv "$OUTPUT_DIR/src/models" "$OUTPUT_DIR/models"
    rm -rf "$OUTPUT_DIR/src" "$OUTPUT_DIR/docs" "$OUTPUT_DIR/Cargo.toml" \
        "$OUTPUT_DIR/README.md" "$OUTPUT_DIR/git_push.sh" \
        "$OUTPUT_DIR/.gitignore" "$OUTPUT_DIR/.travis.yml" \
        "$OUTPUT_DIR/.openapi-generator" "$OUTPUT_DIR/.openapi-generator-ignore"
    for f in "$OUTPUT_DIR"/models/*.rs; do
        sed 's/^use crate::models;$/use crate::openapi::models;/' "$f" > "$f.tmp"
        mv "$f.tmp" "$f"
    done
    cargo fmt -p fcb_api
    echo "Models generated successfully in $OUTPUT_DIR/models"
else
    echo "Error: Output directory was not created"
    exit 1
fi

# Clean up config file
rm -f "$GENERATOR_CONFIG"