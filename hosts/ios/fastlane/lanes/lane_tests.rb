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
    result_bundle: true,
    buildlog_path: "./fastlane/build_logs/",
    include_simulator_logs: true,
    disable_concurrent_testing: true
  }

  # Raw console output is optional; result bundles and logs are always retained.
  if debug_mode
    UI.important "Debug mode enabled - showing raw xcodebuild output"
    scan_params[:xcodebuild_formatter] = ""
  end

  scan(scan_params)
end
