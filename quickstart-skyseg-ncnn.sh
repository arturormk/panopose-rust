#!/usr/bin/env bash
set -Eeuo pipefail

readonly DEFAULT_BUILD_DIR="thirdparty"
readonly DEFAULT_INSTALL_PREFIX="$HOME/.local"
readonly NCNN_REPO_URL="https://github.com/Tencent/ncnn.git"
readonly SKYSEG_REPO_URL="https://github.com/knyipab/skyseg-ncnn.git"
readonly COMPATIBILITY_DATE="August 31, 2026"

ASSUME_YES=0
BUILD_DIR="$DEFAULT_BUILD_DIR"
INSTALL_PREFIX="$DEFAULT_INSTALL_PREFIX"
NO_INSTALL=0
CLEAN=0

usage() {
  cat <<'EOF'
Usage: ./quickstart-skyseg-ncnn.sh [options]

Build and optionally install skyseg-ncnn, the optional external sky removal
tool used by PanoPose.

Options:
  -y, --yes             Answer yes to interactive prompts.
  --build-dir <path>    Build under this directory. Default: ./thirdparty
  --prefix <path>       Install prefix. Default: $HOME/.local
  --no-install          Build only; do not install skyseg-ncnn.
  --clean               Remove the build directory before starting.
  -h, --help            Show this help.
EOF
}

die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

warn() {
  printf 'warning: %s\n' "$*" >&2
}

info() {
  printf '%s\n' "$*"
}

optional_command_status() {
  local command_name="$1"
  if command -v "$command_name" >/dev/null 2>&1; then
    printf 'found'
  else
    printf 'missing'
  fi
}

need_command() {
  local command_name="$1"
  local description="$2"

  if ! command -v "$command_name" >/dev/null 2>&1; then
    die "$description is required but '$command_name' was not found in PATH."
  fi
}

confirm() {
  local prompt="$1"
  local reply

  if ((ASSUME_YES)); then
    return 0
  fi

  printf '%s [y/N] ' "$prompt"
  read -r reply
  case "$reply" in
    y|Y|yes|YES) return 0 ;;
    *) return 1 ;;
  esac
}

