# Copyright (C) 2026  namazso <admin@namazso.eu>
# SPDX-License-Identifier: MPL-2.0
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

# Resolve libtorrent-rasterbar (>= 2.1) into an interface target
# `ctorrent::libtorrent` carrying all usage requirements, most importantly
# the TORRENT_ABI_VERSION (and, for shared libraries, TORRENT_LINKING_SHARED)
# interface compile definitions the library was built with. Both discovery
# paths propagate them: the CMake config package as PUBLIC definitions, the
# .pc file in Cflags.
#
# When CTORRENT_LIBTORRENT_PREFIX is set, it is a strict override: libtorrent
# is resolved from exactly that prefix (no default search paths, no
# pkg-config fallback), and a miss is a hard configure error rather than a
# silent fall-through to some system libtorrent.
#
# Also sets:
#   CTORRENT_LT_TARGET - the underlying IMPORTED target; try_compile's
#                        LINK_LIBRARIES only accepts imported targets, so the
#                        ABI probe must use this instead of the alias.
# and writes ${CMAKE_BINARY_DIR}/ctorrent-link.txt (LINKDIR=/LIB=/TYPE=
# lines, plus one EXTRA= line per additional resolved link item) so the
# Rust build can emit the matching link flags (static archive vs shared
# library). The EXTRA items matter for static libtorrent builds, whose
# archive does not carry its dependencies (WebTorrent's
# datachannel/usrsctp, Apple frameworks, OpenSSL).

set(CTORRENT_LIBTORRENT_PREFIX "" CACHE PATH
  "Resolve libtorrent from exactly this install prefix (no other locations)")

add_library(ctorrent_libtorrent INTERFACE)
add_library(ctorrent::libtorrent ALIAS ctorrent_libtorrent)

# Collects a target's additional link items as strings the Rust build
# can act on: imported targets become $<TARGET_FILE:...> paths (one
# level of interface indirection is followed), literal flags and paths
# pass through, and anything still holding an unresolvable generator
# expression is skipped with a notice.
function(_ctorrent_collect_link_items tgt out_var)
  get_target_property(_iface "${tgt}" INTERFACE_LINK_LIBRARIES)
  if(NOT _iface)
    set("${out_var}" "" PARENT_SCOPE)
    return()
  endif()
  set(_items "")
  foreach(_raw IN LISTS _iface)
    # Exported static targets wrap private dependencies in $<LINK_ONLY:...>.
    string(REGEX REPLACE "^\\$<LINK_ONLY:(.+)>$" "\\1" _item "${_raw}")
    if(TARGET "${_item}")
      get_target_property(_type "${_item}" TYPE)
      if(_type STREQUAL "INTERFACE_LIBRARY")
        get_target_property(_nested "${_item}" INTERFACE_LINK_LIBRARIES)
        if(_nested)
          foreach(_n IN LISTS _nested)
            if(TARGET "${_n}")
              list(APPEND _items "$<TARGET_FILE:${_n}>")
            elseif(NOT _n MATCHES "\\$<")
              list(APPEND _items "${_n}")
            endif()
          endforeach()
        endif()
      else()
        list(APPEND _items "$<TARGET_FILE:${_item}>")
      endif()
    elseif(NOT _item MATCHES "\\$<")
      list(APPEND _items "${_item}")
    else()
      message(STATUS "ctorrent: skipping unresolvable link item ${_item}")
    endif()
  endforeach()
  set("${out_var}" "${_items}" PARENT_SCOPE)
endfunction()

# 2.1 is the real floor (abi_asserts.cpp also pins LIBTORRENT_VERSION_NUM >=
# 20100); requiring it here fails at configure time instead of mid-build.
if(CTORRENT_LIBTORRENT_PREFIX)
  find_package(LibtorrentRasterbar 2.1 CONFIG QUIET
    PATHS "${CTORRENT_LIBTORRENT_PREFIX}" NO_DEFAULT_PATH)
  if(NOT TARGET LibtorrentRasterbar::torrent-rasterbar)
    message(FATAL_ERROR "ctorrent: CTORRENT_LIBTORRENT_PREFIX is set "
      "(${CTORRENT_LIBTORRENT_PREFIX}) but no libtorrent >= 2.1 CMake config "
      "package was found under it")
  endif()
