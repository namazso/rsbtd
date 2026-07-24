// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import type { CodegenConfig } from '@graphql-codegen/cli';

// Typed GraphQL documents from the checked-in SDL copy (webui/schema.graphql;
// refresh with `npm run schema:refresh`).
//
// documentMode 'string' emits TypedDocumentString (plain strings): the fetch
// wrapper and graphql-ws consume them directly, so the `graphql` package
// stays a dev-only dependency and never ships in the bundle.
const config: CodegenConfig = {
  schema: './schema.graphql',
  documents: ['src/**/*.{ts,tsx}'],
  ignoreNoDocuments: true,
  generates: {
    './src/gen/gql/': {
      preset: 'client',
      config: {
        documentMode: 'string',
        useTypeImports: true,
        scalars: {
          InfoHash: 'string',
          Base64: 'string',
        },
      },
      presetConfig: {
        // Plain result types (no useFragment ceremony): the torrent store and
        // field registry work with the fragment's flat object shape.
        fragmentMasking: false,
      },
    },
  },
};

export default config;
