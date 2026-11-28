package io.paritytech.polkadotapp.feature_products_api.domain.runtime

/** Who decides whether the core draws materials. */
enum class MaterialsOverride {
    /** The Remote Config switch `materials_enabled`, as in a release build. */
    REMOTE_CONFIG,
    ON,
    OFF,
}

/**
 * Lets a build with debug tools draw materials, or stop drawing them, whatever Remote Config says, so a developer can
 * try a card on their own phone without changing the switch for everyone. The core reads the result when its runtime
 * starts, so a change applies after a restart. A build without debug tools always follows Remote Config.
 */
interface MaterialsDebugSettings {
    fun override(): MaterialsOverride

    fun setOverride(override: MaterialsOverride)
}
