# HOW-TO: Build and Install skyseg-ncnn for PanoPose

These notes describe one known-good Linux build path for `skyseg-ncnn`, the external sky segmentation executable used by PanoPose.

Important date scope: these instructions reflect the state of `skyseg-ncnn` and `ncnn` as tested on August 31, 2026. The compatibility edits below are workarounds for the versions available at that time. Re-check the upstream projects before assuming these patches are still needed.

References:

- `skyseg-ncnn`: <https://github.com/knyipab/skyseg-ncnn>
- `ncnn`: <https://github.com/Tencent/ncnn>
- Original segmentation project referenced by `skyseg-ncnn`: <https://github.com/xiongzhu666/Sky-Segmentation-and-Post-processing>

## Quickstart Script

For normal PanoPose development on Linux, prefer the repository helper:

```sh
./quickstart-skyseg-ncnn.sh
```

The script:

- explains the clone/build/install steps and asks for permission before starting;
- builds under ignored `./thirdparty/ncnn` and `./thirdparty/skyseg-ncnn` directories;
- builds and installs `ncnn` locally inside that workspace;
- tries to build `skyseg-ncnn` without source edits first;
- explains and asks before applying the dated compatibility patches if the unpatched build fails;
- installs `skyseg-ncnn` under `$HOME/.local` by default;
- prints a `PATH` hint if `$HOME/.local/bin` is not visible to the current shell.

Useful options:

```sh
./quickstart-skyseg-ncnn.sh --yes
./quickstart-skyseg-ncnn.sh --clean
./quickstart-skyseg-ncnn.sh --no-install
./quickstart-skyseg-ncnn.sh --build-dir /tmp/panopose-thirdparty
./quickstart-skyseg-ncnn.sh --prefix /opt/skyseg
```

The manual commands below are still useful when debugging a failed build or adapting the process to changed upstream versions.

## What PanoPose Expects

PanoPose does not bundle `skyseg-ncnn`. It looks for an executable named `skyseg-ncnn` on `PATH`.

When the executable is found, PanoPose shows the `Remove Sky` toggle. During preview/export it renders the corrected panorama and runs:

```sh
skyseg-ncnn <corrected-pano-image> <mask.jpg>
```

The mask is interpreted as:

- black: ground, kept opaque;
- white: sky, made transparent;
- gray: partial transparency at mixed edges.

## Prerequisites

Install the normal C++ build tools, CMake, Git, and OpenCV development files for your distribution.

On Debian/Ubuntu-style systems, the package set is typically:

```sh
sudo apt install build-essential cmake git libopencv-dev
```

Other distributions use different package names, but the important requirements are:

- a C++ compiler;
- CMake;
- Git;
- OpenCV headers and libraries;
- enough disk space and time to build `ncnn` from source.

## Build Directory

The commands below assume an arbitrary working directory named `skyseg`. It can be anywhere, for example under `~/src`, `/tmp`, or another development directory.

```sh
mkdir skyseg
cd skyseg
```

After this point, all paths are relative to that `skyseg` directory.

## Build and Install ncnn Locally

Clone `ncnn` recursively. The recursive clone matters because `ncnn` uses submodules.

```sh
git clone --recursive https://github.com/Tencent/ncnn.git
```

Configure a release build and install it into `ncnn/build/install` inside the working directory:

```sh
mkdir -p ncnn/build
cmake -S ncnn -B ncnn/build \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_INSTALL_PREFIX="$PWD/ncnn/build/install"
```

Build and install:

```sh
cmake --build ncnn/build -j"$(nproc)"
cmake --install ncnn/build
```

This keeps the `ncnn` install private to this build directory, avoiding changes to system-wide libraries.

## Clone skyseg-ncnn

Clone the wrapper project:

```sh
git clone https://github.com/knyipab/skyseg-ncnn.git
```

The upstream project provides a CMake build wrapper and includes the model files used by the executable.

## Apply Compatibility Patches

As of August 31, 2026, the current `skyseg-ncnn` source did not compile cleanly against the current `ncnn` headers without a few small edits.

The first three edits update include paths. The installed `ncnn` headers expose these files directly, not under an `ncnn/` prefix in this build layout:

```sh
sed -i 's|"ncnn/benchmark.h"|"benchmark.h"|' skyseg-ncnn/skyseg-ncnn.cpp.in
sed -i 's|"ncnn/datareader.h"|"datareader.h"|' skyseg-ncnn/skyseg-ncnn.cpp.in
sed -i 's|"ncnn/net.h"|"net.h"|' skyseg-ncnn/skyseg-ncnn.cpp.in
```

The final edit updates thread-count configuration. The older code sets the extractor thread count directly; the tested `ncnn` version expects this option on the network options instead:

```sh
sed -i 's/ex\.set_num_threads(4);/skynet.opt.num_threads = 4;/' skyseg-ncnn/skyseg-ncnn.cpp.in
```

If a future `skyseg-ncnn` release builds without these edits, prefer the upstream source as-is.

## Build skyseg-ncnn

Configure `skyseg-ncnn` and point CMake at the local `ncnn` package config generated above:

```sh
mkdir -p skyseg-ncnn/build
cmake -S skyseg-ncnn -B skyseg-ncnn/build \
  -DCMAKE_BUILD_TYPE=Release \
  -Dncnn_DIR="$PWD/ncnn/build/install/lib/cmake/ncnn"
```

Build it:

```sh
cmake --build skyseg-ncnn/build -j"$(nproc)"
```

## Install skyseg-ncnn

Install the executable. The default install prefix used by the project may require administrator privileges:

```sh
sudo cmake --install skyseg-ncnn/build
```

If you do not want a system-wide install, configure `skyseg-ncnn` with your own prefix instead:

```sh
cmake -S skyseg-ncnn -B skyseg-ncnn/build \
  -DCMAKE_BUILD_TYPE=Release \
  -Dncnn_DIR="$PWD/ncnn/build/install/lib/cmake/ncnn" \
  -DCMAKE_INSTALL_PREFIX="$HOME/.local"
cmake --build skyseg-ncnn/build -j"$(nproc)"
cmake --install skyseg-ncnn/build
```

For a user-local install, make sure the install location is on `PATH`. For example, if the executable is installed to `$HOME/.local/bin`, your shell startup file should include:

```sh
export PATH="$HOME/.local/bin:$PATH"
```

Restart PanoPose after changing `PATH`.

## Verify the Install

Check that the executable can be found:

```sh
command -v skyseg-ncnn
```

Then run it on any image and confirm it writes a mask:

```sh
skyseg-ncnn input.jpg mask.jpg
ls -lh mask.jpg
```

Once `skyseg-ncnn` is visible on `PATH`, PanoPose should show the `Remove Sky` toggle on startup.

## Troubleshooting

If CMake cannot find `ncnn`, check that this path exists:

```sh
ls ncnn/build/install/lib/cmake/ncnn
```

Then make sure the `-Dncnn_DIR=...` value points to that exact directory.

If compilation fails on missing `ncnn/...` headers, the include-path compatibility edits above were probably not applied, or the upstream source has changed enough that the edits no longer match.

If compilation fails around `set_num_threads`, inspect `skyseg-ncnn/skyseg-ncnn.cpp.in` and the installed `ncnn` API. The workaround shown here was valid on August 31, 2026, but may need to be removed or adjusted for future `ncnn` releases.

If PanoPose does not show `Remove Sky`, run `command -v skyseg-ncnn` in the same environment used to launch PanoPose. Desktop launchers sometimes have a different `PATH` from interactive terminals.
