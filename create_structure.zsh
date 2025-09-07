#!/usr/bin/env zsh

# Base project dir (current directory). Change if needed:
BASE_DIR="."

dirs=(
  "$BASE_DIR/config"
  "$BASE_DIR/src"
  "$BASE_DIR/src/config"
  "$BASE_DIR/src/executor"
  "$BASE_DIR/src/sources"
  "$BASE_DIR/src/utils"
  "$BASE_DIR/examples"
  "$BASE_DIR/tests"
)

files=(
  "$BASE_DIR/Cargo.toml"
  "$BASE_DIR/README.md"
  "$BASE_DIR/config/default.json"
  "$BASE_DIR/config/schema.json"
  "$BASE_DIR/src/main.rs"
  "$BASE_DIR/src/lib.rs"
  "$BASE_DIR/src/config/mod.rs"
  "$BASE_DIR/src/config/loader.rs"
  "$BASE_DIR/src/config/models.rs"
  "$BASE_DIR/src/executor/mod.rs"
  "$BASE_DIR/src/executor/runtime.rs"
  "$BASE_DIR/src/executor/actions.rs"
  "$BASE_DIR/src/sources/mod.rs"
  "$BASE_DIR/src/sources/file.rs"
  "$BASE_DIR/src/sources/directory.rs"
  "$BASE_DIR/src/sources/tcp.rs"
  "$BASE_DIR/src/utils/mod.rs"
  "$BASE_DIR/src/utils/interpolation.rs"
  "$BASE_DIR/src/utils/window.rs"
  "$BASE_DIR/examples/simple_macro.json"
  "$BASE_DIR/examples/multi_step_workflow.json"
  "$BASE_DIR/tests/integration_test.rs"
)

# Create directories if missing
for d in "${dirs[@]}"; do
  if [[ ! -d "$d" ]]; then
    mkdir -p "$d"
  fi
done

# Create empty files if missing
for f in "${files[@]}"; do
  if [[ ! -e "$f" ]]; then
    : > "$f"
  fi
done

echo "Structure créée (sans contenu)."