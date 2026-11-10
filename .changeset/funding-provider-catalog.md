---
"@parity/truapi": minor
---

The core reads what funding providers need from their manifests itself:

- `FundingCandidate.brand` carries the name and icon a provider's root manifest gives it, from the shipped
  `FundingProviderEntry.root_manifest` and then dotNS, so hosts load no brand of their own.
- An App route is offered when a bundled entry says the provider ships an App, else when dotNS publishes an `app`
  subname for it; `NativeFundingCallbacks::provider_has_app` and `FundingPlatform::provider_has_app` are gone.
- One rule set for manifests: a Worker needs `$v` 1, an `appVersion`, a non-blank `entrypoint` and at least one
  surface (`chat`, `pocket` or `funding`), with `null` read as absent; a brand needs a non-blank `displayName`, a
  `description` and an icon with a CID and format, an icon the core cannot load costing only the icon.
