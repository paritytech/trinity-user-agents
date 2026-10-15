// swift-tools-version: 5.10
// The swift-tools-version declares the minimum version of Swift required to build this package.

import Foundation
import PackageDescription

// Sentry is linked only when the build environment opts in. Release builds leave
// ISSUE_MONITORING unset, so sentry-cocoa never enters the dependency graph and no
// Sentry code, resource bundle or privacy manifest reaches the shipped bundle.
// Compile-time guards cannot achieve this: the app target passes -ObjC, which
// force-loads every Objective-C object file out of the static Sentry archive
// regardless of whether any call site survives the preprocessor.
let sentryEnabled = ProcessInfo.processInfo.environment["ISSUE_MONITORING"] == "sentry"

let package = Package(
    name: "IssueMonitoring",
    platforms: [.iOS(.v17)],
    products: [
        .library(
            name: "IssueMonitoring",
            targets: ["IssueMonitoring"]
        )
    ],
    dependencies: sentryEnabled
        ? [.package(url: "https://github.com/getsentry/sentry-cocoa", from: "9.0.0")]
        : [],
    targets: [
        .target(
            name: "IssueMonitoring",
            dependencies: sentryEnabled
                ? [.product(name: "Sentry", package: "sentry-cocoa")]
                : [],
            swiftSettings: sentryEnabled ? [.define("SENTRY_ENABLED")] : []
        )
    ]
)
