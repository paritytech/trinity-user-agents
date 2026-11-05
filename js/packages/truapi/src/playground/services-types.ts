export interface MethodInfo {
  name: string;
  type: "unary" | "subscription";
  /** Whether the host initiates this subscription into the product. */
  hostInitiated?: boolean;
  /** TS-shaped signature for the method (e.g. `getAccount(request: HostAccountGetRequest): Promise<…>`). */
  signature?: string;
  /** Cargo-doc URL fragment for this method (relative to the rustdoc root for the truapi crate). */
  docUrl?: string;
  description?: string;
  requestDescription?: string;
  exampleSource?: string;
  /** DataType id (kebab-case) of the method's request payload, when applicable. */
  requestType?: string;
  /** DataType id of the method's response. */
  responseType?: string;
  /** DataType id of the method's error. */
  errorType?: string;
}

/** Trusted executable kind required to access a generated service. */
export type ExecutableKind = "App" | "Widget" | "Worker";

export interface ServiceInfo {
  name: string;
  /**
   * Executable kind required by the host, or unrestricted when absent. Typed
   * as a string because the explorer's frozen per-version snapshots record the
   * kind each past protocol version declared, not today's variants.
   */
  requiredExecution?: string;
  methods: MethodInfo[];
}

/** Services visible to one trusted executable kind. */
export function servicesForExecution(
  services: ServiceInfo[],
  execution: ExecutableKind,
): ServiceInfo[] {
  return services.filter(
    (service) =>
      service.requiredExecution === undefined ||
      service.requiredExecution === execution,
  );
}
