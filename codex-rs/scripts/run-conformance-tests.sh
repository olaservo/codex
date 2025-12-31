#!/bin/bash
# Run MCP conformance tests against Codex's real MCP client implementation
# Requires the conformance test framework to be available

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CODEX_RS_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
CONFORMANCE_DIR="${CONFORMANCE_DIR:-}"

# Find or use conformance directory
if [ -z "$CONFORMANCE_DIR" ]; then
    # Try common locations
    if [ -d "$CODEX_RS_DIR/../conformance" ]; then
        CONFORMANCE_DIR="$CODEX_RS_DIR/../conformance"
    elif [ -d "$HOME/conformance" ]; then
        CONFORMANCE_DIR="$HOME/conformance"
    else
        echo "Error: Cannot find conformance test framework"
        echo "Set CONFORMANCE_DIR environment variable or install conformance tests"
        exit 1
    fi
fi

# Build the Codex CLI
echo "Building Codex CLI..."
cd "$CODEX_RS_DIR"
cargo build -p codex-cli

CODEX_PATH="$CODEX_RS_DIR/target/debug/codex"

# On Windows, add .exe extension
if [[ "$OSTYPE" == "msys" || "$OSTYPE" == "cygwin" || "$OSTYPE" == "win32" ]]; then
    CODEX_PATH="$CODEX_PATH.exe"
fi

# Verify binary exists
if [ ! -f "$CODEX_PATH" ]; then
    echo "Error: Codex binary not found at $CODEX_PATH"
    exit 1
fi

echo "Using conformance tests from: $CONFORMANCE_DIR"
echo "Using Codex client: $CODEX_PATH mcp conformance"
echo ""

cd "$CONFORMANCE_DIR"

# Run basic tests using Codex's real MCP client
echo "=== Running Initialize Test ==="
npm run start -- client --command "$CODEX_PATH mcp conformance" --scenario initialize

echo ""
echo "=== Running Tools Call Test ==="
npm run start -- client --command "$CODEX_PATH mcp conformance" --scenario tools_call

echo ""
echo "=== Test Summary ==="
echo "Conformance tests completed using Codex's real MCP client implementation."
echo ""
echo "Note: Some tests (elicitation, auth) require interactive UI and are skipped."
