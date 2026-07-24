# Copyright (C) 2026  namazso <admin@namazso.eu>
# SPDX-License-Identifier: MPL-2.0
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

# Minimal config-mode Boost package for a plain header tree.
#
# CMake 4 removed the FindBoost module, and a Boost release archive is
# just headers (libtorrent 2.1 uses Boost header-only) with no
# BoostConfig.cmake of its own. build.ps1 points CMAKE_PREFIX_PATH at
# this directory and BOOST_ROOT at the extracted headers; this file
# provides the Boost::headers target and the version variables
# libtorrent's CMakeLists checks. On CMake < 4 the FindBoost module wins
# (module mode precedes config mode) and behaves the same.

if(TARGET Boost::headers)
	set(Boost_FOUND TRUE)
	return()
endif()

set(_rsbtd_boost_root "$ENV{BOOST_ROOT}")
if(NOT _rsbtd_boost_root OR NOT EXISTS "${_rsbtd_boost_root}/boost/version.hpp")
	set(Boost_FOUND FALSE)
	set(Boost_NOT_FOUND_MESSAGE
		"BOOST_ROOT is not set or does not point at a Boost header tree \
(expected \$ENV{BOOST_ROOT}/boost/version.hpp)")
	return()
endif()

file(STRINGS "${_rsbtd_boost_root}/boost/version.hpp" _rsbtd_boost_version_line
	REGEX "#define BOOST_VERSION [0-9]+")
string(REGEX MATCH "[0-9]+" _rsbtd_boost_version "${_rsbtd_boost_version_line}")
math(EXPR Boost_MAJOR_VERSION "${_rsbtd_boost_version} / 100000")
math(EXPR Boost_MINOR_VERSION "${_rsbtd_boost_version} / 100 % 1000")
math(EXPR Boost_SUBMINOR_VERSION "${_rsbtd_boost_version} % 100")
set(Boost_VERSION_STRING
	"${Boost_MAJOR_VERSION}.${Boost_MINOR_VERSION}.${Boost_SUBMINOR_VERSION}")
set(Boost_VERSION "${Boost_VERSION_STRING}")
set(Boost_INCLUDE_DIRS "${_rsbtd_boost_root}")

add_library(Boost::headers INTERFACE IMPORTED)
set_target_properties(Boost::headers PROPERTIES
	INTERFACE_INCLUDE_DIRECTORIES "${_rsbtd_boost_root}")
# Legacy alias some projects still link against.
add_library(Boost::boost INTERFACE IMPORTED)
set_target_properties(Boost::boost PROPERTIES
	INTERFACE_LINK_LIBRARIES Boost::headers)

set(Boost_FOUND TRUE)
