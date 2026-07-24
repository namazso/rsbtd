# Copyright (C) 2026  namazso <admin@namazso.eu>
# SPDX-License-Identifier: MPL-2.0
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

add_executable(ct_test_internal test_internal.cpp)
target_link_libraries(ct_test_internal PRIVATE ctorrent ctorrent::libtorrent)
target_include_directories(ct_test_internal PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}/../src")
add_test(NAME ct_test_internal COMMAND ct_test_internal)
