import { services as generatedServices } from "@parity/truapi/playground/services";
import { servicesForExecution } from "@parity/truapi/playground/services-types";
import type {
  MethodInfo,
  ExecutableKind,
  ServiceInfo,
} from "@parity/truapi/playground/services-types";
import { WEBRTC_SERVICE } from "./webrtc-check";

export type { MethodInfo, ExecutableKind, ServiceInfo };
export { servicesForExecution };

// Generated App-compatible services plus the synthetic WebRTC browser-capability
// method, which is exercised the same way (an example) but is not a wire method.
export const services: ServiceInfo[] = [
  ...servicesForExecution(generatedServices, "App"),
  WEBRTC_SERVICE,
];
