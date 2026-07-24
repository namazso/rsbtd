/* Copyright (C) 2026  namazso <admin@namazso.eu>
 * SPDX-License-Identifier: MPL-2.0
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

/* ABI configuration: CT_SIZEOF_* / CT_ALIGNOF_* / CT_LT_* macros harvested
 * from the libtorrent headers at CMake configure time. Storage structs that
 * masquerade as libtorrent C++ objects size themselves with these; the C++
 * implementation static_asserts every value against the real types.
 */
#ifndef CT_ABI_H_INCLUDED
#define CT_ABI_H_INCLUDED

/* Generated into the build tree; resolved through the include path both
 * there (generated/include/) and when installed (include/). */
#include "ctorrent/ct_abi_config.h"

#if defined(__cplusplus)
#define CT_ALIGNAS(x) alignas(x)
#else
#define CT_ALIGNAS(x) _Alignas(x)
#endif

#endif
