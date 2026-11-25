import { RouterProvider } from '@tanstack/react-router';

import { router } from './router';

// Bootstrap and the main-app provider shell (PappProvider, ConfirmationProvider,
// PairingModal, RemotePermissionPromptHost, the chat binding + AppShell) moved
// into the `_app` pathless layout route (src/routes/_app.tsx). App is now just
// the router mount, so the sibling `/call` route — loaded in the call window,
// which shares this bundle — runs none of the app. Shared providers (theme,
// i18n, error boundary, Toaster) stay in src/index.tsx wrapping this.
export const App = () => <RouterProvider router={router} />;
