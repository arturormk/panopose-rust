#!/usr/bin/env bash
set -Eeuo pipefail

readonly FRONTEND_DIR="frontend"
readonly APP_NAME="PanoPose"
readonly APP_ID="dev.panopose.desktop"
readonly CLI_BIN_NAME="panopose-cli"
readonly SIDECAR_DIR="src-tauri/binaries"
readonly APPIMAGE_INSTALL_DIR="$HOME/.local/bin"
readonly DESKTOP_INSTALL_DIR="$HOME/.local/share/applications"

ASSUME_YES=0
NO_INSTALL=0
SKIP_APP_INSTALL=0
BUILT_ARTIFACT=""
PACKAGE_SELECTION=""
NO_BUNDLE=0

usage() {
  cat <<'EOF'
Usage: ./quickstart-panopose.sh [options]

Install local build dependencies, build a release version of PanoPose for the
current platform, and offer to install the resulting desktop package.

Options:
  -y, --yes             Answer yes to interactive prompts.
  --no-install          Do not install missing build tools such as cargo-tauri.
  --skip-app-install    Build only; do not offer to install the app.
  --bundles <list>      Build comma-separated packages, for example deb,rpm.
  --no-bundle           Build only the release executable; do not package it.
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

prompt() {
  local question="$1"
  local default_value="$2"
  local reply

  if ((ASSUME_YES)); then
    printf '%s' "$default_value"
    return 0
  fi

  printf '%s [%s] ' "$question" "$default_value" >&2
  read -r reply
  printf '%s' "${reply:-$default_value}"
}

ensure_repo_root() {
  [[ -f Cargo.toml ]] || die "run this script from the panopose-rust repository root."
  [[ -f "$FRONTEND_DIR/package.json" ]] || die "$FRONTEND_DIR/package.json was not found."
  [[ -f src-tauri/tauri.conf.json ]] || die "src-tauri/tauri.conf.json was not found."
}

ensure_base_requirements() {
  need_command cargo "Rust Cargo"
  need_command rustc "rustc"
  need_command node "Node.js"
  need_command npm "npm"
}

ensure_cargo_tauri() {
  if cargo tauri --version >/dev/null 2>&1; then
    return 0
  fi

  if ((NO_INSTALL)); then
    die "cargo-tauri is missing and --no-install was provided."
  fi

  if confirm "cargo-tauri is missing. Install tauri-cli with cargo install?"; then
    cargo install tauri-cli --locked
  else
    die "cargo-tauri is required to build the desktop release."
  fi
}

ensure_node_dependencies() {
  if [[ -f "$FRONTEND_DIR/package-lock.json" ]]; then
    info "Installing frontend dependencies with npm ci"
    npm ci --prefix "$FRONTEND_DIR"
  else
    info "Installing frontend dependencies with npm install"
    npm install --prefix "$FRONTEND_DIR"
  fi
}

print_system_report() {
  local os arch

  os="$(uname -s 2>/dev/null || printf unknown)"
  arch="$(uname -m 2>/dev/null || printf unknown)"

  info "System report"
  info "  OS/arch:      $os $arch"
  info "  cargo:        $(optional_command_status cargo)"
  info "  rustc:        $(optional_command_status rustc)"
  info "  node:         $(optional_command_status node)"
  info "  npm:          $(optional_command_status npm)"
  info "  cargo-tauri:  $(cargo tauri --version >/dev/null 2>&1 && printf found || printf missing)"
  info "  dpkg:         $(optional_command_status dpkg)"
  info "  rpm:          $(optional_command_status rpm)"
  info "  desktop-file-install: $(optional_command_status desktop-file-install)"
  info
}

build_release() {
  local bundle_args=()

  select_build_packages
  build_cli_utility

  info "Building $APP_NAME release for the current platform"
  if ((NO_BUNDLE)); then
    info "Package selection: none; building release executables only"
    cargo tauri build --ci --no-bundle
  else
    info "Package selection: $PACKAGE_SELECTION"
    bundle_args=(--bundles "$PACKAGE_SELECTION")
    cargo tauri build --ci "${bundle_args[@]}"
  fi
}

rust_host_triple() {
  rustc -vV | awk '/^host: / {print $2}'
}

build_cli_utility() {
  local target_triple source_binary staged_binary

  target_triple="$(rust_host_triple)"
  [[ -n "$target_triple" ]] || die "could not determine Rust host target triple."

  info "Building $CLI_BIN_NAME utility"
  cargo build -p "$CLI_BIN_NAME" --release

  source_binary="target/release/$CLI_BIN_NAME"
  [[ -f "$source_binary" ]] || die "$source_binary was not produced."

  mkdir -p "$SIDECAR_DIR"
  staged_binary="$SIDECAR_DIR/$CLI_BIN_NAME-$target_triple"
  cp "$source_binary" "$staged_binary"
  chmod +x "$staged_binary"
}

default_package_selection() {
  local os

  os="$(uname -s 2>/dev/null || printf unknown)"
  case "$os" in
    Linux)
      if command -v dpkg >/dev/null 2>&1; then
        printf 'deb'
      elif command -v rpm >/dev/null 2>&1; then
        printf 'rpm'
      else
        printf 'appimage'
      fi
      ;;
    *)
      printf 'none'
      ;;
  esac
}

