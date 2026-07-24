#!/bin/sh
# Copyright (C) 2026  namazso <admin@namazso.eu>
# SPDX-License-Identifier: MPL-2.0
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

# Container entrypoint: run rsbtd with the operator-mounted config at
# /etc/rsbtd/rsbtd.toml, or generate one from environment variables.
# The image deliberately ships no config: a baked-in default token
# would be a universal credential for every deployment, so RSBTD_TOKEN
# is required when no config is mounted.
#
#   RSBTD_TOKEN    API bearer token (required without a mounted config)
#   RSBTD_LISTEN   listen address (default 0.0.0.0:3928)
set -eu

config=/etc/rsbtd/rsbtd.toml
if [ ! -e "$config" ]; then
    if [ -z "${RSBTD_TOKEN:-}" ]; then
        echo "rsbtd: no config mounted at $config and RSBTD_TOKEN is not set." >&2
        echo "rsbtd: set -e RSBTD_TOKEN=<secret> or mount your own config." >&2
        exit 1
    fi
    case "$RSBTD_TOKEN" in
        *'"'*|*'\'*)
            echo 'rsbtd: RSBTD_TOKEN must not contain `"` or `\`' >&2
            exit 1 ;;
    esac
    config=/run/rsbtd/rsbtd.toml
    cat > "$config" <<EOF
state_dir = "/var/lib/rsbtd/state"

[api]
listen = "${RSBTD_LISTEN:-0.0.0.0:3928}"
token = "${RSBTD_TOKEN}"
serve_root = "/usr/share/rsbtd/webui"
EOF
fi

exec /usr/bin/rsbtd --config "$config" "$@"
