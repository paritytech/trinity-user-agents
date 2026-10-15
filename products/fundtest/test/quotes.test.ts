import { describe, expect, test } from "bun:test";
import type { FundingQuoteAsk } from "@parity/truapi";
import {
  answerAsk,
  payoutOf,
  priceOf,
  quoteIdOf,
  termsOfQuoteId,
} from "../worker/quotes";

const NOW = 1_800_000_000_000;
const CASH = 1_000_000n;

const ask = (overrides: Partial<FundingQuoteAsk>): FundingQuoteAsk => ({
  direction: "In",
  rail: "Card",
  asset: "EUR",
  amount: 25n * CASH,
  country: "DE",
  ...overrides,
});

describe("answerAsk", () => {
  // The host shows these figures to the user, so they must add up: what is
  // paid is the base price plus both fees, in the unit the user pays in.
  test("prices a card top-up in euro cents at 0.99 EUR per CASH plus fees", () => {
    expect(answerAsk(ask({}), NOW)).toEqual({
      tag: "Quoted",
      value: {
        quote: {
          quoteId: quoteIdOf(
            { direction: "In", rail: "Card", asset: "EUR", amount: 25n * CASH },
            NOW + 60_000,
          ),
          sendAmount: 2_510n,
          receiveAmount: 25n * CASH,
          providerFee: 25n,
          networkFee: 10n,
          etaSecs: 300n,
          expiresAt: BigInt(NOW + 60_000),
          assetDecimals: 2,
        },
      },
    });
  });

  // For an outbound ask the fees are in balance units, the unit of what the
  // user sends, and the payout is what is left at the provider's rate.
  test("prices a bank withdrawal with the fees in balance units", () => {
    const answer = answerAsk(
      ask({ direction: "Out", rail: "Bank", amount: 100n * CASH }),
      NOW,
    );
    expect(answer.tag === "Quoted" && answer.value.quote).toMatchObject({
      sendAmount: 100n * CASH,
      providerFee: CASH,
      networkFee: 101_011n,
      receiveAmount: 9_790n,
      etaSecs: 172_800n,
    });
  });

  test("prices a PAS deposit at the demo rate of 4 CASH per PAS", () => {
    const answer = answerAsk(
      ask({ rail: "Crypto", asset: "PAS", amount: 20n * CASH, country: "US" }),
      NOW,
    );
    expect(answer.tag === "Quoted" && answer.value.quote).toMatchObject({
      sendAmount: 50_000_000_000n + 500_000_000n + 100_000_000n,
      etaSecs: 600n,
    });
  });

  // Every route moves on Asset Hub, so the host's network picker must not
  // offer another network as served by this provider.
  test("serves crypto only on the polkadot network", () => {
    const crypto = ask({ rail: "Crypto", asset: "USDT", country: undefined });
    expect(answerAsk({ ...crypto, network: "polkadot" }, NOW).tag).toBe(
      "Quoted",
    );
    expect(answerAsk({ ...crypto, network: "ethereum" }, NOW)).toEqual({
      tag: "Refused",
      value: {
        reason: {
          tag: "Other",
          value: { message: "only the polkadot network is served" },
        },
      },
    });
  });

  // The excluded country is what exercises CountryUnsupported in the host;
  // crypto declares no countries and stays open there.
  test("refuses a fiat rail outside its countries", () => {
    expect(answerAsk(ask({ country: "US" }), NOW)).toEqual({
      tag: "Refused",
      value: { reason: { tag: "CountryUnsupported" } },
    });
    expect(answerAsk(ask({ country: "de" }), NOW).tag).toBe("Quoted");
  });

  test("refuses amounts outside 10 to 5000 CASH with the bound", () => {
    expect(answerAsk(ask({ amount: 10n * CASH - 1n }), NOW)).toEqual({
      tag: "Refused",
      value: { reason: { tag: "BelowMinimum", value: { min: 10n * CASH } } },
    });
    expect(answerAsk(ask({ amount: 5_000n * CASH + 1n }), NOW)).toEqual({
      tag: "Refused",
      value: { reason: { tag: "AboveMaximum", value: { max: 5_000n * CASH } } },
    });
  });

  test("refuses an asset the rail does not take", () => {
    expect(answerAsk(ask({ asset: "USDT" }), NOW).tag).toBe("Refused");
    expect(answerAsk(ask({ rail: "Crypto", asset: "DOT" }), NOW).tag).toBe(
      "Refused",
    );
  });
});

describe("quote ids", () => {
  // An assignment carries only the quote, so the session reads its rail and
  // asset back from the id.
  test("carry the terms they were given for", () => {
    const terms = {
      direction: "Out",
      rail: "Crypto",
      asset: "USDC",
      amount: 42n * CASH,
    } as const;
    expect(termsOfQuoteId(quoteIdOf(terms, NOW))).toEqual(terms);
  });

  test("from another provider read as none", () => {
    expect(termsOfQuoteId("q-123")).toBeUndefined();
    expect(termsOfQuoteId("ft1.In.Wire.EUR.1.2")).toBeUndefined();
  });

  test("price what the quote asked for", () => {
    expect(
      priceOf({
        direction: "In",
        rail: "Bank",
        asset: "EUR",
        amount: 25n * CASH,
      }),
    ).toBe(2_510n);
  });

  // The payout screen shows what the user receives, so it must match the
  // outbound quote the user chose, fees taken out rather than added.
  test("pay out what the outbound quote receives", () => {
    const answer = answerAsk(
      ask({ direction: "Out", rail: "Bank", amount: 100n * CASH }),
      NOW,
    );
    expect(
      payoutOf({
        direction: "Out",
        rail: "Bank",
        asset: "EUR",
        amount: 100n * CASH,
      }),
    ).toBe(answer.tag === "Quoted" ? answer.value.quote.receiveAmount : -1n);
    expect(
      payoutOf({
        direction: "Out",
        rail: "Bank",
        asset: "EUR",
        amount: 100n * CASH,
      }),
    ).toBe(9_790n);
  });
});