select_build_packages() {
  local os default_selection selection

  if ((NO_BUNDLE)); then
    return 0
  fi

  if [[ -n "$PACKAGE_SELECTION" ]]; then
    return 0
  fi

  os="$(uname -s 2>/dev/null || printf unknown)"
  default_selection="$(default_package_selection)"
  case "$os" in
    Linux)
      info
      info "Choose final packages to build."
      print_linux_package_menu "$default_selection"
      selection="$(prompt "Selection" "$(linux_package_default_choice "$default_selection")")"
      selection="$(linux_package_selection_from_menu_choice "$selection")"
      ;;
    *)
      selection="$(prompt "Packages to build, or none" "$default_selection")"
      ;;
  esac

  normalize_package_selection "$selection"
}

print_linux_package_menu() {
  local default_selection="$1"
  local default_choice

  default_choice="$(linux_package_default_choice "$default_selection")"
  info "  1) deb"
  info "  2) rpm"
  info "  3) appimage"
  info "  4) deb,rpm"
  info "  5) all"
  info "  6) none"
  info "Default: $default_choice) $default_selection"
}

linux_package_default_choice() {
  case "$1" in
    deb) printf '1' ;;
    rpm) printf '2' ;;
    appimage) printf '3' ;;
    deb,rpm|rpm,deb) printf '4' ;;
    deb,rpm,appimage|all) printf '5' ;;
    none|"") printf '6' ;;
    *) printf '%s' "$1" ;;
  esac
}

linux_package_selection_from_menu_choice() {
  local selection="$1"

  selection="${selection,,}"
  selection="${selection// /}"
  case "$selection" in
    1) printf 'deb' ;;
    2) printf 'rpm' ;;
    3) printf 'appimage' ;;
    4) printf 'deb,rpm' ;;
    5) printf 'all' ;;
    6) printf 'none' ;;
    *) printf '%s' "$selection" ;;
  esac
}

normalize_package_selection() {
  local selection normalized

  selection="${1,,}"
  selection="${selection// /}"
  case "$selection" in
    ""|none|no|n)
      NO_BUNDLE=1
      PACKAGE_SELECTION=""
      ;;
    all)
      PACKAGE_SELECTION="deb,rpm,appimage"
      ;;
    *)
      normalized="${selection//,/ }"
      for bundle in $normalized; do
        case "$bundle" in
          deb|rpm|appimage)
            ;;
          *)
            die "unknown package '$bundle'; expected deb, rpm, appimage, all, or none."
            ;;
        esac
      done
      PACKAGE_SELECTION="$selection"
      ;;
  esac
}

