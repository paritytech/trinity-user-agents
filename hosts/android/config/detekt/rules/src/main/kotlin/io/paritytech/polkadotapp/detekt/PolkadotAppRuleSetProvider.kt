package io.paritytech.polkadotapp.detekt

import dev.detekt.api.RuleSet
import dev.detekt.api.RuleSetId
import dev.detekt.api.RuleSetProvider

class PolkadotAppRuleSetProvider : RuleSetProvider {

    override val ruleSetId = RuleSetId("polkadotapp")

    override fun instance() = RuleSet(ruleSetId, listOf(::HardcodedComposeText))
}
