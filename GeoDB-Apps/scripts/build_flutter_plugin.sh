#!/usr/bin/env zsh
# scripts/build_flutter_plugin.sh

SCRIPT_DIR=${0:A:h}
source "$SCRIPT_DIR/build_helper.zsh"

bh_parse_args "$@"
bh_print_config

CRATE_DIR="crates/$FFI_CRATE"
PLUGIN_DIR="$CRATE_DIR/$PLUGIN_NAME"

# 1. Ensure Flutter Plugin Scaffolding Exists
if [[ ! -d "$PLUGIN_DIR" || "$FORCE_PLUGIN" = true ]]; then
  if [[ "$FORCE_PLUGIN" = true && -d "$PLUGIN_DIR" ]]; then
      log_warn "Force recreating plugin (this might wipe custom changes in lib/!)"
       rm -rf "$PLUGIN_DIR" # Dangerous! Maybe just let flutter create overwrite safe parts?
      # Flutter create usually skips existing files, so safe to run again.
  fi

  log_info "Creating/Updating Flutter plugin: $PLUGIN_NAME"
  bh_check_command flutter

  pushd "$CRATE_DIR" >/dev/null
  log_cmd flutter create --template=plugin --platforms=android,ios "$PLUGIN_NAME"
  popd >/dev/null
fi

# 2. Ensure XCFramework Exists (Dependency)
if [[ ! -d "$CRATE_DIR/$XC_FRAMEWORK_NAME" ]]; then
  log_warn "XCFramework not found. Triggering build..."
  # We pass the granular flag if global force was set, or just defaults
  "$SCRIPT_DIR/build_xcframework.sh" --ffi-crate "$FFI_CRATE" --xc-framework-name "$XC_FRAMEWORK_NAME"
fi

# 3. Install Artifacts into Plugin
log_info "Installing artifacts into Flutter plugin..."
if [[ ! -d "$PLUGIN_DIR" || "$FORCE_PLUGIN" = true ]]; then
# A. The XCFramework
IOS_DIR="$PLUGIN_DIR/ios"
log_cmd rm -rf "$IOS_DIR/$XC_FRAMEWORK_NAME"
log_cmd cp -R "$CRATE_DIR/$XC_FRAMEWORK_NAME" "$IOS_DIR/"

# B. The Swift Bindings
CLASSES_DIR="$IOS_DIR/Classes"
mkdir -p "$CLASSES_DIR"

# Copy generated swift files
log_cmd cp "$CRATE_DIR/generated/swift/"*.swift "$CLASSES_DIR/"
else
  log_info "Skipping plugin copy"
fi
log_success "Flutter plugin '$PLUGIN_NAME' prepared (iOS side)."