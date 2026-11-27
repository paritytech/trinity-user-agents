// Stands in for the SDK a product ships, so a host's test page drives the
// injected transport the way a product does.
import { getClientSync, subscribeConnectionStatus } from '@parity/truapi/sandbox';

Object.assign(window, { truapiTestSdk: { getClientSync, subscribeConnectionStatus } });
