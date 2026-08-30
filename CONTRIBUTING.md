# Contributing

Thanks for considering a contribution to PanoPose.

## Development Setup

Install Rust, Node.js, npm, and the Linux dependencies required by Tauri. Then run:

```sh
npm ci --prefix frontend
cargo test --workspace
npm run build --prefix frontend
```

For interactive development:

```sh
npm run dev --prefix frontend
cargo tauri dev
```

`Save As` metadata writing requires `exiftool` on `PATH`.

## Pull Requests

- Keep changes focused and explain user-visible behavior.
- Add or update tests when changing core math, export behavior, metadata handling, sky masking, or versioning.
- Run `cargo fmt`, `cargo test --workspace`, and `npm run build --prefix frontend` before opening a PR.
- Do not commit generated build output, local packages, `target/`, `frontend/dist/`, or `frontend/node_modules/`.

## Versioning

Odd minor versions are development lines. The executable derives the patch number from the Git commit count. Even minor versions are release lines and should only be changed by an explicit version bump.

## Licensing

Unless stated otherwise, contributions are accepted under `MIT OR Apache-2.0`, matching the project license.
