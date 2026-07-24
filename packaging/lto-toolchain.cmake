# Copyright (C) 2026  namazso <admin@namazso.eu>
# SPDX-License-Identifier: MPL-2.0
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

# Injected via the CMAKE_TOOLCHAIN_FILE environment variable (honored by
# CMake >= 3.21) into the CMake builds that libctorrent-sys's build.rs
# drives: the vendored libtorrent and the libctorrent shim.
#
# Under -flto the objects in their static archives are LLVM bitcode, so
# the archives must be created and indexed with the llvm-* binutils; GNU
# ar/ranlib would write an archive whose symbol index misses every
# bitcode member.
#
# Compilers and -flto flags are not set here: they arrive through the
# CC/CXX/CFLAGS/CXXFLAGS environment exported in packaging/rsbtd.spec's
# %build, which build.rs's cmake crate forwards to both CMake projects.
find_program(CMAKE_AR llvm-ar REQUIRED)
find_program(CMAKE_RANLIB llvm-ranlib REQUIRED)
find_program(CMAKE_NM llvm-nm REQUIRED)
