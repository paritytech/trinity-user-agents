package io.paritytech.polkadotapp.chains.network.binding

import io.novasama.substrate_sdk_android.runtime.definitions.types.composite.DictEnum
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.scale.ToDynamicScaleInstance

sealed class OriginCaller : ToDynamicScaleInstance {
    sealed class System : OriginCaller() {
        object Root : System() {
            override fun toEncodableInstance(): Any? {
                return wrapInSystemDict(DictEnum.Entry("Root", null))
            }
        }

        class Signed(val accountId: AccountId) : System() {
            override fun toEncodableInstance(): Any? {
                return wrapInSystemDict(DictEnum.Entry("Signed", accountId.value))
            }
        }

        protected fun wrapInSystemDict(inner: Any): DictEnum.Entry<*> {
            return DictEnum.Entry("system", inner)
        }
    }
}
