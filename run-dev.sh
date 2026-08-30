#!/usr/bin/env bash
set -Eeuo pipefail

readonly FRONTEND_DIR="frontend"
readonly VITE_URL="http://127.0.0.1:5173"
readonly DEFAULT_LOG_DIR=".panopose-dev/logs"

NO_INSTALL=0
HEADLESS=0
LOG_DIR=""
LOG_FILE=""
VITE_PID=""
APP_PID=""
STARTED_VITE=0

usage() {
  cat <<'EOF'
Usage: ./run-dev.sh [options]

Start the PanoPose frontend development server and launch the Rust/Tauri app.

Options:
  --no-install       Do not run npm ci when frontend/node_modules is missing.
  --log-dir <path>   Write Vite and app output to timestamped log files.
  --headless         Launch the Rust app through xvfb-run -a.
  -h, --help         Show this help.
EOF
}

die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

info() {
  printf '%s\n' "$*"
}

need_command() {
  local command_name="$1"
  local description="$2"

  if ! command -v "$command_name" >/dev/null 2>&1; then
    die "$description is required but '$command_name' was not found in PATH."
  fi
}

optional_command_status() {
  local command_name="$1"
  if command -v "$command_name" >/dev/null 2>&1; then
    printf 'found'
  else
    printf 'missing'
  fi
}

ensure_repo_root() {
  [[ -f Cargo.toml ]] || die "run this script from the panopose-rust repository root."
  [[ -f "$FRONTEND_DIR/package.json" ]] || die "$FRONTEND_DIR/package.json was not found."
  [[ -f src-tauri/tauri.conf.json ]] || die "src-tauri/tauri.conf.json was not found."
}

ensure_requirements() {
  need_command cargo "Rust Cargo"
  need_command rustc "rustc"
  need_command node "Node.js"
  need_command npm "npm"
  need_command curl "curl"
  need_command setsid "setsid"

  if ((HEADLESS)); then
    need_command xvfb-run "xvfb-run"
  fi
}

ensure_node_dependencies() {
  if [[ -d "$FRONTEND_DIR/node_modules" ]]; then
    return 0
  fi

  if ((NO_INSTALL)); then
    die "$FRONTEND_DIR/node_modules is missing and --no-install was provided."
  fi

  if [[ -f "$FRONTEND_DIR/package-lock.json" ]]; then
    info "Installing frontend dependencies with npm ci"
    npm ci --prefix "$FRONTEND_DIR"
  else
    info "Installing frontend dependencies with npm install"
    npm install --prefix "$FRONTEND_DIR"
  fi
}

print_system_report() {
  info "System report"
  info "  cargo:    $(optional_command_status cargo)"
  info "  rustc:    $(optional_command_status rustc)"
  info "  node:     $(optional_command_status node)"
  info "  npm:      $(optional_command_status npm)"
  info "  curl:     $(optional_command_status curl)"
  info "  setsid:   $(optional_command_status setsid)"
  info "  xvfb-run: $(optional_command_status xvfb-run)"
  info
}

print_launch_report() {
  info "Launching PanoPose dev mode"
  info "  Vite URL:     $VITE_URL"
  info "  frontend dir: $FRONTEND_DIR"
  info "  app command:  cargo run -p panopose --bin panopose"

  if [[ -n "$LOG_DIR" ]]; then
    mkdir -p "$LOG_DIR"
    LOG_FILE="$LOG_DIR/panopose-dev-$(date +%Y%m%d-%H%M%S)"
    info "  Vite log:     $LOG_FILE-vite.log"
    info "  app log:      $LOG_FILE-app.log"
  fi

  if ((HEADLESS)); then
    info "  display:      xvfb-run -a"
  fi
  info
}

vite_is_ready() {
  curl --fail --silent --output /dev/null "$VITE_URL"
}

start_vite() {
  if vite_is_ready; then
    info "Reusing existing Vite server at $VITE_URL"
    return 0
  fi

  info "Starting Vite server"
  if [[ -n "$LOG_FILE" ]]; then
    setsid npm run dev --prefix "$FRONTEND_DIR" >"$LOG_FILE-vite.log" 2>&1 &
  else
    setsid npm run dev --prefix "$FRONTEND_DIR" &
  fi
  VITE_PID=$!
  STARTED_VITE=1

  for _ in {1..80}; do
    if vite_is_ready; then
      info "Vite is ready at $VITE_URL"
      return 0
    fi
    sleep 0.25
  done

  die "Vite did not become ready at $VITE_URL."
}

run_app() {
  local -a command=(cargo run -p panopose --bin panopose)

  if ((HEADLESS)); then
    command=(xvfb-run -a "${command[@]}")
  fi

  info "Launching Rust/Tauri app"
  if [[ -n "$LOG_FILE" ]]; then
    setsid "${command[@]}" >"$LOG_FILE-app.log" 2>&1 &
  else
    setsid "${command[@]}" &
  fi

  APP_PID=$!
  wait "$APP_PID"
}

cleanup() {
  local status=$?

  if [[ -n "$APP_PID" ]] && kill -0 "$APP_PID" >/dev/null 2>&1; then
    info
    info "Stopping PanoPose app process group: $APP_PID"
    kill -- "-$APP_PID" >/dev/null 2>&1 || kill "$APP_PID" >/dev/null 2>&1 || true
    wait "$APP_PID" >/dev/null 2>&1 || true
  fi

  if ((STARTED_VITE)) && [[ -n "$VITE_PID" ]] && kill -0 "$VITE_PID" >/dev/null 2>&1; then
    info "Stopping Vite process group: $VITE_PID"
    kill -- "-$VITE_PID" >/dev/null 2>&1 || kill "$VITE_PID" >/dev/null 2>&1 || true
    wait "$VITE_PID" >/dev/null 2>&1 || true
  fi

  exit "$status"
}

parse_args() {
  while (($#)); do
    case "$1" in
      --no-install)
        NO_INSTALL=1
        ;;
      --headless)
        HEADLESS=1
        ;;
      --log-dir)
        shift
        [[ $# -gt 0 ]] || die "--log-dir requires a path."
        LOG_DIR="$1"
        ;;
      --log-dir=*)
        LOG_DIR="${1#--log-dir=}"
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

  if ((HEADLESS)) && [[ -z "$LOG_DIR" ]]; then
    LOG_DIR="$DEFAULT_LOG_DIR"
  fi
}

main() {
  parse_args "$@"
  ensure_repo_root
  print_system_report
  ensure_requirements
  ensure_node_dependencies
  print_launch_report

  trap cleanup INT TERM EXIT
  start_vite
  run_app
}

main "$@"
