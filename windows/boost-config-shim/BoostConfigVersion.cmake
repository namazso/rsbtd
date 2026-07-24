# Copyright (C) 2026  namazso <admin@namazso.eu>
# SPDX-License-Identifier: MPL-2.0
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

# Companion to BoostConfig.cmake: reports the version of the header tree
# at $ENV{BOOST_ROOT}. Header-only consumers accept any version; the
# actual pin lives in build.ps1.
set(PACKAGE_VERSION "0.0.0")
set(_rsbtd_boost_root "$ENV{BOOST_ROOT}")
if(_rsbtd_boost_root AND EXISTS "${_rsbtd_boost_root}/boost/version.hpp")
	file(STRINGS "${_rsbtd_boost_root}/boost/version.hpp" _rsbtd_boost_version_line
		REGEX "#define BOOST_VERSION [0-9]+")
	string(REGEX MATCH "[0-9]+" _rsbtd_boost_version "${_rsbtd_boost_version_line}")
	math(EXPR _rsbtd_major "${_rsbtd_boost_version} / 100000")
	math(EXPR _rsbtd_minor "${_rsbtd_boost_version} / 100 % 1000")
	math(EXPR _rsbtd_patch "${_rsbtd_boost_version} % 100")
	set(PACKAGE_VERSION "${_rsbtd_major}.${_rsbtd_minor}.${_rsbtd_patch}")
endif()
set(PACKAGE_VERSION_COMPATIBLE TRUE)
