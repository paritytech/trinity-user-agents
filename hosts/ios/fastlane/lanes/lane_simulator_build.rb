desc "Build the iOS app for iOS Simulator with ad-hoc signing"
desc "Produces an ad-hoc-signed (CODE_SIGN_IDENTITY=-) arm64-iphonesimulator"
desc ".app bundle under build_simulator/ — entitlements embedded, no"
desc "provisioning profile — that any Apple Silicon macOS host can install"
desc "via `xcrun simctl install`."
desc "Used by paritytech/triangle-e2e to drive the iOS shard of the App Tests"
desc "matrix; mirrors the role app-nightly.apk plays for the Android shard."
desc " "
desc "Parameters:"
desc "- 'scheme'        : scheme to build (default: polkadot-app)"
desc "- 'configuration' : build configuration (default: Nightly — mirrors the"
desc "                    Android nightly W3S build the e2e suite targets)"
desc " "
desc "Example usage: fastlane build_app_simulator"
desc "Example usage: fastlane build_app_simulator configuration:'DevCI'"
lane :build_app_simulator do |options|
  scheme        = options[:scheme] || "polkadot-app"
  configuration = options[:configuration] || "Nightly"

  derived_data_path = "build_simulator_dd"
  output_dir        = "build_simulator"

  # Fastlane cd's into `fastlane/` before running a lane (see
  # fastlane/runner.rb:45 `chdir`), so without switching back to the repo
  # root `polkadot-app.xcodeproj`, the SPM cache (`source_packages/`), and
  # the derivedData / output paths all resolve under `fastlane/`. Wrap the
  # whole build in `Dir.chdir("..")` so paths match repo-root layout — same
  # convention `gym` follows internally for the existing lanes.
  Dir.chdir("..") do
    # Raw xcodebuild rather than `gym` because `gym` always archives for a
    # device + needs provisioning. Simulator builds don't need a provisioning
    # profile, but they DO need AD-HOC signing (CODE_SIGN_IDENTITY="-") so the
    # target's entitlements — App Group + keychain-access-groups — are actually
    # embedded. Without them the app hits errSecMissingEntitlement (-34018) at
    # runtime: Firebase RemoteConfig can't read its keychain installations
    # token, chain setup never completes, and onboarding stalls on "Waiting for
    # network". CODE_SIGNING_ALLOWED=NO produced an unsigned .app that booted
    # but failed this way in the triangle-e2e iOS shard. Mirrors the local
    # triangle-e2e scripts/ios-build.sh signing flags exactly.
    #
    # `ARCHS=arm64` restricts to the arm64-iphonesimulator slice only. Without
    # it, `-destination generic/platform=iOS Simulator` builds a fat
    # arm64+x86_64 binary, which roughly doubles peak memory during Swift
    # WMO and OOM-killed the `macos-26` runner on the first attempt (job
    # 83650893151 went silent at 11:52:30 mid-compile, no error surface). The
    # triangle-e2e CI runners that consume this artifact (macos-latest,
    # macos-14-large, parity-macos) are all Apple Silicon, so the x86_64
    # slice would be dead weight regardless.
    sh(
      [
        "xcodebuild",
        "build",
        "-project", "polkadot-app.xcodeproj",
        "-scheme", scheme,
        "-configuration", configuration,
        "-sdk", "iphonesimulator",
        "-destination", "'generic/platform=iOS Simulator'",
        "-derivedDataPath", derived_data_path,
        "-clonedSourcePackagesDirPath", "source_packages",
        "-skipPackagePluginValidation",
        "-skipMacroValidation",
        "ARCHS=arm64",
        "ONLY_ACTIVE_ARCH=NO",
        # E2E_TEST gates the simulator-only onboarding enablers (attest /
        # jailbreak / LocalAuth / WebView inspection). It lives ONLY here, in
        # the triangle-e2e build lane — never in an xcconfig — so it can never
        # reach a device or Release/Nightly App Store build. A plain simulator
        # build (local dev, or a Nightly sim build without this lane) keeps the
        # real code paths.
        "SWIFT_ACTIVE_COMPILATION_CONDITIONS='$(inherited) E2E_TEST'",
        'CODE_SIGN_IDENTITY="-"',
        "CODE_SIGNING_REQUIRED=NO",
        "CODE_SIGNING_ALLOWED=YES",
        "RUN_IN_CI=#{ENV['RUN_IN_CI'] || 'true'}"
      ].join(" ")
    )

    # Copy the produced .app out of derivedData into build_simulator/ so
    # downstream CI steps (zip, upload-artifact, gh-release) have a stable
    # path that doesn't depend on Xcode's internal layout.
    products_dir = "#{derived_data_path}/Build/Products/#{configuration}-iphonesimulator"
    app_bundle = Dir.glob("#{products_dir}/*.app").first
    UI.user_error!("No .app produced under #{products_dir}") if app_bundle.nil?

    FileUtils.mkdir_p(output_dir)
    destination = File.join(output_dir, File.basename(app_bundle))
    FileUtils.rm_rf(destination)
    FileUtils.cp_r(app_bundle, destination)

    UI.success "Built #{File.basename(app_bundle)} -> #{destination}"
  end
end
