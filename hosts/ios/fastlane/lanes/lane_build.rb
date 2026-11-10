desc "Build the iOS app"
desc "Parameters:"
desc "- 'scheme : <value>' defines scheme to use for build phase"
desc "- 'target : <value>' defines target to build"
desc "- 'configuration : <value>' defines configuration for build"
desc "- 'debug : true/false' to enable verbose logging for troubleshooting build failures"
desc " "
desc "Example usage: fastlane build_app scheme:'polkadot-app' target: 'polkadot-app' configuration: 'Release' "
desc "Example usage: fastlane base_build_app scheme:'polkadot-app' target: 'polkadot-app' configuration: 'DevCI' debug:true"
lane :base_build_app do |options|
  scheme = options[:scheme]
  target = options[:target]
  configuration = options[:configuration]

  # Sentry is linked only when Packages/IssueMonitoring resolves with ISSUE_MONITORING set.
  # Release leaves it unset so sentry-cocoa never enters the dependency graph. The resolved
  # package cache is keyed per flavour so a cached checkout cannot carry Sentry into Release.
  issue_monitoring_enabled = configuration != "Release"
  if issue_monitoring_enabled
    ENV["ISSUE_MONITORING"] = "sentry"
  else
    ENV.delete("ISSUE_MONITORING")
  end
  source_packages_path = issue_monitoring_enabled ? "source_packages" : "source_packages_release"
  debug_mode = options[:debug] == true || options[:debug] == 'true'
  app_identifier = ENV["IOS_BUNDLE_ID"]
  extension_identifier = ENV["IOS_EXTENSION_BUNDLE_ID"]

  destination_plist = "../#{target}/GoogleService-Info.plist"

  if ENV["RUN_IN_CI"] == "true"
    unless File.file?(destination_plist) && !File.zero?(destination_plist)
      UI.user_error!("Missing generated GoogleService-Info.plist at #{destination_plist}")
    end
  else
    plist_suffix_map = {
      "io.parity.polkadotapp" => "-Release",
      "io.parity.polkadotapp.safety" => "-Safety",
      "io.parity.polkadotapp.develop" => "-Dev"
    }

    plist_suffix = plist_suffix_map.fetch(app_identifier) do
      UI.user_error!("Unsupported IOS_BUNDLE_ID for Firebase configuration: #{app_identifier.inspect}")
    end
    source_plist = "../#{target}/GoogleService/GoogleService-Info#{plist_suffix}.plist"
    sh("cp", source_plist, destination_plist)
  end

  profile_name = ENV["PROVISIONING_PROFILE_SPECIFIER"]
  extension_profile_name = ENV["EXTENSION_PROVISIONING_PROFILE_SPECIFIER"]
  output_name = scheme
  export_method = options[:export_method] || ENV["EXPORT_METHOD"] || "app-store"
  compile_bitcode = false
  xcodeproj_path = "./#{target}.xcodeproj"
  extension_target = "NotificationServiceExtension"

  clean_build_artifacts

  increment_build_number(
    build_number: options[:build_number] || ENV["BUILD_NUMBER"],
    xcodeproj: xcodeproj_path
  )

  # Update code signing for main app
  update_code_signing_settings(
    use_automatic_signing: false,
    targets: [target],
    code_sign_identity: ENV["CODE_SIGN_IDENTITY"],
    bundle_identifier: app_identifier,
    profile_name: profile_name,
    build_configurations: [configuration]
  )

  # Update code signing for notification extension (if configured)
  if extension_identifier && !extension_identifier.empty?
    update_code_signing_settings(
      use_automatic_signing: false,
      targets: [extension_target],
      code_sign_identity: ENV["CODE_SIGN_IDENTITY"],
      bundle_identifier: extension_identifier,
      profile_name: extension_profile_name,
      build_configurations: [configuration]
    )
  end

  # Prepare provisioning profiles mapping
  provisioning_profiles = { app_identifier => profile_name }
  if extension_identifier && !extension_identifier.empty?
    provisioning_profiles[extension_identifier] = extension_profile_name
  end

  # Emit an .xcresult bundle so CI can extract & ratchet build warnings.
  result_bundle = options[:result_bundle] == true || options[:result_bundle] == 'true'

  # Base gym parameters
  gym_params = {
    scheme: scheme,
    output_name: output_name,
    configuration: configuration,
    xcargs: "-skipPackagePluginValidation -skipMacroValidation RUN_IN_CI=#{ENV['RUN_IN_CI']}",
    clean: true,
    result_bundle: result_bundle,
    cloned_source_packages_path: source_packages_path,
    export_options: {
      method: export_method,
      provisioningProfiles: provisioning_profiles,
      compileBitcode: compile_bitcode
    }
  }

  # DEBUG MODE: Helps diagnose build failures (signing, dependencies, etc.)
  # Enables: verbose logs, saves build artifacts
  # Use when: normal builds fail with unclear errors
  if debug_mode
    UI.important "🔍 Debug mode enabled - verbose logging and artifacts will be collected"
    gym_params.merge!({
      buildlog_path: "./fastlane/build_logs/",  # Saves xcodebuild logs for analysis
      disable_xcpretty: true                     # Shows full xcodebuild output
    })
  end

  gym(gym_params)

  verify_no_issue_monitoring unless issue_monitoring_enabled
end

desc "Fails when the archived Release app still carries the Sentry SDK"
desc "Example usage: fastlane verify_no_issue_monitoring"
lane :verify_no_issue_monitoring do
  ipa_path = lane_context[SharedValues::IPA_OUTPUT_PATH]
  UI.user_error!("No .ipa produced, cannot verify Sentry exclusion") if ipa_path.to_s.empty?

  require "shellwords"
  require "tmpdir"

  Dir.mktmpdir do |unpack_dir|
    sh("unzip", "-q", ipa_path, "-d", unpack_dir)

    app_bundle = Dir.glob(File.join(unpack_dir, "Payload", "*.app")).first
    UI.user_error!("No .app inside #{ipa_path}") if app_bundle.nil?

    # Covers the app binary, every embedded framework and every PlugIn (the notification
    # service extension), whatever the bundle is named.
    candidates = Dir.glob(File.join(app_bundle, "**", "*"), File::FNM_DOTMATCH)
      .select { |path| File.file?(path) && !File.symlink?(path) }

    mach_o_files = candidates.each_slice(200).flat_map do |batch|
      # --mime-type keeps one line per file; the plain description spans several for fat binaries.
      types = `file -b --mime-type #{batch.map(&:shellescape).join(' ')}`.lines.map(&:strip)
      batch.zip(types).select { |_, type| type == "application/x-mach-binary" }.map(&:first)
    end

    offenders = mach_o_files.reject do |binary|
      `strings -a #{binary.shellescape} | grep -c -E 'SentrySDK|sentry\\.io'`.strip.to_i.zero?
    end

    unless offenders.empty?
      names = offenders.map { |path| path.sub("#{unpack_dir}/", "") }.join(", ")
      UI.user_error!("Release bundle carries Sentry symbols or a DSN in: #{names}")
    end
  end

  UI.success("Release bundle contains no Sentry SDK")
end