abs_path() {
  local path="$1"
  case "$path" in
    /*)
      printf '%s\n' "$path"
      ;;
    *)
      printf '%s/%s\n' "$(pwd -P)" "$path"
      ;;
  esac
}

display_path() {
  local path="$1"
  case "$path" in
    /*|./*|../*)
      printf '%s\n' "$path"
      ;;
    *)
      printf './%s\n' "$path"
      ;;
  esac
}

ensure_repo_root() {
  [[ -f Cargo.toml ]] || die "run this script from the panopose-rust repository root."
  [[ -f README.md ]] || die "README.md was not found."
  [[ -f docs/HOW-TO-skyseg-ncnn.md ]] || die "docs/HOW-TO-skyseg-ncnn.md was not found."
}

ensure_linux() {
  local os

  os="$(uname -s 2>/dev/null || printf unknown)"
  [[ "$os" == "Linux" ]] || die "this quickstart currently supports Linux only; detected $os."
}

ensure_base_requirements() {
  need_command git "Git"
  need_command cmake "CMake"
  need_command nproc "nproc"
}

print_system_report() {
  info "System report"
  info "  OS/arch:   $(uname -s 2>/dev/null || printf unknown) $(uname -m 2>/dev/null || printf unknown)"
  info "  git:       $(optional_command_status git)"
  info "  cmake:     $(optional_command_status cmake)"
  info "  nproc:     $(optional_command_status nproc)"
  info "  c++:       $(optional_command_status c++)"
  info "  pkg-config: $(optional_command_status pkg-config)"
  info "  existing skyseg-ncnn: $(command -v skyseg-ncnn 2>/dev/null || printf missing)"
  info
}

prepare_build_dir() {
  if ((CLEAN)) && [[ -e "$BUILD_DIR" ]]; then
    if confirm "Remove existing build directory '$BUILD_DIR'?"; then
      rm -rf "$BUILD_DIR"
    else
      die "--clean was requested but build directory removal was declined."
    fi
  fi

  mkdir -p "$BUILD_DIR"
  BUILD_DIR="$(abs_path "$BUILD_DIR")"
  INSTALL_PREFIX="$(abs_path "$INSTALL_PREFIX")"

  info "Build directory:   $BUILD_DIR"
  if ((NO_INSTALL)); then
    info "Install prefix:    none; build only"
  else
    info "Install prefix:    $INSTALL_PREFIX"
  fi
  info
}

confirm_start() {
  local display_build_dir

  display_build_dir="$(display_path "$BUILD_DIR")"

  info "This script builds the optional external sky removal tool used by PanoPose."
  info
  info "It will:"
  info "  1. use build workspace: $display_build_dir/"
  info "  2. clone or reuse Tencent/ncnn under $display_build_dir/ncnn"
  info "  3. clone or reuse knyipab/skyseg-ncnn under $display_build_dir/skyseg-ncnn"
  info "  4. build ncnn locally under $display_build_dir/ncnn/build/install"
  info "  5. try to build skyseg-ncnn without compatibility patches first"
  info "  6. ask before applying the known $COMPATIBILITY_DATE compatibility patches if needed"
  if ((NO_INSTALL)); then
    info "  7. leave skyseg-ncnn built but not installed"
  else
    info "  7. install skyseg-ncnn under $INSTALL_PREFIX"
  fi
  info
  info "The external repositories are not part of PanoPose and are ignored by Git."
  info
  if ! confirm "Continue?"; then
    info "Cancelled."
    exit 1
  fi
}

clone_or_reuse() {
  local name="$1"
  local url="$2"
  local recursive="${3:-0}"
  local path="$BUILD_DIR/$name"
  local clone_args=()

  if [[ -d "$path/.git" ]]; then
    info "Using existing $name checkout: $path"
    if ((recursive)); then
      git -C "$path" submodule update --init --recursive
    fi
    return 0
  fi

  if [[ -e "$path" ]]; then
    die "$path exists but is not a Git checkout."
  fi

  info "Cloning $name"
  if ((recursive)); then
    clone_args+=(--recursive)
  fi
  git clone "${clone_args[@]}" "$url" "$path"
}

build_ncnn() {
  local source_dir="$BUILD_DIR/ncnn"
  local build_dir="$source_dir/build"
  local install_dir="$build_dir/install"

  info
  info "Configuring ncnn"
  mkdir -p "$build_dir"
  cmake -S "$source_dir" -B "$build_dir" \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_INSTALL_PREFIX="$install_dir"

  info "Building ncnn"
  cmake --build "$build_dir" -j"$(nproc)"

  info "Installing ncnn into local build directory"
  cmake --install "$build_dir"
}

configure_skyseg() {
  local source_dir="$BUILD_DIR/skyseg-ncnn"
  local build_dir="$source_dir/build"
  local ncnn_dir="$BUILD_DIR/ncnn/build/install/lib/cmake/ncnn"

  mkdir -p "$build_dir"
  cmake -S "$source_dir" -B "$build_dir" \
    -DCMAKE_BUILD_TYPE=Release \
    -Dncnn_DIR="$ncnn_dir" \
    -DCMAKE_INSTALL_PREFIX="$INSTALL_PREFIX"
}

build_skyseg() {
  cmake --build "$BUILD_DIR/skyseg-ncnn/build" -j"$(nproc)"
}

try_build_skyseg_unpatched() {
  local log="$BUILD_DIR/skyseg-ncnn-unpatched-build.log"

  info
  info "Trying skyseg-ncnn build without compatibility patches"
  if ! configure_skyseg >"$log" 2>&1; then
    if grep -qi opencv "$log"; then
      die "skyseg-ncnn configure failed while looking for OpenCV. Install OpenCV development packages and retry. Build log: $log"
    fi
    warn "skyseg-ncnn configure failed before compilation."
    warn "Build log: $log"
    return 1
  fi

  if build_skyseg >>"$log" 2>&1; then
    info "skyseg-ncnn built without compatibility patches."
    return 0
  fi

  warn "skyseg-ncnn did not build cleanly without compatibility patches."
  warn "Build log: $log"
  return 1
}

apply_text_patch() {
  local file="$1"
  local before="$2"
  local after="$3"
  local sed_expr="$4"

  if grep -Fq "$after" "$file"; then
    info "Patch already present: $after"
    return 0
  fi
  if grep -Fq "$before" "$file"; then
    sed -i "$sed_expr" "$file"
    info "Applied patch: $before -> $after"
    return 0
  fi

  warn "Patch text was not found and replacement is not present: $before"
  return 1
}

apply_compatibility_patches() {
  local source_file="$BUILD_DIR/skyseg-ncnn/skyseg-ncnn.cpp.in"
  local failed=0

  [[ -f "$source_file" ]] || die "$source_file was not found."

  info
  info "Applying skyseg-ncnn compatibility patches"
  info "These patches are known-good as of $COMPATIBILITY_DATE."

  apply_text_patch "$source_file" '"ncnn/benchmark.h"' '"benchmark.h"' \
    's|"ncnn/benchmark.h"|"benchmark.h"|' || failed=1
  apply_text_patch "$source_file" '"ncnn/datareader.h"' '"datareader.h"' \
    's|"ncnn/datareader.h"|"datareader.h"|' || failed=1
  apply_text_patch "$source_file" '"ncnn/net.h"' '"net.h"' \
    's|"ncnn/net.h"|"net.h"|' || failed=1
  apply_text_patch "$source_file" 'ex.set_num_threads(4);' 'skynet.opt.num_threads = 4;' \
    's/ex\.set_num_threads(4);/skynet.opt.num_threads = 4;/' || failed=1

  ((failed == 0)) || die "one or more compatibility patches could not be applied."
}

build_skyseg_with_optional_patches() {
  if try_build_skyseg_unpatched; then
    return 0
  fi

  info
  info "The known $COMPATIBILITY_DATE workaround edits include paths and thread-count setup."
  info "They only touch $BUILD_DIR/skyseg-ncnn/skyseg-ncnn.cpp.in."
  if ! confirm "Apply the compatibility patches and retry the build?"; then
    die "skyseg-ncnn build failed without compatibility patches."
  fi

  apply_compatibility_patches

  info
  info "Retrying skyseg-ncnn build after compatibility patches"
  configure_skyseg
  build_skyseg
}

install_skyseg() {
  if ((NO_INSTALL)); then
    info
    info "Skipping install because --no-install was provided."
    info "Built executable should be under:"
    info "  $BUILD_DIR/skyseg-ncnn/build"
    return 0
  fi

  info
  info "Installing skyseg-ncnn"
  cmake --install "$BUILD_DIR/skyseg-ncnn/build"
}

verify_install() {
  local bin_dir="$INSTALL_PREFIX/bin"
  local executable="$bin_dir/skyseg-ncnn"

  info
  info "Verification"
  if command -v skyseg-ncnn >/dev/null 2>&1; then
    info "  PATH contains skyseg-ncnn: $(command -v skyseg-ncnn)"
    return 0
  fi

  if [[ -x "$executable" ]]; then
    warn "skyseg-ncnn was installed but is not visible on PATH."
    warn "Add this to your shell startup file, then restart PanoPose:"
    warn "  export PATH=\"$bin_dir:\$PATH\""
    return 0
  fi

  if ((NO_INSTALL)); then
    warn "skyseg-ncnn was built but not installed."
    return 0
  fi

  warn "skyseg-ncnn was not found on PATH after install."
}

parse_args() {
  while (($#)); do
    case "$1" in
      -y|--yes)
        ASSUME_YES=1
        ;;
      --build-dir)
        shift
        [[ $# -gt 0 ]] || die "--build-dir requires a path."
        BUILD_DIR="$1"
        ;;
      --build-dir=*)
        BUILD_DIR="${1#*=}"
        ;;
      --prefix)
        shift
        [[ $# -gt 0 ]] || die "--prefix requires a path."
        INSTALL_PREFIX="$1"
        ;;
      --prefix=*)
        INSTALL_PREFIX="${1#*=}"
        ;;
      --no-install)
        NO_INSTALL=1
        ;;
      --clean)
        CLEAN=1
        ;;
      -h|--help)
        usage
        exit 0
        ;;
      *)
        die "unknown argument: $1"
        ;;
    esac
    shift
  done
}

main() {
  parse_args "$@"
  confirm_start
  ensure_repo_root
  ensure_linux
  print_system_report
  ensure_base_requirements
  info "Compatibility patches are only applied after an unpatched skyseg-ncnn build fails."
  info
  prepare_build_dir
  clone_or_reuse ncnn "$NCNN_REPO_URL" 1
  clone_or_reuse skyseg-ncnn "$SKYSEG_REPO_URL" 0
  build_ncnn
  build_skyseg_with_optional_patches
  install_skyseg
  verify_install
}

main "$@"
