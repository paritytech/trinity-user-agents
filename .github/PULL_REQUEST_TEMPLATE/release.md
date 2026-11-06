## Release: <!-- e.g. @parity/truapi 0.1.1 -->

> [!IMPORTANT]
> The PR title must start with `release:` for the publish workflow to fire.
> Example: `release: @parity/truapi 0.5.0, @parity/truapi-host 0.2.0, @parity/ios-host 0.2.0, @parity/android-host 0.1.0`.
> Don't rewrite the squash commit subject in the merge dialog — the
> `release:` prefix has to land on `main`.

### Summary

<!-- One-paragraph summary of what's shipping in this version -->

### Checklist

- [ ] Ran `npm run changeset` and selected the package + bump type (patch / minor / major)
- [ ] Ran `npm run version-packages` to consume the changeset
- [ ] `js/packages/truapi/package.json` version is bumped
- [ ] `js/packages/truapi-host/package.json` version is bumped when releasing the host
- [ ] The PR title includes `@parity/ios-host <version>` when publishing the iOS host
- [ ] Regenerated iOS bindings and container outputs are committed; CI builds and simulator-tests the XCFramework, publishes it, then commits `Package.swift`
- [ ] The PR title includes `@parity/android-host <version>` when publishing the Android host AAR; nothing in the tree records that version, so there is no manifest to bump
- [ ] `@parity/truapi-host` depends on `^<current @parity/truapi version>`
- [ ] `js/packages/truapi/CHANGELOG.md` has the new entry
- [ ] `js/packages/truapi-host/CHANGELOG.md` has the new entry when releasing the host
- [ ] `rust/crates/truapi/Cargo.toml` and `rust/crates/truapi-host-cli/Cargo.toml` versions match `js/packages/truapi/package.json` (`npm run check-release-versions`), and `Cargo.lock` records them
- [ ] Releasing `@parity/truapi` also publishes the prebuilt `truapi-host` binaries; no extra title entry is needed
- [ ] `rust/crates/trinity-stack/Cargo.toml` version matches `js/packages/trinity-stack/package.json` when releasing `@parity/trinity-stack`, which also publishes the crate to crates.io
- [ ] No leftover files under `.changeset/` (other than `config.json`)