else()
  find_package(LibtorrentRasterbar 2.1 CONFIG QUIET)
endif()
if(TARGET LibtorrentRasterbar::torrent-rasterbar)
  message(STATUS "ctorrent: using libtorrent via CMake config package "
                 "(${LibtorrentRasterbar_VERSION})")
  set(CTORRENT_LT_TARGET LibtorrentRasterbar::torrent-rasterbar)
  target_link_libraries(ctorrent_libtorrent INTERFACE
    LibtorrentRasterbar::torrent-rasterbar)
  _ctorrent_collect_link_items(LibtorrentRasterbar::torrent-rasterbar
    _ctorrent_extra)
  set(_ctorrent_extra_lines "")
  foreach(_ctorrent_item IN LISTS _ctorrent_extra)
    string(APPEND _ctorrent_extra_lines "EXTRA=${_ctorrent_item}\n")
  endforeach()
  # Imported location is config-dependent; resolve at generate time.
  file(GENERATE
    OUTPUT "${CMAKE_BINARY_DIR}/ctorrent-link.txt"
    CONTENT
      "LINKDIR=$<TARGET_FILE_DIR:LibtorrentRasterbar::torrent-rasterbar>\nLIB=torrent-rasterbar\nTYPE=$<TARGET_PROPERTY:LibtorrentRasterbar::torrent-rasterbar,TYPE>\n${_ctorrent_extra_lines}")
else()
  find_package(PkgConfig REQUIRED)
  pkg_check_modules(ctorrent_lt REQUIRED IMPORTED_TARGET libtorrent-rasterbar>=2.1)
  message(STATUS "ctorrent: using libtorrent via pkg-config "
                 "(${ctorrent_lt_VERSION})")
  set(CTORRENT_LT_TARGET PkgConfig::ctorrent_lt)
  target_link_libraries(ctorrent_libtorrent INTERFACE PkgConfig::ctorrent_lt)
  # Determine what pkg-config actually resolved: a static-only install
  # yields an archive, which needs the Libs.private/Cflags of --static.
  set(_ctorrent_lt_type SHARED_LIBRARY)
  foreach(_ctorrent_lt_lib IN LISTS ctorrent_lt_LINK_LIBRARIES)
    if(_ctorrent_lt_lib MATCHES "torrent-rasterbar[^/\\]*\\.(a|lib)$")
      set(_ctorrent_lt_type STATIC_LIBRARY)
    endif()
  endforeach()
  if(_ctorrent_lt_type STREQUAL "STATIC_LIBRARY")
    message(STATUS "ctorrent: pkg-config resolved a static libtorrent; "
                   "using its --static link and compile flags")
    target_compile_options(ctorrent_libtorrent INTERFACE
      ${ctorrent_lt_STATIC_CFLAGS_OTHER})
    target_link_libraries(ctorrent_libtorrent INTERFACE
      ${ctorrent_lt_STATIC_LINK_LIBRARIES})
  endif()
  if(ctorrent_lt_LIBRARY_DIRS)
    list(GET ctorrent_lt_LIBRARY_DIRS 0 _ctorrent_lt_dir)
  else()
    # Empty LIBRARY_DIRS means the default linker path.
    set(_ctorrent_lt_dir "")
  endif()
  # Everything pkg-config resolved beyond the primary library.
  if(_ctorrent_lt_type STREQUAL "STATIC_LIBRARY")
    set(_ctorrent_pc_items ${ctorrent_lt_STATIC_LINK_LIBRARIES})
  else()
    set(_ctorrent_pc_items ${ctorrent_lt_LINK_LIBRARIES})
  endif()
  set(_ctorrent_extra_lines "")
  foreach(_ctorrent_item IN LISTS _ctorrent_pc_items)
    if(NOT _ctorrent_item MATCHES "torrent-rasterbar")
      string(APPEND _ctorrent_extra_lines "EXTRA=${_ctorrent_item}\n")
    endif()
  endforeach()
  file(WRITE "${CMAKE_BINARY_DIR}/ctorrent-link.txt"
    "LINKDIR=${_ctorrent_lt_dir}\nLIB=torrent-rasterbar\nTYPE=${_ctorrent_lt_type}\n${_ctorrent_extra_lines}")
endif()
