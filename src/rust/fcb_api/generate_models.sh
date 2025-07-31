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

# Post-processing: Setup proper module structure
if [ -d "$OUTPUT_DIR" ]; then
    echo "Setting up module structure..."

    # Create a simple mod.rs to re-export models
    cat > "$OUTPUT_DIR/mod.rs" << 'MODEOF'
// Re-export generated OpenAPI models
pub use self::models::*;

// Include the generated modules
pub mod models;
MODEOF

    echo "Models generated successfully in $OUTPUT_DIR"
else
    echo "Error: Output directory was not created"
    exit 1
fi

# Clean up config file
rm -f "$GENERATOR_CONFIG"