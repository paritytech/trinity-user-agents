## MELD Integration Setup

- Base URL: `https://api-sb.meld.io/`
- Auth: `Authorization: Basic <configuration.basicAuthToken>`
- Header on all requests: `Content-Type: application/json`
- Fiat currency: `USD`
- Country: `Locale.autoupdatingCurrent.region?.identifier ?? "US"`
- Asset context:
  - `cryptoChain` is currently hardcoded to `"DOT"`
  - `cryptoCurrency` is currently hardcoded to `"DOT"`
- Wallet address source: selected wallet account for the configured chain asset

## High-Level Sequence

1. User opens Fiat Onramp from token selection.
2. Amount screen loads and fetches purchase limits.
3. User enters amount and taps Continue.
4. Providers screen loads providers and then quotes.
5. User selects provider.
6. App creates MELD widget session.
7. App shows confirmation sheet for external URL.
8. User confirms, app opens provider widget in full-screen web modal.

## Endpoint Calls (In Order)

### 1) Fetch fiat purchase limits

- Method: `GET`
- Path: `/service-providers/limits/fiat-currency-purchases`
- Query params (comma-joined for arrays):
  - `statuses=LIVE,RECENTLY_ADDED`
  - `categories=CRYPTO_ONRAMP`
  - `accountFilter=false`
  - `countries=<countryCode>`
  - `fiatCurrencies=USD`
  - `cryptoChains=DOT`
  - `cryptoCurrencies=DOT`
  - `includeDetails=true`
- Used for:
  - amount min/max validation on amount screen
  - optional provider-specific amount constraints (`serviceProviderDetails`)

### 2) Fetch providers

- Trigger: `FiatOnRampProviderInteractor.setup()`
- Method: `GET`
- Path: `/service-providers`
- Query params:
  - `statuses=LIVE,RECENTLY_ADDED`
  - `categories=CRYPTO_ONRAMP`
  - `accountFilter=false`
  - `countries=<countryCode>`
  - `fiatCurrencies=USD`
  - `cryptoChains=DOT`
  - `cryptoCurrencies=DOT`
- Response mapped to provider list:
  - `id = serviceProvider`
  - `name = name`
  - `iconUrl = logos.darkShort`
- Once received:
  - Filter out providers which limits are not satisfied for the purchased amount.

### 3) Fetch quotes

- Trigger:
  - once right after providers are loaded
  - then every 30s while widget is not loading
- Method: `POST`
- Path: `/payments/crypto/quote`
- JSON body:

```json
{
  "countryCode": "<countryCode>",
  "destinationCurrencyCode": "DOT",
  "serviceProviders": ["<providerId1>", "<providerId2>"],
  "sourceAmount": "<Decimal amount from selected fiat input>",
  "sourceCurrencyCode": "USD",
  "walletAddress": "<selected wallet address>"
}
```

- Notes:
  - `serviceProviders` are provider IDs from step 2.
  - Quotes are sroted by `customerScore`, highest to lowest.

### 4) Create widget session

- Executed when user selects a provider
- Method: `POST`
- Path: `/crypto/session/widget`
- Generated values:
  - `sessionId = UUID().uuidString`
  - `redirectUrl = <appScheme>://fiatOnramp/buySuccess?sessionId=<sessionId>`
  - `externalCustomerId = blake2b32(walletAddress) hex`
  - `sessionType = "BUY"`
- JSON body:

```json
{
  "sessionData": {
    "countryCode": "<countryCode>",
    "destinationCurrencyCode": "DOT",
    "serviceProvider": "<selectedProviderId>",
    "sourceAmount": "<sourceAmount as string>",
    "sourceCurrencyCode": "USD",
    "walletAddress": "<selected wallet address>",
    "redirectUrl": "<appScheme>://fiatOnramp/buySuccess?sessionId=<sessionId>"
  },
  "sessionType": "BUY",
  "externalCustomerId": "<blake2b32(walletAddress) hex or null>",
  "externalSessionId": "<sessionId>"
}
```
- Response handling:
  - widget URL comes from `serviceProviderWidgetUrl` (fallback `widgetUrl`)
  - `sessionId` is stored in UserDefaults for polling.
