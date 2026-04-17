# Rust on Windows (LiminalBD)

Use the **MSVC** Rust toolchain on Windows for reliable linking.

## Prerequisites

1. Install [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with **Desktop development with C++** (MSVC + Windows SDK).
2. Install the MSVC toolchain:

```powershell
rustup toolchain install stable-x86_64-pc-windows-msvc
```

## Recommended: MSVC only in this repo (GNU stays default)

If other projects should keep **GNU** as `rustup default`, use a **directory override** for `LiminalBD` only.

From the repository root:

```powershell
.\scripts\windows-msvc-override.ps1
```

Or manually:

```powershell
cd <path-to-LiminalBD>
rustup override set stable-x86_64-pc-windows-msvc
rustc -vV
```

Inside this directory, `rustc -vV` should show `host: x86_64-pc-windows-msvc`.

To remove the override later:

```powershell
rustup override unset
```

## Alternative: MSVC as global default

```powershell
rustup default stable-x86_64-pc-windows-msvc
```

GNU/MinGW setups frequently break with missing `libgcc_eh` / `libgcc` when mixed with LLVM-based toolchains.
