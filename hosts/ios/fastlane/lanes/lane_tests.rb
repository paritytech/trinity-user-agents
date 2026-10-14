desc "Run tests"
desc "Parameters:"
desc "- 'scheme : <value>' to define scheme to test"
desc "- 'debug : true/false' to enable verbose logging for troubleshooting build failures (especially Crashlytics issues)"
desc " "
desc "Example usage: fastlane test_build scheme:'polkadot-app'"
desc "Example usage: fastlane test_build scheme:'polkadot-app' debug:true"
lane :test_build do |options|
  scheme = options[:scheme]
  debug_mode = options[:debug] == true || options[:debug] == 'true'

  # Base scan parameters for running tests
  scan_params = {
    clean: true,
    cloned_source_packages_path: 'source_packages',
    scheme: scheme,
    configuration: "DevCI",
    # ENABLE_TESTABILITY=YES: SPM packages build as Release under custom configurations
    # (DevCI), which disables testability and breaks @testable imports in package tests
    xcargs: "-skipPackagePluginValidation -skipMacroValidation ENABLE_TESTABILITY=YES RUN_IN_CI=#{ENV['RUN_IN_CI']}",
    output_directory: "./fastlane/test_output/",
    # CI publishes report.junit as a check run, so name it explicitly rather than
    # relying on scan's default output types.
    output_types: "junit,html",
    output_files: "report.junit,report.html",
    result_bundle: true,
    disable_concurrent_testing: true
  }

  # DEBUG MODE: Helps diagnose build failures (Crashlytics, dependencies, etc.)
  # Enables: verbose logs, saves build artifacts, continues on failure
  # Use when: normal builds fail with unclear errors
  if debug_mode
    UI.important "🔍 Debug mode enabled - verbose logging and artifacts will be collected"
    scan_params.merge!({
      buildlog_path: "./fastlane/build_logs/",  # Saves xcodebuild logs for analysis
      xcpretty_args: "--verbose",               # Shows full xcodebuild output
      fail_build: false                          # Continues to collect maximum info even on failure
    })
  end

  scan(scan_params)
end
