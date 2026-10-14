package io.paritytech.polkadotapp.chains.util

@JvmInline
value class EncodedArguments private constructor(val encoded: Map<String, Any?>) {
    companion object {
        fun noArgs(): EncodedArguments {
            return emptyMap<String, Any?>().intoEncodedArgs()
        }

        inline fun <reified T> autoEncodedArgs(
            argument1: Pair<String, T>,
        ): EncodedArguments {
            return mapOf(
                argument1.first to argument1.second.scaleEncodeSerializable()
            ).intoEncodedArgs()
        }

        inline fun <reified T1, reified T2> autoEncodedArgs(
            argument1: Pair<String, T1>,
            argument2: Pair<String, T2>,
        ): EncodedArguments {
            return mapOf(
                argument1.first to argument1.second.scaleEncodeSerializable(),
                argument2.first to argument2.second.scaleEncodeSerializable(),
            ).intoEncodedArgs()
        }

        inline fun <reified T1, reified T2, reified T3> autoEncodedArgs(
            argument1: Pair<String, T1>,
            argument2: Pair<String, T2>,
            argument3: Pair<String, T3>,
        ): EncodedArguments {
            return mapOf(
                argument1.first to argument1.second.scaleEncodeSerializable(),
                argument2.first to argument2.second.scaleEncodeSerializable(),
                argument3.first to argument3.second.scaleEncodeSerializable(),
            ).intoEncodedArgs()
        }

        inline fun <reified T1, reified T2, reified T3, reified T4> autoEncodedArgs(
            argument1: Pair<String, T1>,
            argument2: Pair<String, T2>,
            argument3: Pair<String, T3>,
            argument4: Pair<String, T4>,
        ): EncodedArguments {
            return mapOf(
                argument1.first to argument1.second.scaleEncodeSerializable(),
                argument2.first to argument2.second.scaleEncodeSerializable(),
                argument3.first to argument3.second.scaleEncodeSerializable(),
                argument4.first to argument4.second.scaleEncodeSerializable(),
            ).intoEncodedArgs()
        }

        inline fun <reified T1, reified T2, reified T3, reified T4, reified T5> autoEncodedArgs(
            argument1: Pair<String, T1>,
            argument2: Pair<String, T2>,
            argument3: Pair<String, T3>,
            argument4: Pair<String, T4>,
            argument5: Pair<String, T5>,
        ): EncodedArguments {
            return mapOf(
                argument1.first to argument1.second.scaleEncodeSerializable(),
                argument2.first to argument2.second.scaleEncodeSerializable(),
                argument3.first to argument3.second.scaleEncodeSerializable(),
                argument4.first to argument4.second.scaleEncodeSerializable(),
                argument5.first to argument5.second.scaleEncodeSerializable(),
            ).intoEncodedArgs()
        }

        inline fun <reified T1, reified T2, reified T3, reified T4, reified T5, reified T6> autoEncodedArgs(
            argument1: Pair<String, T1>,
            argument2: Pair<String, T2>,
            argument3: Pair<String, T3>,
            argument4: Pair<String, T4>,
            argument5: Pair<String, T5>,
            argument6: Pair<String, T6>,
        ): EncodedArguments {
            return mapOf(
                argument1.first to argument1.second.scaleEncodeSerializable(),
                argument2.first to argument2.second.scaleEncodeSerializable(),
                argument3.first to argument3.second.scaleEncodeSerializable(),
                argument4.first to argument4.second.scaleEncodeSerializable(),
                argument5.first to argument5.second.scaleEncodeSerializable(),
                argument6.first to argument6.second.scaleEncodeSerializable(),
            ).intoEncodedArgs()
        }

        inline fun <reified T1, reified T2, reified T3, reified T4, reified T5, reified T6, reified T7> autoEncodedArgs(
            argument1: Pair<String, T1>,
            argument2: Pair<String, T2>,
            argument3: Pair<String, T3>,
            argument4: Pair<String, T4>,
            argument5: Pair<String, T5>,
            argument6: Pair<String, T6>,
            argument7: Pair<String, T7>,
        ): EncodedArguments {
            return mapOf(
                argument1.first to argument1.second.scaleEncodeSerializable(),
                argument2.first to argument2.second.scaleEncodeSerializable(),
                argument3.first to argument3.second.scaleEncodeSerializable(),
                argument4.first to argument4.second.scaleEncodeSerializable(),
                argument5.first to argument5.second.scaleEncodeSerializable(),
                argument6.first to argument6.second.scaleEncodeSerializable(),
                argument7.first to argument7.second.scaleEncodeSerializable(),
            ).intoEncodedArgs()
        }

        inline fun <reified T1, reified T2, reified T3, reified T4, reified T5, reified T6, reified T7, reified T8> autoEncodedArgs(
            argument1: Pair<String, T1>,
            argument2: Pair<String, T2>,
            argument3: Pair<String, T3>,
            argument4: Pair<String, T4>,
            argument5: Pair<String, T5>,
            argument6: Pair<String, T6>,
            argument7: Pair<String, T7>,
            argument8: Pair<String, T8>,
        ): EncodedArguments {
            return mapOf(
                argument1.first to argument1.second.scaleEncodeSerializable(),
                argument2.first to argument2.second.scaleEncodeSerializable(),
                argument3.first to argument3.second.scaleEncodeSerializable(),
                argument4.first to argument4.second.scaleEncodeSerializable(),
                argument5.first to argument5.second.scaleEncodeSerializable(),
                argument6.first to argument6.second.scaleEncodeSerializable(),
                argument7.first to argument7.second.scaleEncodeSerializable(),
                argument8.first to argument8.second.scaleEncodeSerializable(),
            ).intoEncodedArgs()
        }

        inline fun <reified T1, reified T2, reified T3, reified T4, reified T5, reified T6, reified T7, reified T8, reified T9> autoEncodedArgs(
            argument1: Pair<String, T1>,
            argument2: Pair<String, T2>,
            argument3: Pair<String, T3>,
            argument4: Pair<String, T4>,
            argument5: Pair<String, T5>,
            argument6: Pair<String, T6>,
            argument7: Pair<String, T7>,
            argument8: Pair<String, T8>,
            argument9: Pair<String, T9>,
        ): EncodedArguments {
            return mapOf(
                argument1.first to argument1.second.scaleEncodeSerializable(),
                argument2.first to argument2.second.scaleEncodeSerializable(),
                argument3.first to argument3.second.scaleEncodeSerializable(),
                argument4.first to argument4.second.scaleEncodeSerializable(),
                argument5.first to argument5.second.scaleEncodeSerializable(),
                argument6.first to argument6.second.scaleEncodeSerializable(),
                argument7.first to argument7.second.scaleEncodeSerializable(),
                argument8.first to argument8.second.scaleEncodeSerializable(),
                argument9.first to argument9.second.scaleEncodeSerializable(),
            ).intoEncodedArgs()
        }

        @PublishedApi
        internal fun Map<String, Any?>.intoEncodedArgs(): EncodedArguments {
            return EncodedArguments(this)
        }
    }
}
