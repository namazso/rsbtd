// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { useEffect, useState } from 'react';
import { RouterProvider } from 'react-router';
import { Toaster } from 'sonner';
import { AuthError } from '@/api/client';
import { startLive } from '@/api/live';
import { ThemeApplier } from '@/components/ThemeApplier';
import { TooltipProvider } from '@/components/ui/tooltip';
import LoginPage from '@/features/auth/LoginPage';
import { router } from '@/routes';
import { useConnection } from '@/store/connection';

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 3_000,
      refetchOnWindowFocus: false,
      retry: (failureCount, error) => failureCount < 1 && !(error instanceof AuthError),
    },
  },
});

export default function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <TooltipProvider delayDuration={400}>
        <ThemeApplier />
        <Toaster richColors position="bottom-right" closeButton />
        <Root />
      </TooltipProvider>
    </QueryClientProvider>
  );
}

function Root() {
  const state = useConnection((s) => s.state);
  const [gated, setGated] = useState(false);

  useEffect(() => {
    startLive();
  }, []);

  // Connection gate instead of a /login route: the current hash URL is
  // preserved and restored once connected. The page hosts both credentials
  // and the endpoint editor, so it must also cover the `down` state; it
  // stays up through the down state's automatic re-probe cycle so the form
  // doesn't flicker away while a retry is in flight.
  if ((state === 'authRequired' || state === 'down') && !gated) setGated(true);
  else if ((state === 'up' || state === 'degraded') && gated) setGated(false);

  if (gated) return <LoginPage />;
  return <RouterProvider router={router} />;
}
