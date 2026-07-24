# Copyright (C) 2026  namazso <admin@namazso.eu>
# SPDX-License-Identifier: MPL-2.0
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

cmake_minimum_required(VERSION 3.25)
project(ctorrent CXX)

# C++20, specifically for its defined signed left-shift into the sign bit
# (P0907): libtorrent's header-inline peer_class_type_filter operations
# compute `1 << peer_class`, which for the valid class 31 is UB under
# C++17. The shift runs in whichever TU instantiates the inline function,
# so building the shim as C++20 makes it well-defined for any libtorrent,
# vendored or system (the vendored tree is a pinned upstream submodule and
# cannot carry a local patch).
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_POSITION_INDEPENDENT_CODE ON)

option(CTORRENT_BUILD_TESTS "Build libctorrent internal tests" OFF)

include("${CMAKE_CURRENT_SOURCE_DIR}/cmake/FindLibtorrent.cmake")
include("${CMAKE_CURRENT_SOURCE_DIR}/cmake/AbiProbe.cmake")

ctorrent_abi_probe()

add_library(ctorrent STATIC
	src/abi_asserts.cpp
	src/add_torrent_params.cpp
	src/alerts.cpp
	src/create_torrent.cpp
	src/error.cpp
	src/fingerprint.cpp
	src/ip_filter.cpp
	src/peer_class.cpp
	src/peer_info.cpp
	src/session.cpp
	src/settings.cpp
	src/stats.cpp
	src/torrent_handle.cpp
	src/torrent_info.cpp
	src/torrent_status.cpp
	src/version.cpp
)

target_include_directories(ctorrent PUBLIC
	$<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
	$<BUILD_INTERFACE:${CMAKE_CURRENT_BINARY_DIR}/generated/include>
	$<INSTALL_INTERFACE:include>
)
target_link_libraries(ctorrent PRIVATE ctorrent::libtorrent)

if(CMAKE_CXX_COMPILER_ID MATCHES "GNU|Clang")
	target_compile_options(ctorrent PRIVATE -Wall -Wextra)
endif()

if(WIN32)
	# libtorrent targets Windows 10 via a non-exported add_definitions();
	# our translation units include its headers and need the same floor.
	target_compile_definitions(ctorrent PRIVATE _WIN32_WINNT=0x0A00)
endif()
if(MSVC)
	target_compile_options(ctorrent PRIVATE /bigobj)
endif()

install(TARGETS ctorrent ARCHIVE DESTINATION lib)
install(DIRECTORY include/ DESTINATION include)
install(FILES "${CMAKE_CURRENT_BINARY_DIR}/generated/include/ctorrent/ct_abi_config.h"
	DESTINATION include/ctorrent)
install(FILES "${CMAKE_BINARY_DIR}/ctorrent-link.txt" DESTINATION lib)

if(CTORRENT_BUILD_TESTS)
	enable_testing()
	add_subdirectory(tests)
endif()
