/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly FUNDTEST_FAUCET_SEED?: string;
  readonly FUNDTEST_DEMO_CRYPTO?: string;
  readonly FUNDTEST_CARD_ARRIVAL_MS?: string;
  readonly FUNDTEST_BANK_ARRIVAL_MS?: string;
  readonly FUNDTEST_CRYPTO_ARRIVAL_MS?: string;
  readonly FUNDTEST_PAYOUT_MS?: string;
}
