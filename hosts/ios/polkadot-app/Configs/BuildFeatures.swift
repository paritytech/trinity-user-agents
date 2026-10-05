// Feature-availability flags. Each one is independent and gates a single feature; a configuration
// sets whichever combination it ships.
//
// FEATURE_DIMS       — any DIM surface at all: the DIM2 weekly game chat
//                      extension, its push routing, deeplinks, background
//                      tasks, and chain sync services            (Debug, DevCI).
// FEATURE_DIMS_FULL  — MobRules, DIM1, PolkadotPeer, DIM2 person
//                      actions, the extension-enable guard       (Debug, DevCI).
// FEATURE_INPUT      — the scan panel's in-panel search field, its
//                      results and keyboard-tracking panel; without it the
//                      panel shows a search button that opens full-screen
//                      search                                   (Debug, DevCI, Nightly).
// FEATURE_PRIZES     — prize branding and the game reveal webview   (none).
// FEATURE_PRODUCTS   — the browse tab                            (Debug, DevCI, Nightly).
// FEATURE_SIGN_IN    — sign in with Polkadot: the `pair` deeplink
//                      and the linked-devices settings row       (Debug, DevCI, Nightly).
//
// Release sets none of them: no environment flag, no feature flag.
//
// Every flag is positive: `#if FEATURE_X` always reads "this build has X", and each `#if` site is
// self-contained — its `#else` arm depends only on its own flag, never on another being set.
//
// FEATURE_DIMS_FULL names the DIM surfaces beyond the weekly game, so it presupposes FEATURE_DIMS.
//
// FEATURE_PRIZES and FEATURE_DIMS_FULL are the one genuinely exclusive pair: prize branding
// replaces the DIM2 bot identity rather than sitting alongside it, so a build cannot coherently
// carry both. FEATURE_PRODUCTS is independent of both and may be combined with either.
//
// The environment axis (UNSTABLE / NIGHTLY) is independent and unconstrained by these checks.

#if FEATURE_PRIZES && FEATURE_DIMS_FULL
    #error("FEATURE_PRIZES and FEATURE_DIMS_FULL are mutually exclusive — see Configs/base.*.xcconfig")
#endif

#if FEATURE_DIMS_FULL && !FEATURE_DIMS
    #error("FEATURE_DIMS_FULL requires FEATURE_DIMS — see Configs/base.*.xcconfig")
#endif

#if FEATURE_PRIZES && !FEATURE_DIMS
    #error("FEATURE_PRIZES requires FEATURE_DIMS — see Configs/base.*.xcconfig")
#endif
