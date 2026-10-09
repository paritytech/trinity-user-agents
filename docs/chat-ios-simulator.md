# Testing Chat on the iOS simulator

Use this guide for the reference iOS app in **this Host repository**, including the Chat product and its native Coinage integration. The standalone upstream iOS repository or an older published Host binary does not necessarily contain these changes.

## Safety and current behavior

**Start with a new simulator and a separate test account. Do not import your established account or its recovery phrase.** A separate simulator does not isolate an imported identity from its existing peers: device announcements and protocol messages can affect those peers.

The wallet integration and Chat reception are separate concerns:

- The reference iOS runtime injects the existing native `CoinageService` adapter. An unavailable native wallet returns an error; it does not fall back to a second Rust wallet.
- Built-in Swift Chat and product workers start independently. Opening the Chat product does not stop or replace built-in Chat.
- The product has its own persisted Chat device and conversation state under the wallet identity. Peers must authenticate/learn that device before sending it device-addressed traffic.
- Existing native conversations remain with native Chat; the product does not automatically import the native app's contacts or message database. The legacy migration in the Rust Chat implementation is not a Swift database migration.
- Identity-level invitation subscriptions overlap. **Inference:** an invitation may appear in both surfaces. This is not a single shared inbox where the first reader necessarily consumes the message for everyone.
- Multi-device delivery can include both clients, but duplicate invitation handling, acknowledgments, and device updates need an actual coexistence test. Do not treat coexistence as verified solely because the app builds.
- Product reception depends on its worker/runtime being alive. Keeping the product open is the simplest initial test. This guide makes no guarantee of reception after termination or iOS background suspension.

Do not test with valuable funds. Establish message-routing behavior before testing payments with dedicated test funds on the intended test network.

## Recommended: download the CI simulator app

This avoids building Rust, bindings, and the XCFramework on your Mac.

### Prerequisites

- An **Apple Silicon Mac**, Xcode, and an installed iOS Simulator runtime compatible with the preview build. The CI preview is arm64 and cannot be installed on a physical iPhone or an Intel simulator.
- Access to the repository's GitHub Actions artifacts.
- A successful **iOS CI / Simulator preview build** containing the intended Host changes.
- The **matching Chat product build and its launch location**. The Host `.app` and the Chat product are separate artifacts; installing the Host alone does not update a deployed Chat product.

Uncommitted changes are not included in CI. First arrange for the intended source changes to be committed and pushed to the existing feature branch/PR. No PR merge is required. Verify the artifact's commit rather than downloading an arbitrary recent build.

The workflow builds the local core and uses the configured development Firebase secret with `DevCI`. If CI lacks the required secrets or fails, there is no usable preview to download; use the failure logs rather than substituting an unrelated build.

### Install

1. Open Xcode, then **Xcode → Open Developer Tool → Simulator**. Create or select a fresh compatible iPhone simulator and boot it. Do not erase a simulator containing data you want to keep.
2. Open the successful iOS CI run and download `simulator-preview-<short-sha>`.
3. Unzip the download. A browser download wraps the `.app.zip` in another ZIP, so it needs unzipping twice.
4. Drag `polkadot-app.app` into the booted Simulator, then launch it from the simulator's Home Screen.

Alternatively, with GitHub CLI installed and authenticated, run these commands in a new empty download directory. Replace the example run ID and short SHA with those from the same successful CI run:

```bash
RUN_ID=123456789
SHORT_SHA=012345678

gh run download "$RUN_ID" --repo paritytech/host-rust-core \
  --name "simulator-preview-$SHORT_SHA"
unzip polkadot-app-*.app.zip
xcrun simctl install booted polkadot-app.app
xcrun simctl launch booted io.parity.polkadotapp.develop
```

Have only the intended simulator booted when using `booted`. Artifacts expire after 14 days. This is a simulator install, not TestFlight or a signed device build.

### Load the matching Chat product

Before starting the test, record:

- Host commit and CI run.
- Chat product commit and content hash/CID, where available.
- Product name or development launch URL corresponding to that build.
- The configured network and both test identities.

