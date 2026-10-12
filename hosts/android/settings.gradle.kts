pluginManagement {
    includeBuild("build-logic")
    repositories {
        google()
        mavenCentral()
        mavenLocal()
        gradlePluginPortal()
    }
}
dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
        mavenLocal()
        maven("https://jitpack.io")
    }
}
rootProject.name = "Polkadot Community"
include(":app")
include(":common")
include(":design")
include(":chains")
include(":database")

include(":bindings:bandersnatch-crypto")
include(":bindings:hydra-dx-math")
include(":bindings:sr25519-vrf")

// The :bindings:truapi-host module compiles the TrUAPI Rust core from an
// out-of-repo checkout, located via `truapi.dir` in local.properties or the
// TRUAPI_DIR env var. The module is required — :feature:products:impl depends
// on it in every variant — so a missing or stale checkout fails configuration
// here, with instructions, instead of later with a missing-project error.
run {
    val localProps = java.util.Properties().apply {
        val f = file("local.properties")
        if (f.exists()) f.inputStream().use { load(it) }
    }
    val truapiDir = (localProps.getProperty("truapi.dir") ?: System.getenv("TRUAPI_DIR"))
        ?.takeIf { it.isNotBlank() }
    val configuredDir = truapiDir?.let {
        val d = File(it).let { p -> if (p.isAbsolute) p else file(it) }
        if (File(d, "rust/crates/truapi").isDirectory) d else null
    }
    // When this tree is vendored into the core's own repository the core is two
    // levels up, so there is nothing to check out. An explicit truapi.dir or
    // TRUAPI_DIR still wins, which keeps a separate checkout working for anyone
    // building this tree on its own.
    val inTreeDir = file("../..").takeIf { File(it, "rust/crates/truapi").isDirectory }
    // Only an absent setting falls back. A configured one that does not resolve
    // still fails here, with instructions, rather than being quietly replaced
    // and dying later in cargo.
    val resolved = if (truapiDir != null) configuredDir else inTreeDir
    if (resolved == null) {
        val configured = truapiDir?.let { "truapi.dir=$it does not contain rust/crates/truapi" }
            ?: "truapi.dir is not set and this tree is not inside the core repository"
        error(
            "A truapi checkout is required to build this project ($configured). " +
                "Run scripts/setup-truapi.py, which clones " +
                "https://github.com/paritytech/trinity-user-agents at the `truapi_ref` pin from " +
                ".github/actions/install/action.yaml and sets truapi.dir in local.properties. " +
                "Pass --dir to keep the checkout somewhere else, or set TRUAPI_DIR yourself.",
        )
    }
    include(":bindings:truapi-host")
    // Standalone demo app that loads a product through the TrUAPI core.
    if (file("bindings/truapi-host-demo/build.gradle.kts").exists()) {
        include(":bindings:truapi-host-demo")
    }
}

include(":test-shared")

include(":feature:account:api")
include(":feature:account:impl")
include(":feature:balances:api")
include(":feature:balances:impl")
include(":feature:calls:api")
include(":feature:calls:impl")
include(":feature:chain-resources:api")
include(":feature:chain-resources:impl")
include(":feature:chats:api")
include(":feature:chats:impl")
include(":feature:chats:transport-protocol")
include(":feature:cross-chain-transfers:api")
include(":feature:cross-chain-transfers:impl")
include(":feature:device-sync:api")
include(":feature:device-sync:impl")
include(":feature:fund:api")
include(":feature:fund:impl")
include(":feature:prices:api")
include(":feature:prices:impl")
include(":feature:scan:api")
include(":feature:scan:impl")
include(":feature:settings:api")
include(":feature:settings:impl")
include(":feature:splash:api")
include(":feature:splash:impl")
include(":feature:sso:api")
include(":feature:sso:impl")
include(":feature:statement-store:api")
include(":feature:statement-store:impl")
include(":feature:swap:api")
include(":feature:swap:impl")
include(":feature:tokens:api")
include(":feature:tokens:impl")
include(":feature:transaction-storage:api")
include(":feature:transaction-storage:impl")
include(":feature:pgas:api")
include(":feature:pgas:impl")
include(":feature:transactions:api")
include(":feature:transactions:impl")
include(":feature:transfers:api")
include(":feature:transfers:impl")
include(":feature:usernames:api")
include(":feature:usernames:impl")
include(":feature:wallet:api")
include(":feature:wallet:impl")
include(":feature:xcm:api")
include(":feature:xcm:impl")
include(":feature:videogame:api")
include(":feature:videogame:impl")
include(":feature:people:api")
include(":feature:people:impl")
include(":feature:members:api")
include(":feature:members:impl")
include(":feature:vouchers:api")
include(":feature:vouchers:impl")
include(":feature:become-citizen:api")
include(":feature:become-citizen:impl")
include(":feature:backup:api")
include(":feature:backup:impl")
include(":feature:mobrules:api")
include(":feature:mobrules:impl")
include(":feature:identity:api")
include(":feature:identity:impl")
include(":feature:products:api")
include(":feature:products:impl")
include(":feature:coinage:api")
include(":feature:coinage:impl")
include(":feature:dotns:api")
include(":feature:dotns:impl")
include(":feature:dotns-gateway:api")
include(":feature:dotns-gateway:impl")
include(":feature:upgrade-username:api")
include(":feature:upgrade-username:impl")
include(":feature:connection-status:api")
include(":feature:connection-status:impl")
include(":feature:revive:api")
include(":feature:revive:impl")
include(":feature:w3s-pay:impl")

include(":tools:assethub-sdk:api")
include(":tools:assethub-sdk:impl")
include(":tools:auth:api")
include(":tools:auth:impl")
include(":tools:backup:api")
include(":tools:backup:impl")
include(":tools:biometrics:api")
include(":tools:biometrics:impl")
include(":tools:car-parser")
include(":tools:common")
include(":tools:media-connection:api")
include(":tools:media-connection:impl")
include(":tools:hydration-sdk:api")
include(":tools:hydration-sdk:impl")
include(":tools:integrity:api")
include(":tools:integrity:impl")
include(":tools:jwt-auth:api")
include(":tools:jwt-auth:impl")
include(":tools:push-notifications:api")
include(":tools:push-notifications:impl")
include(":tools:remoteconfig:api")
include(":tools:remoteconfig:impl")
include(":tools:ipfs:api")
include(":tools:ipfs:impl")