find_latest_artifact() {
  local pattern="$1"
  local roots=()

  [[ -d target ]] && roots+=(target)
  [[ -d src-tauri/target ]] && roots+=(src-tauri/target)
  ((${#roots[@]})) || return 0

  find "${roots[@]}" -path "$pattern" -type f -printf '%T@ %p\n' 2>/dev/null \
    | sort -nr \
    | awk 'NR == 1 {print substr($0, index($0, $2))}'
}

bundle_selected() {
  local wanted="$1"
  local bundles

  bundles=",${PACKAGE_SELECTION},"
  [[ "$bundles" == *",$wanted,"* ]]
}

select_install_artifact() {
  local os deb rpm appimage appbundle dmg msi exe

  os="$(uname -s 2>/dev/null || printf unknown)"

  case "$os" in
    Linux)
      deb=""
      rpm=""
      appimage=""
      bundle_selected deb && deb="$(find_latest_artifact '*/release/bundle/deb/*.deb')"
      bundle_selected rpm && rpm="$(find_latest_artifact '*/release/bundle/rpm/*.rpm')"
      bundle_selected appimage && appimage="$(find_latest_artifact '*/release/bundle/appimage/*.AppImage')"
      if command -v dpkg >/dev/null 2>&1 && [[ -n "$deb" ]]; then
        BUILT_ARTIFACT="$deb"
      elif command -v rpm >/dev/null 2>&1 && [[ -n "$rpm" ]]; then
        BUILT_ARTIFACT="$rpm"
      elif [[ -n "$appimage" ]]; then
        BUILT_ARTIFACT="$appimage"
      fi
      ;;
    Darwin)
      dmg="$(find_latest_artifact '*/release/bundle/dmg/*.dmg')"
      appbundle="$(find_latest_artifact '*/release/bundle/macos/*.app')"
      BUILT_ARTIFACT="${dmg:-$appbundle}"
      ;;
    MINGW*|MSYS*|CYGWIN*)
      msi="$(find_latest_artifact '*/release/bundle/msi/*.msi')"
      exe="$(find_latest_artifact '*/release/bundle/nsis/*.exe')"
      BUILT_ARTIFACT="${msi:-$exe}"
      ;;
  esac
}

list_artifacts() {
  info
  info "Built artifacts"
  print_package_artifacts "  "
}

