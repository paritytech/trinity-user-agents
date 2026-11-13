/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly FUNDTEST_DEMO_CRYPTO?: string;
  readonly FUNDTEST_CARD_IN_AUTHORISE_MS?: string;
  readonly FUNDTEST_CARD_IN_RECEIVE_MS?: string;
  readonly FUNDTEST_CARD_IN_CONFIRM_MS?: string;
  readonly FUNDTEST_CARD_IN_CONVERT_MS?: string;
  readonly FUNDTEST_CARD_IN_CREDIT_MS?: string;
  readonly FUNDTEST_CARD_IN_DELIVER_MS?: string;
  readonly FUNDTEST_BANK_IN_RECEIVE_MS?: string;
  readonly FUNDTEST_BANK_IN_CONFIRM_MS?: string;
  readonly FUNDTEST_BANK_IN_CONVERT_MS?: string;
  readonly FUNDTEST_BANK_IN_CREDIT_MS?: string;
  readonly FUNDTEST_BANK_IN_DELIVER_MS?: string;
  readonly FUNDTEST_CRYPTO_IN_RECEIVE_MS?: string;
  readonly FUNDTEST_CRYPTO_IN_CONFIRM_MS?: string;
  readonly FUNDTEST_CRYPTO_IN_CONVERT_MS?: string;
  readonly FUNDTEST_CRYPTO_IN_CREDIT_MS?: string;
  readonly FUNDTEST_CRYPTO_IN_DELIVER_MS?: string;
  readonly FUNDTEST_CARD_OUT_CONVERT_MS?: string;
  readonly FUNDTEST_CARD_OUT_SEND_MS?: string;
  readonly FUNDTEST_BANK_OUT_CONVERT_MS?: string;
  readonly FUNDTEST_BANK_OUT_SEND_MS?: string;
  readonly FUNDTEST_CRYPTO_OUT_CONVERT_MS?: string;
  readonly FUNDTEST_CRYPTO_OUT_SEND_MS?: string;
  readonly FUNDTEST_CRYPTO_IN_CONFIRMATIONS?: string;
  readonly FUNDTEST_BANK_ETA_SECS?: string;
  readonly FUNDTEST_REPORT_RETRY_MS?: string;
}
