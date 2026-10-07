// Feature-availability flags. Each one is independent and gates a single feature; a configuration
// sets whichever combination it ships.
//
// FEATURE_INPUT      — the scan panel's in-panel search field, its
//                      results and keyboard-tracking panel; without it the
//                      panel shows a search button that opens full-screen
//                      search                                   (Debug, DevCI, Nightly).
// FEATURE_PRODUCTS   — the browse tab                            (Debug, DevCI, Nightly).
// FEATURE_SIGN_IN    — sign in with Polkadot: the `pair` deeplink
//                      and the linked-devices settings row       (Debug, DevCI, Nightly).
//
// Release sets none of them: no environment flag, no feature flag.
//
// Every flag is positive: `#if FEATURE_X` always reads "this build has X", and each `#if` site is
// self-contained — its `#else` arm depends only on its own flag, never on another being set.
//
// The environment axis (UNSTABLE / NIGHTLY / SAFETYNET) is independent of the feature flags and
// unconstrained by the checks above. Exactly one environment flag may be set.

#if (UNSTABLE && NIGHTLY) || (UNSTABLE && SAFETYNET) || (NIGHTLY && SAFETYNET)
    #error("UNSTABLE, NIGHTLY and SAFETYNET are mutually exclusive — see Configs/base.*.xcconfig")
#endif
