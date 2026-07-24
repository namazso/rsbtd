# Copyright (C) 2026  namazso <admin@namazso.eu>
# SPDX-License-Identifier: MPL-2.0
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

# Toolchain file for the arm64 cross build (build.ps1 -Arch arm64 on an
# x64 host): declaring the target system puts CMake in cross-compiling
# mode (no try_run of arm64 binaries on the x64 host), and the arm64 VS
# dev shell supplies the matching LIB/INCLUDE. All the LTO/CRT setup is
# shared with the native build.
set(CMAKE_SYSTEM_NAME Windows)
set(CMAKE_SYSTEM_PROCESSOR ARM64)
# clang-cl targets its host by default. A --target in CFLAGS retargets
# the actual compilations but never reaches CMake's compiler
# identification, which would then record an x64 toolchain and pass
# /machine:x64 to the linker; the compiler-target variable covers both.
set(CMAKE_C_COMPILER_TARGET arm64-pc-windows-msvc)
set(CMAKE_CXX_COMPILER_TARGET arm64-pc-windows-msvc)
include(${CMAKE_CURRENT_LIST_DIR}/lto-toolchain.cmake)