find_release_executable() {
  local roots=()

  [[ -f target/release/panopose ]] && roots+=(target/release/panopose)
  [[ -f src-tauri/target/release/panopose ]] && roots+=(src-tauri/target/release/panopose)
  ((${#roots[@]})) || return 0

  printf '%s\n' "${roots[@]}" | head -n 1
}

find_cli_executable() {
  local roots=()

  [[ -f target/release/$CLI_BIN_NAME ]] && roots+=(target/release/$CLI_BIN_NAME)
  [[ -f src-tauri/target/release/$CLI_BIN_NAME ]] && roots+=(src-tauri/target/release/$CLI_BIN_NAME)
  ((${#roots[@]})) || return 0

  printf '%s\n' "${roots[@]}" | head -n 1
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

print_package_artifacts() {
  local prefix="$1"
  local roots=()
  local bundles bundle pattern found=0

  if ((NO_BUNDLE)); then
    printf '%snone requested\n' "$prefix"
    return 0
  fi

  [[ -d target/release/bundle ]] && roots+=(target/release/bundle)
  [[ -d src-tauri/target/release/bundle ]] && roots+=(src-tauri/target/release/bundle)
  if ((${#roots[@]} == 0)); then
    printf '%snone found\n' "$prefix"
    return 0
  fi

  bundles="${PACKAGE_SELECTION//,/ }"
  for bundle in $bundles; do
    case "$bundle" in
      deb) pattern='*.deb' ;;
      rpm) pattern='*.rpm' ;;
      appimage) pattern='*.AppImage' ;;
      *) continue ;;
    esac
    while IFS= read -r artifact; do
      printf '%s%s\n' "$prefix" "$(abs_path "$artifact")"
      found=1
    done < <(find "${roots[@]}" -type f -name "$pattern" 2>/dev/null | sort)
  done

  if ((found == 0)); then
    printf '%snone found\n' "$prefix"
  fi
}

print_build_outputs() {
  local executable cli_executable

  info
  info "Build outputs"
  executable="$(find_release_executable)"
  if [[ -n "$executable" ]]; then
    info "  App executable:"
    info "    $(abs_path "$executable")"
  else
    warn "release app executable was not found."
  fi

  cli_executable="$(find_cli_executable)"
  if [[ -n "$cli_executable" ]]; then
    info "  CLI executable:"
    info "    $(abs_path "$cli_executable")"
  else
    warn "$CLI_BIN_NAME release executable was not found."
  fi

  info "  Final packages:"
  print_package_artifacts "    "
}

install_artifact() {
  local artifact="$1"

  case "$artifact" in
    *.deb)
      need_command sudo "sudo"
      sudo dpkg -i "$artifact"
      ;;
    *.rpm)
      need_command sudo "sudo"
      sudo rpm -Uvh "$artifact"
      ;;
    *.AppImage)
      install_appimage "$artifact"
      ;;
    *.dmg|*.app|*.msi|*.exe)
      warn "automatic install is not implemented for this artifact:"
      warn "$artifact"
      ;;
    *)
      warn "no install method is known for this artifact:"
      warn "$artifact"
      ;;
  esac
}

install_appimage() {
  local artifact="$1"
  local target="$APPIMAGE_INSTALL_DIR/panopose.AppImage"
  local desktop_file="$DESKTOP_INSTALL_DIR/panopose.desktop"

  mkdir -p "$APPIMAGE_INSTALL_DIR" "$DESKTOP_INSTALL_DIR"
  cp "$artifact" "$target"
  chmod +x "$target"

  cat >"$desktop_file" <<EOF
[Desktop Entry]
Type=Application
Name=$APP_NAME
Exec=$target
Terminal=false
Categories=Graphics;Science;
StartupWMClass=$APP_ID
EOF

  if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$DESKTOP_INSTALL_DIR" >/dev/null 2>&1 || true
  fi

  info "Installed AppImage: $target"
  info "Installed desktop entry: $desktop_file"
}

offer_install() {
  ((SKIP_APP_INSTALL)) && return 0
  if ((NO_BUNDLE)); then
    print_build_outputs
    return 0
  fi

  select_install_artifact
  if [[ -z "$BUILT_ARTIFACT" ]]; then
    warn "no installable artifact was found."
    return 0
  fi

  info
  info "Preferred install artifact:"
  info "  $BUILT_ARTIFACT"
  if confirm "Install $APP_NAME now?"; then
    install_artifact "$BUILT_ARTIFACT"
  else
    info "Skipping app install."
    print_build_outputs
  fi
}

parse_args() {
  while (($#)); do
    case "$1" in
      -y|--yes)
        ASSUME_YES=1
        ;;
      --no-install)
        NO_INSTALL=1
        ;;
      --skip-app-install)
        SKIP_APP_INSTALL=1
        ;;
      --bundles)
        shift
        [[ $# -gt 0 ]] || die "--bundles requires a comma-separated package list."
        normalize_package_selection "$1"
        ;;
      --bundles=*)
        normalize_package_selection "${1#*=}"
        ;;
      --no-bundle)
        NO_BUNDLE=1
        PACKAGE_SELECTION=""
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
  ensure_repo_root
  print_system_report
  ensure_base_requirements
  ensure_cargo_tauri
  ensure_node_dependencies
  build_release
  list_artifacts
  if ((SKIP_APP_INSTALL)); then
    print_build_outputs
  fi
  offer_install
}

main "$@"
