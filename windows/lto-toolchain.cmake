# Copyright (C) 2026  namazso <admin@namazso.eu>
# SPDX-License-Identifier: MPL-2.0
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

# Toolchain file for the canonical Windows build (injected by build.ps1
# via the CMAKE_TOOLCHAIN_FILE environment variable, reaching both CMake
# projects through libctorrent-sys's cmake crate).
#
# Static libraries full of LLVM bitcode need the LLVM archiver, and the
# CRT selection must match rustc's `-Ctarget-feature=+crt-static` (/MT):
# both CMake projects set CMP0091 to NEW, which would otherwise default
# to the DLL runtime. The compilers and -flto=full itself come from the
# CC/CXX/CFLAGS/CXXFLAGS environment build.ps1 exports.
find_program(CMAKE_AR llvm-lib REQUIRED)
set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreaded" CACHE STRING "")
