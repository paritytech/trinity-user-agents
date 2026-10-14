import type {
  TrUApiClient,
  HostContext,
  ScriptAssert,
} from "./script-types.d.ts";

declare const truapi: TrUApiClient;
declare const host: HostContext;
declare const assert: ScriptAssert;

const productContext = await truapi.system.getProductContext();
assert(productContext.isOk(), "getProductContext failed", productContext);
const productId: string = productContext.value.productId;
assert(productId.length > 0);

const userId = await truapi.account.getUserId();
if (userId.isErr() && userId.error.tag === "Cancelled") {
  const tag: "Cancelled" = userId.error.tag;
  assert(tag);
}

// @ts-expect-error Product context does not contain the signed-in user.
productContext.value.userId;

const subscription = truapi.locale.subscribe().subscribe({
  next(locale) {
    const languageTag: string = locale.languageTag;
    assert(languageTag.length > 0);

    // @ts-expect-error Locale updates contain a language tag, not a product id.
    locale.productId;
  },
  error(error) {
    if (error.reason?.tag === "HostFailure") {
      const reason: string = error.reason.value.reason;
      assert(reason.length > 0);
    }
  },
});
const subscriptionId: string = subscription.subscriptionId;
assert(subscriptionId.length > 0);
subscription.unsubscribe();

const account = host.productAccount(0);
const accountProductId: string = account.dotNsIdentifier;
assert(accountProductId.length > 0);

// @ts-expect-error Product accounts have no user-facing username.
account.username;

// @ts-expect-error Derivation indices are numeric.
host.productAccount("0");

// @ts-expect-error The generated client rejects unknown services.
truapi.unknownService;