Open that product through the reference app's product browser/launcher. Do not assume that opening an existing public product name loads your local changes. If the matching product bundle or launch location has not been prepared, stop here: the Host can be tested for startup, but the new Chat implementation cannot yet be tested end to end.

## Alternative: build the Host locally on the Mac

Use a checkout containing the intended changes from this Host repository, not a fresh checkout of the standalone upstream iOS app. Install Xcode, the repository's required Rust toolchains through rustup, and Node/npm first. Run from the **Host repository root**:

```bash
npm ci --ignore-scripts
make ios-bootstrap SIM_ONLY=1
./hosts/ios/Runscripts/setup-secrets.sh
open hosts/ios/polkadot-app.xcodeproj
```

`ios-bootstrap` generates bindings, builds/stages the local Host XCFramework, and prepares the packages Xcode needs. `SIM_ONLY=1` skips device slices. Rerun the bootstrap after changing the Rust API/bindings. Do not pair the new bindings with an older published XCFramework.

The secrets setup script only scaffolds missing local files. **Placeholder Firebase configuration can compile, but cannot get the app past startup.** Configure the intended development Firebase project and Remote Config before attempting a live test. See [the iOS publishing/configuration guide](../hosts/ios/docs/PUBLISHING.md). Keep credentials out of commits.

In Xcode select the **polkadot-app** scheme and a compatible iPhone simulator, then run with **Cmd+R**. The matching Chat product is still required separately, as described above.

The root `make ios-run` / `make ios-chat-run` targets default to a separate iOS checkout and playground workflows; they are not drop-in commands for testing this in-tree egui Chat product.

## Coexistence test checklist

Use test account **A** in the simulator and a separate account **B** as the peer. Keep the simulator app and product running during foreground tests. Record which surface displays each event, not just whether the sender reports success.

1. **Native baseline:** establish a native Chat conversation between A and B. Send `native-before-product-1` from B and confirm it appears in A's native Chat.
2. **Product startup:** open the matching Chat product as A. Record its device account/public identifier if exposed. Check that native contacts/history have not silently been assumed to be imported.
3. **Before product acceptance:** send `before-product-acceptance-1` from B. Record whether it appears in native Chat, product Chat, both, or neither. Sharing the identity alone does not establish the product's peer/device authority.
4. **Connect the product:** complete its invitation/acceptance and authenticated device update with B. Send `after-product-acceptance-1` from B and a reply from the product. Record delivery in both surfaces and sender acknowledgment behavior.
5. **Invitation overlap:** use another fresh peer/conversation to send A an invitation while both receivers run. Record where it appears. Accept it in one surface first; observe the other before accepting there. Record duplicate prompts or conflicting state.
6. **Worker lifecycle:** leave the product screen and repeat delivery, then test termination/relaunch separately. Leaving a screen may not stop its worker. Record actual worker/app state and do not infer background delivery guarantees from a foreground success.
7. **Persistence:** relaunch the app and reopen both surfaces. Check conversation state, duplicate messages, pending invitations, and any unexpected device changes.
8. **Payments, only after messaging:** on the intended test network, use a small dedicated test amount. Record native wallet balance and payment status before/after send, receive, and restart. Confirm the product uses native custody and does not present a second independently managed purse. Stop on ambiguous status rather than retrying blindly.

For each step capture the Host/product versions, network, sender, unique message label, receiving surface(s), acknowledgment state, and relevant logs. Do not include recovery phrases or private keys in reports.

Success is not merely “the product displayed a message.” We need to know whether coexistence is predictable and whether native Chat remains usable. Replacing built-in Chat requires a separate explicit handoff/migration; this test does not perform one.

## Source references

- [iOS startup](../hosts/ios/polkadot-app/Common/Services/ServiceCoordinator.swift): starts native Chat and product workers independently.
- [Host device state](../rust/crates/truapi-server/src/runtime/native_chat/actor.rs): product-specific device construction and legacy Rust migration.
- [Chat and main-purse design](rfcs/native-chat-main-purse.md): product/Host ownership and native wallet custody.
- [iOS CI workflow](../.github/workflows/ios-pr.yml): preview build, Firebase configuration, artifact naming, and installation instructions.
- [Host build targets](../Makefile): `ios-bootstrap` and simulator/device build options.
