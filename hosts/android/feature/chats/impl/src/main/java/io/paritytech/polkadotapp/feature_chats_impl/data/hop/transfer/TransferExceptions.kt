package io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer

class RetryableTransferException(message: String) : Exception(message)

class TerminalTransferException(message: String) : Exception(message)

class TransferCancelledException : Exception("Transfer was cancelled")
