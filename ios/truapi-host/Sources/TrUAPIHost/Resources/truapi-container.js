"use strict";
(() => {
  var __defProp = Object.defineProperty;
  var __typeError = (msg) => {
    throw TypeError(msg);
  };
  var __defNormalProp = (obj, key, value) => key in obj ? __defProp(obj, key, { enumerable: true, configurable: true, writable: true, value }) : obj[key] = value;
  var __export = (target, all) => {
    for (var name in all)
      __defProp(target, name, { get: all[name], enumerable: true });
  };
  var __publicField = (obj, key, value) => __defNormalProp(obj, typeof key !== "symbol" ? key + "" : key, value);
  var __accessCheck = (obj, member, msg) => member.has(obj) || __typeError("Cannot " + msg);
  var __privateGet = (obj, member, getter2) => (__accessCheck(obj, member, "read from private field"), getter2 ? getter2.call(obj) : member.get(obj));
  var __privateAdd = (obj, member, value) => member.has(obj) ? __typeError("Cannot add the same private member more than once") : member instanceof WeakSet ? member.add(obj) : member.set(obj, value);
  var __privateSet = (obj, member, value, setter) => (__accessCheck(obj, member, "write to private field"), setter ? setter.call(obj, value) : member.set(obj, value), value);

  // src/freeze.ts
  var failures = [];
  function describe(obj) {
    if (obj === globalThis) return "window";
    const name = obj?.constructor?.name;
    return typeof name === "string" && name.length > 0 ? name : "object";
  }
  function recordFailure(obj, prop) {
    failures.push(`${describe(obj)}.${prop}`);
  }
  function freezeAndDelete(obj, prop) {
    try {
      Object.defineProperty(obj, prop, {
        get: () => void 0,
        set() {
        },
        configurable: false
      });
    } catch {
      try {
        delete obj[prop];
      } catch {
      }
    }
    if (obj?.[prop] !== void 0) {
      recordFailure(obj, prop);
    }
  }
  function freezeValue(obj, prop, value) {
    try {
      Object.defineProperty(obj, prop, {
        get: () => value,
        set() {
        },
        configurable: false
      });
    } catch {
    }
    if (obj?.[prop] !== value) {
      recordFailure(obj, prop);
    }
  }
  function freezeCustom(obj, prop, descriptor, verify) {
    try {
      Object.defineProperty(obj, prop, { configurable: false, ...descriptor });
    } catch {
    }
    let locked = false;
    try {
      locked = verify(obj?.[prop]);
    } catch {
    }
    if (!locked) {
      recordFailure(obj, prop);
    }
  }
  function reportLockdownFailures() {
    if (failures.length === 0) {
      return;
    }
    const message = `TrUAPI container lockdown failed for: ${failures.join(", ")}`;
    try {
      console.error(message);
    } catch {
    }
    throw new Error(message);
  }

  // src/webrtc.ts
  function installWebRtcPolicy(win, authorize) {
    const aliases = [
      "RTCPeerConnection",
      "webkitRTCPeerConnection",
      "mozRTCPeerConnection"
    ];
    if (typeof authorize !== "function") {
      for (const name of aliases) freezeAndDelete(win, name);
      return;
    }
    const apply = Reflect.apply;
    const construct = Reflect.construct;
    const get = Reflect.get;
    const define = Object.defineProperty;
    const NativePromise = win.Promise;
    const NativeError = win.TypeError;
    const NativeProxy = Proxy;
    const finite = Number.isFinite;
    const truncate = Math.trunc;
    const states = /* @__PURE__ */ new WeakMap();
    const weakGet = WeakMap.prototype.get;
    const weakSet = WeakMap.prototype.set;
    const installed = /* @__PURE__ */ new Map();
    function state(connection2) {
      const value = apply(weakGet, states, [connection2]);
      if (!value) throw new NativeError("Invalid RTCPeerConnection receiver");
      return value;
    }
    function configuration(input) {
      const result = { value: input, pool: 0 };
      if (input == null) return result;
      if (typeof input !== "object" && typeof input !== "function") {
        throw new NativeError("RTCConfiguration must be an object");
      }
      result.value = new NativeProxy(
        {},
        {
          get(_target, name) {
            const value = get(input, name, input);
            if (name !== "iceCandidatePoolSize") return value;
            const number = value === void 0 ? 0 : +value;
            const pool = truncate(number);
            if (!finite(number) || pool < 0 || pool > 255) {
              throw new NativeError(
                "iceCandidatePoolSize must be between 0 and 255"
              );
            }
            result.pool = pool;
            return 0;
          }
        }
      );
      return result;
    }
    function withPool(value, pool) {
      define(value, "iceCandidatePoolSize", {
        value: pool,
        writable: true,
        enumerable: true,
        configurable: true
      });
      return value;
    }
    function drain(connection2, current) {
      let call = current.head;
      current.head = null;
      current.tail = null;
      while (call) {
        try {
          if (current.phase === "allowed") {
            call.resolve(apply(call.method, connection2, call.args));
          } else {
            const error = new NativeError(
              current.phase === "closed" ? "WebRTC connection is closed" : "WebRTC access is not allowed"
            );
            if (call.errorCallback) {
              apply(call.errorCallback, void 0, [error]);
              call.resolve(void 0);
            } else {
              call.reject(error);
            }
          }
        } catch (error) {
          call.reject(error);
        }
        call = call.next;
      }
    }
    for (const alias of aliases) {
      let finish2 = function(connection2, current, allowed) {
        if (current.phase !== "pending") return;
        current.phase = allowed === true ? "allowed" : "denied";
        current.cancel?.();
        current.cancel = null;
        try {
          if (current.phase === "allowed" && current.pool !== 0) {
            apply(nativeSetConfiguration, connection2, [
              withPool(
                apply(nativeGetConfiguration, connection2, []),
                current.pool
              )
            ]);
          } else if (current.phase === "denied") {
            apply(nativeClose, connection2, []);
          }
        } catch {
          current.phase = "denied";
          try {
            apply(nativeClose, connection2, []);
          } catch {
          }
        }
        drain(connection2, current);
      };
      var finish = finish2;
      const Native = win[alias];
      if (typeof Native !== "function") continue;
      const previous = installed.get(Native) ?? installed.get(Native.prototype);
      if (previous) {
        freezeValue(win, alias, previous);
        continue;
      }
      const prototype = Native.prototype;
      const nativeClose = prototype.close;
      const nativeGetConfiguration = prototype.getConfiguration;
      const nativeSetConfiguration = prototype.setConfiguration;
      const Guarded = new NativeProxy(Native, {
        construct(_target, args, newTarget) {
          const requested = configuration(args[0]);
          define(args, "0", {
            value: requested.value,
            writable: true,
            enumerable: true,
            configurable: true
          });
          const connection2 = construct(Native, args, newTarget);
          apply(weakSet, states, [
            connection2,
            {
              phase: "idle",
              pool: requested.pool,
              head: null,
              tail: null,
              cancel: null
            }
          ]);
          return connection2;
        }
      });
      freezeValue(prototype, "constructor", Guarded);
      for (const method of [
        "createOffer",
        "createAnswer",
        "setLocalDescription",
        "setRemoteDescription",
        "addIceCandidate"
      ]) {
        const nativeMethod = prototype[method];
        freezeValue(prototype, method, function(...args) {
          return new NativePromise(
            (resolve, reject) => {
              const current = state(this);
              const callbackIndex = method === "createOffer" || method === "createAnswer" ? 0 : 1;
              const errorCallback = typeof args[callbackIndex] === "function" && typeof args[callbackIndex + 1] === "function" ? args[callbackIndex + 1] : void 0;
              const call = {
                method: nativeMethod,
                args,
                errorCallback,
                resolve,
                reject,
                next: null
              };
              if (current.tail) current.tail.next = call;
              else current.head = call;
              current.tail = call;
              if (current.phase !== "idle" && current.phase !== "pending") {
                drain(this, current);
                return;
              }
              if (current.phase === "pending") return;
              current.phase = "pending";
              try {
                const cancel = authorize(
                  (allowed) => finish2(this, current, allowed)
                );
                if (current.phase === "pending") current.cancel = cancel;
                else cancel();
              } catch {
                finish2(this, current, false);
              }
            }
          );
        });
      }
      freezeValue(
        prototype,
        "setConfiguration",
        function(input) {
          const current = state(this);
          if (current.phase === "allowed")
            return apply(nativeSetConfiguration, this, [input]);
          const requested = configuration(input);
          const result = apply(nativeSetConfiguration, this, [requested.value]);
          current.pool = requested.pool;
          return result;
        }
      );
      freezeValue(prototype, "getConfiguration", function() {
        const current = state(this);
        const value = apply(nativeGetConfiguration, this, []);
        return current.phase === "allowed" ? value : withPool(value, current.pool);
      });
      freezeValue(prototype, "close", function() {
        const current = state(this);
        current.phase = "closed";
        current.cancel?.();
        current.cancel = null;
        try {
          return apply(nativeClose, this, []);
        } finally {
          drain(this, current);
        }
      });
      installed.set(Native, Guarded);
      installed.set(prototype, Guarded);
      freezeValue(win, alias, Guarded);
    }
  }

  // src/network.ts
  function getter(prototype, name) {
    return Object.getOwnPropertyDescriptor(prototype, name).get;
  }
  function installFetchGate(win, authorize) {
    const nativeFetch = win.fetch.bind(win);
    const NativeRequest = win.Request;
    const NativeURL = win.URL;
    const NativePromise = win.Promise;
    const NetworkError = win.TypeError;
    const apply = Reflect.apply;
    const requestUrl = getter(NativeRequest.prototype, "url");
    const requestSignal = getter(NativeRequest.prototype, "signal");
    const urlOrigin = getter(NativeURL.prototype, "origin");
    const urlProtocol = getter(NativeURL.prototype, "protocol");
    const urlHost = getter(NativeURL.prototype, "host");
    const signalAborted = getter(win.AbortSignal.prototype, "aborted");
    const signalReason = getter(win.AbortSignal.prototype, "reason");
    const addEventListener = win.EventTarget.prototype.addEventListener;
    const removeEventListener = win.EventTarget.prototype.removeEventListener;
    function origin(url) {
      const value = apply(urlOrigin, url, []);
      return value === "null" ? `${apply(urlProtocol, url, [])}//${apply(urlHost, url, [])}` : value;
    }
    const productOrigin = win.location && origin(new NativeURL(win.location.href));
    freezeValue(
      win,
      "fetch",
      (input, init) => new NativePromise((resolve, reject) => {
        let signal;
        let settled = false;
        let cancelAuthorization;
        function finish() {
          settled = true;
          cancelAuthorization?.();
          if (signal) apply(removeEventListener, signal, ["abort", abort]);
        }
        function deny() {
          finish();
          reject(new NetworkError("Network access is not allowed"));
        }
        function abort() {
          if (settled) return;
          finish();
          reject(apply(signalReason, signal, []));
        }
        try {
          let authorized2 = function(allowed) {
            if (settled) return;
            if (allowed !== true) {
              deny();
              return;
            }
            finish();
            try {
              resolve(nativeFetch(request));
            } catch (error) {
              reject(error);
            }
          };
          var authorized = authorized2;
          const request = new NativeRequest(input, init);
          const destination = apply(requestUrl, request, []);
          const url = new NativeURL(destination);
          const sameOrigin = origin(url) === productOrigin;
          const protocol = apply(urlProtocol, url, []);
          if (!sameOrigin && protocol !== "http:" && protocol !== "https:") {
            deny();
            return;
          }
          signal = apply(requestSignal, request, []);
          if (apply(signalAborted, signal, [])) {
            abort();
            return;
          }
          apply(addEventListener, signal, ["abort", abort]);
          if (sameOrigin) authorized2(true);
          else cancelAuthorization = authorize(destination, authorized2);
        } catch (error) {
          if (signal) deny();
          else reject(error);
        }
      })
    );
  }

  // src/xhr.ts
  function installXhrGate(win, authorize) {
    const NativeXhr = win.XMLHttpRequest;
    if (!NativeXhr) return;
    const prototype = NativeXhr.prototype;
    const apply = Reflect.apply;
    const descriptor = Object.getOwnPropertyDescriptor;
    const nativeOpen = prototype.open;
    const nativeSend = prototype.send;
    const nativeAbort = prototype.abort;
    const nativeHeader = prototype.setRequestHeader;
    const nativeMime = prototype.overrideMimeType;
    const ready = descriptor(prototype, "readyState").get;
    const upload = descriptor(prototype, "upload").get;
    const timeout = descriptor(prototype, "timeout");
    const credentials = descriptor(prototype, "withCredentials");
    const responseType = descriptor(prototype, "responseType");
    const dispatch = win.EventTarget.prototype.dispatchEvent;
    const NativeEvent = win.Event;
    const NativeProgress = win.ProgressEvent;
    const NativeError = win.DOMException;
    const NativeTypeError = win.TypeError;
    const NativeURL = win.URL;
    const href = descriptor(NativeURL.prototype, "href").get;
    const origin = descriptor(NativeURL.prototype, "origin").get;
    const protocol = descriptor(NativeURL.prototype, "protocol").get;
    const host = descriptor(NativeURL.prototype, "host").get;
    const baseURI = win.Node && descriptor(win.Node.prototype, "baseURI")?.get;
    const uppercase = String.prototype.toUpperCase;
    const codeUnit = String.prototype.charCodeAt;
    const stringify = String;
    const states = /* @__PURE__ */ new WeakMap();
    const weakGet = WeakMap.prototype.get;
    const weakSet = WeakMap.prototype.set;
    const now = win.performance.now.bind(win.performance);
    const schedule = win.setTimeout.bind(win);
    const unschedule = win.clearTimeout.bind(win);
    const maximum = Math.max;
    const NativeBytes = win.Uint8Array;
    const bufferLength = descriptor(
      win.ArrayBuffer.prototype,
      "byteLength"
    ).get;
    const viewPrototype = Object.getPrototypeOf(NativeBytes.prototype);
    const viewBuffer = descriptor(viewPrototype, "buffer").get;
    const viewOffset = descriptor(viewPrototype, "byteOffset").get;
    const viewLength = descriptor(viewPrototype, "byteLength").get;
    const dataBuffer = descriptor(win.DataView.prototype, "buffer").get;
    const dataOffset = descriptor(win.DataView.prototype, "byteOffset").get;
    const dataLength = descriptor(win.DataView.prototype, "byteLength").get;
    const isView = win.ArrayBuffer.isView;
    const blobSize = descriptor(win.Blob.prototype, "size").get;
    const NativeForm = win.FormData;
    const formEach = NativeForm.prototype.forEach;
    const formAppend = NativeForm.prototype.append;
    const NativeParams = win.URLSearchParams;
    const paramsString = NativeParams.prototype.toString;
    const nodeType = win.Node && descriptor(win.Node.prototype, "nodeType")?.get;
    const cloneNode = win.Node?.prototype.cloneNode;
    function lockAccessor(name, get, set) {
      const marker = {};
      let verifying = true;
      freezeCustom(
        prototype,
        name,
        {
          get() {
            if (verifying && this === prototype) return marker;
            return apply(get, this, []);
          },
          set
        },
        (value) => value === marker
      );
      verifying = false;
    }
    function request(xhr) {
      apply(ready, xhr, []);
      return apply(weakGet, states, [xhr]);
    }
    function current(xhr, state) {
      return apply(weakGet, states, [xhr]) === state;
    }
    function invalid() {
      throw new NativeError(
        "The request is not open or has already been sent",
        "InvalidStateError"
      );
    }
    function byteString(value) {
      if (typeof value === "symbol")
        throw new NativeTypeError("Cannot convert a Symbol to a string");
      const text = stringify(value);
      for (let index = 0; index < text.length; index++) {
        if (apply(codeUnit, text, [index]) > 255)
          throw new NativeTypeError(
            "ByteString contains a character outside the byte range"
          );
      }
      return text;
    }
    function sendable(xhr, state) {
      if (!state || !current(xhr, state) || state.pending || state.nativeStarted || state.overrideState !== void 0 || apply(ready, xhr, []) !== 1)
        invalid();
    }
    function urlOrigin(url) {
      const value = apply(origin, url, []);
      return value === "null" ? `${apply(protocol, url, [])}//${apply(host, url, [])}` : value;
    }
    const productOrigin = urlOrigin(new NativeURL(win.location.href));
    function cancel(state) {
      state.pending = false;
      state.cancel?.();
      state.cancel = void 0;
      if (state.deadline !== void 0) unschedule(state.deadline);
      state.deadline = void 0;
    }
    function fail(xhr, state, type) {
      if (!current(xhr, state) || !state.pending) return;
      cancel(state);
      apply(nativeAbort, xhr, []);
      state.overrideState = 4;
      apply(dispatch, xhr, [new NativeEvent("readystatechange")]);
      if (!current(xhr, state)) return;
      if (state.body) {
        const target = apply(upload, xhr, []);
        apply(dispatch, target, [new NativeProgress(type)]);
        if (!current(xhr, state)) return;
        apply(dispatch, target, [new NativeProgress("loadend")]);
        if (!current(xhr, state)) return;
      }
      apply(dispatch, xhr, [new NativeProgress(type)]);
      if (current(xhr, state))
        apply(dispatch, xhr, [new NativeProgress("loadend")]);
    }
    function deadline(xhr, state) {
      if (state.deadline !== void 0) unschedule(state.deadline);
      state.deadline = void 0;
      if (state.timeout !== 0) {
        state.deadline = schedule(
          () => fail(xhr, state, "timeout"),
          maximum(0, state.timeout - (now() - state.startedAt))
        );
      }
    }
    function copyBytes(buffer, offset, length) {
      apply(bufferLength, buffer, []);
      const source = new NativeBytes(buffer, offset, length);
      const copy = new NativeBytes(length);
      for (let index = 0; index < length; index++) copy[index] = source[index];
      return copy;
    }
    function snapshot(body) {
      if (body === void 0 || body === null) return null;
      if (isView(body)) {
        try {
          return copyBytes(
            apply(viewBuffer, body, []),
            apply(viewOffset, body, []),
            apply(viewLength, body, [])
          );
        } catch {
          return copyBytes(
            apply(dataBuffer, body, []),
            apply(dataOffset, body, []),
            apply(dataLength, body, [])
          );
        }
      }
      let length;
      try {
        length = apply(bufferLength, body, []);
      } catch {
      }
      if (length !== void 0) return copyBytes(body, 0, length);
      try {
        apply(blobSize, body, []);
        return body;
      } catch {
      }
      if (nodeType && cloneNode) {
        let document2 = false;
        try {
          document2 = apply(nodeType, body, []) === 9;
        } catch {
        }
        if (document2) return apply(cloneNode, body, [true]);
      }
      try {
        const copy = new NativeForm();
        apply(formEach, body, [
          (value, name) => {
            apply(formAppend, copy, [name, value]);
          }
        ]);
        return copy;
      } catch {
      }
      try {
        return new NativeParams(apply(paramsString, body, []));
      } catch {
      }
      if (typeof body === "symbol")
        throw new NativeTypeError("Cannot convert a Symbol to a string");
      return stringify(body);
    }
    freezeValue(
      prototype,
      "open",
      function(method, url, ...rest) {
        request(this);
        method = byteString(method);
        if (typeof url === "symbol")
          throw new NativeTypeError("Cannot convert a Symbol to a string");
        const inputUrl = stringify(url);
        function credential(index) {
          const value = rest.length > index ? rest[index] : null;
          if (value === null || value === void 0) return null;
          if (typeof value === "symbol")
            throw new NativeTypeError("Cannot convert a Symbol to a string");
          return stringify(value);
        }
        const username = credential(1);
        const password = credential(2);
        let destination;
        try {
          destination = new NativeURL(
            inputUrl,
            baseURI ? apply(baseURI, win.document, []) : win.document.baseURI
          );
        } catch {
          throw new NativeError("Invalid XMLHttpRequest URL", "SyntaxError");
        }
        if (rest.length && !rest[0]) {
          throw new NativeError(
            "synchronous XMLHttpRequest is not supported",
            "InvalidAccessError"
          );
        }
        const previous = request(this);
        const oldTimeout = apply(timeout.get, this, []);
        const oldState = apply(ready, this, []);
        const state = {
          url: apply(href, destination, []),
          method: apply(uppercase, method, []),
          sameOrigin: urlOrigin(destination) === productOrigin,
          supported: apply(protocol, destination, []) === "http:" || apply(protocol, destination, []) === "https:",
          pending: false,
          nativeStarted: false,
          body: false,
          overrideState: void 0,
          startedAt: 0,
          waited: 0,
          timeout: previous?.timeout ?? oldTimeout,
          deadline: void 0,
          cancel: void 0
        };
        apply(weakSet, states, [this, state]);
        try {
          apply(timeout.set, this, [state.timeout]);
          apply(nativeOpen, this, [method, state.url, true, username, password]);
        } catch (error) {
          apply(weakSet, states, [this, previous]);
          apply(timeout.set, this, [oldTimeout]);
          throw error;
        }
        if (previous) cancel(previous);
        if (current(this, state) && oldState === 1 && previous?.overrideState !== void 0)
          apply(dispatch, this, [new NativeEvent("readystatechange")]);
      }
    );
    freezeValue(prototype, "send", function(body) {
      const state = request(this);
      sendable(this, state);
      const payload = state.method === "GET" || state.method === "HEAD" ? null : snapshot(body);
      sendable(this, state);
      state.pending = true;
      state.body = payload !== null;
      state.startedAt = now();
      deadline(this, state);
      let sending = true;
      const failSend = (type) => {
        if (sending) schedule(() => fail(this, state, type), 0);
        else fail(this, state, type);
      };
      const decided = (allowed) => {
        if (!current(this, state) || !state.pending) return;
        if (allowed !== true) return failSend("error");
        state.waited = now() - state.startedAt;
        if (state.timeout && state.waited >= state.timeout)
          return failSend("timeout");
        cancel(state);
        state.nativeStarted = true;
        apply(timeout.set, this, [
          state.timeout ? maximum(1, state.timeout - state.waited) : 0
        ]);
        try {
          apply(nativeSend, this, [payload]);
        } catch {
          state.pending = true;
          state.nativeStarted = false;
          failSend("error");
        }
      };
      if (state.sameOrigin) decided(true);
      else if (!state.supported) decided(false);
      else {
        try {
          const cancellation = authorize(state.url, decided);
          if (!state.pending) cancellation();
          else state.cancel = cancellation;
        } catch {
          failSend("error");
        }
      }
      sending = false;
    });
    freezeValue(prototype, "abort", function() {
      const state = request(this);
      if (state?.pending) {
        fail(this, state, "abort");
        if (current(this, state) && state.overrideState === 4)
          state.overrideState = 0;
      } else {
        if (state?.overrideState !== void 0) state.overrideState = 0;
        apply(nativeAbort, this, []);
      }
    });
    freezeValue(
      prototype,
      "setRequestHeader",
      function(...args) {
        if (args.length < 2)
          throw new NativeTypeError("setRequestHeader requires two arguments");
        const name = byteString(args[0]);
        const value = byteString(args[1]);
        const state = request(this);
        if (state?.pending || state?.overrideState !== void 0) invalid();
        return apply(nativeHeader, this, [name, value]);
      }
    );
    freezeValue(
      prototype,
      "overrideMimeType",
      function(...args) {
        if (request(this)?.overrideState === 4) invalid();
        return apply(nativeMime, this, args);
      }
    );
    lockAccessor("readyState", function() {
      const state = request(this);
      return state?.overrideState ?? apply(ready, this, []);
    });
    lockAccessor(
      "timeout",
      function() {
        return request(this)?.timeout ?? apply(timeout.get, this, []);
      },
      function(value) {
        apply(timeout.set, this, [value]);
        const state = request(this);
        if (!state) return;
        state.timeout = apply(timeout.get, this, []);
        if (state.pending) deadline(this, state);
        else if (state.nativeStarted && apply(ready, this, []) !== 4)
          apply(timeout.set, this, [
            state.timeout ? maximum(1, state.timeout - state.waited) : 0
          ]);
      }
    );
    lockAccessor(
      "withCredentials",
      credentials.get,
      function(value) {
        const state = request(this);
        if (state?.pending || state?.overrideState === 4) invalid();
        apply(credentials.set, this, [value]);
      }
    );
    lockAccessor(
      "responseType",
      responseType.get,
      function(value) {
        if (request(this)?.overrideState === 4) invalid();
        apply(responseType.set, this, [value]);
      }
    );
    freezeValue(prototype, "constructor", NativeXhr);
    freezeValue(win, "XMLHttpRequest", NativeXhr);
  }

  // src/websocket.ts
  function installWebSocketGate(win, authorize, factory) {
    const NativeSocket = win.WebSocket;
    if (!NativeSocket) return;
    const apply = Reflect.apply;
    const descriptor = Object.getOwnPropertyDescriptor;
    const define = Object.defineProperty;
    const getPrototype = Object.getPrototypeOf;
    const freeze = Object.freeze;
    const create = Object.create;
    const owns = Object.prototype.hasOwnProperty;
    const nativePrototype = NativeSocket.prototype;
    const nativeSend = nativePrototype.send;
    const nativeClose = nativePrototype.close;
    const nativeProperties = create(null);
    const propertyNames = [
      "readyState",
      "bufferedAmount",
      "extensions",
      "protocol",
      "binaryType"
    ];
    for (const name of propertyNames)
      nativeProperties[name] = descriptor(nativePrototype, name);
    const NativeTarget = win.EventTarget;
    const add = NativeTarget.prototype.addEventListener;
    const remove = NativeTarget.prototype.removeEventListener;
    const dispatch = NativeTarget.prototype.dispatchEvent;
    const NativeEvent = win.Event;
    const NativeMessage = win.MessageEvent;
    const NativeClose = win.CloseEvent;
    const messageData = descriptor(NativeMessage.prototype, "data").get;
    const messageOrigin = descriptor(NativeMessage.prototype, "origin").get;
    const messageId = descriptor(NativeMessage.prototype, "lastEventId").get;
    const closeCode = descriptor(NativeClose.prototype, "code").get;
    const closeReason = descriptor(NativeClose.prototype, "reason").get;
    const closeClean = descriptor(NativeClose.prototype, "wasClean").get;
    const NativeError = win.DOMException;
    const NativeTypeError = win.TypeError;
    const NativeURL = win.URL;
    const href = descriptor(NativeURL.prototype, "href").get;
    const scheme = descriptor(NativeURL.prototype, "protocol");
    const baseURI = win.Node && descriptor(win.Node.prototype, "baseURI")?.get;
    const stringify = String;
    const indexOf = String.prototype.indexOf;
    const test = RegExp.prototype.test;
    const protocolToken = /^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/;
    const iterator = Symbol.iterator;
    const states = /* @__PURE__ */ new WeakMap();
    const weakGet = WeakMap.prototype.get;
    const weakSet = WeakMap.prototype.set;
    const schedule = win.setTimeout.bind(win);
    const encoder = new win.TextEncoder();
    const encode = win.TextEncoder.prototype.encode;
    const bufferLength = descriptor(
      win.ArrayBuffer.prototype,
      "byteLength"
    ).get;
    const viewPrototype = getPrototype(win.Uint8Array.prototype);
    const viewLength = descriptor(viewPrototype, "byteLength").get;
    const dataLength = descriptor(win.DataView.prototype, "byteLength").get;
    const blobSize = descriptor(win.Blob.prototype, "size").get;
    const isView = win.ArrayBuffer.isView;
    const floor = Math.floor;
    const min = Math.min;
    const max = Math.max;
    function state(socket) {
      const value = apply(weakGet, states, [socket]);
      if (!value) throw new NativeTypeError("Illegal WebSocket receiver");
      return value;
    }
    function text(value) {
      if (typeof value === "symbol")
        throw new NativeTypeError("Cannot convert a Symbol to a string");
      return stringify(value);
    }
    function protocols(value) {
      const result = [];
      if (value !== void 0) {
        const method = value !== null && (typeof value === "object" || typeof value === "function") ? value[iterator] : void 0;
        if (method !== void 0 && method !== null) {
          const sequence = apply(method, value, []);
          if (sequence === null || typeof sequence !== "object" && typeof sequence !== "function")
            throw new NativeTypeError("Invalid WebSocket protocol iterator");
          const next = sequence.next;
          while (true) {
            const item = apply(next, sequence, []);
            if (item === null || typeof item !== "object" && typeof item !== "function")
              throw new NativeTypeError(
                "Invalid WebSocket protocol iterator result"
              );
            if (item.done) break;
            result[result.length] = text(item.value);
          }
        } else result[0] = text(value);
      }
      for (let index = 0; index < result.length; index++) {
        const protocol = result[index];
        if (!apply(test, protocolToken, [protocol]))
          throw new NativeError("Invalid WebSocket protocol", "SyntaxError");
        for (let previous = 0; previous < index; previous++) {
          if (result[previous] === protocol)
            throw new NativeError("Duplicate WebSocket protocol", "SyntaxError");
        }
      }
      const iteration = create(null);
      iteration.value = function() {
        let index = 0;
        return {
          next() {
            return index < result.length ? { value: result[index++], done: false } : { value: void 0, done: true };
          }
        };
      };
      define(result, iterator, iteration);
      freeze(result);
      return result;
    }
    function cancel(current) {
      current.pending = false;
      current.cancel?.();
      current.cancel = void 0;
    }
    function fail(socket, current) {
      cancel(current);
      current.phase = 2;
      schedule(() => {
        current.phase = 3;
        apply(dispatch, socket, [new NativeEvent("error")]);
        apply(dispatch, socket, [
          new NativeClose("close", {
            code: 1006,
            reason: "",
            wasClean: false
          })
        ]);
      }, 0);
    }
    function connect(socket, current, requested) {
      const backend = factory ? factory(current.url, requested) : new NativeSocket(current.url, requested);
      const properties = create(null);
      for (let index = 0; index < propertyNames.length; index++) {
        const name = propertyNames[index];
        if (!factory) properties[name] = nativeProperties[name];
        else {
          let object = backend;
          while (object && !properties[name]) {
            properties[name] = descriptor(object, name);
            object = getPrototype(object);
          }
        }
      }
      const send = factory ? backend.send : nativeSend;
      const close = factory ? backend.close : nativeClose;
      current.backend = {
        read(name) {
          const property = properties[name];
          const get = property && apply(owns, property, ["get"]) ? property.get : void 0;
          return get ? apply(get, backend, []) : backend[name];
        },
        binaryType(value) {
          const property = properties.binaryType;
          const set = property && apply(owns, property, ["set"]) ? property.set : void 0;
          if (set) apply(set, backend, [value]);
          else backend.binaryType = value;
        },
        send(data) {
          apply(send, backend, [data]);
        },
        close(code, reason) {
          apply(close, backend, code === void 0 ? [] : [code, reason]);
        }
      };
      current.backend.binaryType(current.binaryType);
      apply(add, backend, [
        "open",
        () => {
          apply(dispatch, socket, [new NativeEvent("open")]);
        }
      ]);
      apply(add, backend, [
        "message",
        (event) => {
          apply(dispatch, socket, [
            new NativeMessage("message", {
              data: apply(messageData, event, []),
              origin: apply(messageOrigin, event, []),
              lastEventId: apply(messageId, event, []),
              source: null,
              ports: []
            })
          ]);
        }
      ]);
      apply(add, backend, [
        "error",
        () => {
          apply(dispatch, socket, [new NativeEvent("error")]);
        }
      ]);
      apply(add, backend, [
        "close",
        (event) => {
          apply(dispatch, socket, [
            new NativeClose("close", {
              code: apply(closeCode, event, []),
              reason: apply(closeReason, event, []),
              wasClean: apply(closeClean, event, [])
            })
          ]);
        }
      ]);
    }
    function validateClose(code, reason) {
      let converted;
      if (code !== void 0) {
        const value = +code;
        const clamped = value !== value ? 0 : min(65535, max(0, value));
        const lower = floor(clamped);
        converted = clamped - lower === 0.5 ? lower % 2 === 0 ? lower : lower + 1 : floor(clamped + 0.5);
        if (converted !== 1e3 && (converted < 3e3 || converted > 4999))
          throw new NativeError(
            "Invalid WebSocket close code",
            "InvalidAccessError"
          );
      }
      const description = reason === void 0 ? "" : text(reason);
      if (apply(encode, encoder, [description]).length > 123)
        throw new NativeError(
          "WebSocket close reason is too long",
          "SyntaxError"
        );
      return [converted, description];
    }
    function payload(data) {
      if (isView(data)) return data;
      try {
        apply(bufferLength, data, []);
        return data;
      } catch {
      }
      try {
        apply(blobSize, data, []);
        return data;
      } catch {
      }
      return text(data);
    }
    function dataSize(data) {
      if (isView(data)) {
        try {
          return apply(viewLength, data, []);
        } catch {
          return apply(dataLength, data, []);
        }
      }
      try {
        return apply(bufferLength, data, []);
      } catch {
      }
      try {
        return apply(blobSize, data, []);
      } catch {
      }
      return apply(encode, encoder, [text(data)]).length;
    }
    class GatedWebSocket extends NativeTarget {
      constructor(input, offered) {
        super();
        if (!arguments.length)
          throw new NativeTypeError("WebSocket requires a URL");
        const originalUrl = text(input);
        const requested = protocols(offered);
        let url;
        try {
          url = new NativeURL(
            originalUrl,
            baseURI ? apply(baseURI, win.document, []) : win.document?.baseURI
          );
        } catch {
          throw new NativeError("Invalid WebSocket URL", "SyntaxError");
        }
        const protocol = apply(scheme.get, url, []);
        if (protocol === "http:") apply(scheme.set, url, ["ws:"]);
        else if (protocol === "https:") apply(scheme.set, url, ["wss:"]);
        else if (protocol !== "ws:" && protocol !== "wss:")
          throw new NativeError("Invalid WebSocket URL scheme", "SyntaxError");
        const address = apply(href, url, []);
        if (apply(indexOf, address, ["#"]) !== -1)
          throw new NativeError(
            "WebSocket URLs cannot contain fragments",
            "SyntaxError"
          );
        const current = {
          url: address,
          phase: 0,
          binaryType: "blob",
          bufferedAmount: 0,
          pending: true,
          backend: void 0,
          cancel: void 0,
          handlers: create(null)
        };
        apply(weakSet, states, [this, current]);
        let constructing = true;
        const decided = (allowed) => {
          if (constructing) {
            schedule(() => decided(allowed), 0);
            return;
          }
          if (!current.pending) return;
          cancel(current);
          if (allowed !== true) return fail(this, current);
          try {
            connect(this, current, requested);
          } catch {
            fail(this, current);
          }
        };
        try {
          const cancellation = authorize(address, decided);
          if (!current.pending) cancellation();
          else current.cancel = cancellation;
        } catch {
          fail(this, current);
        }
        constructing = false;
      }
      get url() {
        return state(this).url;
      }
      get readyState() {
        const current = state(this);
        return current.backend?.read("readyState") ?? current.phase;
      }
      get bufferedAmount() {
        const current = state(this);
        return current.backend?.read("bufferedAmount") ?? current.bufferedAmount;
      }
      get protocol() {
        return state(this).backend?.read("protocol") ?? "";
      }
      get extensions() {
        return state(this).backend?.read("extensions") ?? "";
      }
      get binaryType() {
        return state(this).binaryType;
      }
      set binaryType(value) {
        const current = state(this);
        const converted = text(value);
        if (converted !== "blob" && converted !== "arraybuffer") return;
        current.binaryType = converted;
        current.backend?.binaryType(converted);
      }
      send(data) {
        const current = state(this);
        if (!arguments.length)
          throw new NativeTypeError("WebSocket.send requires data");
        const converted = payload(data);
        if ((current.backend?.read("readyState") ?? current.phase) === 0)
          throw new NativeError("WebSocket is not open", "InvalidStateError");
        if (current.backend) current.backend.send(converted);
        else current.bufferedAmount += dataSize(converted);
      }
      close(code, reason) {
        const current = state(this);
        const converted = validateClose(code, reason);
        if (current.backend) current.backend.close(converted[0], converted[1]);
        else if (current.phase === 0) fail(this, current);
      }
    }
    for (const name of ["open", "message", "error", "close"]) {
      define(GatedWebSocket.prototype, `on${name}`, {
        configurable: false,
        enumerable: true,
        get() {
          return state(this).handlers[name]?.callback ?? null;
        },
        set(value) {
          const current = state(this);
          const previous = current.handlers[name];
          if (typeof value !== "function") {
            if (previous) apply(remove, this, [name, previous.listener]);
            delete current.handlers[name];
          } else if (previous) previous.callback = value;
          else {
            const handler = {
              callback: value,
              listener: (event) => {
                apply(handler.callback, this, [event]);
              }
            };
            current.handlers[name] = handler;
            apply(add, this, [name, handler.listener]);
          }
        }
      });
    }
    for (const name of [
      "url",
      "readyState",
      "bufferedAmount",
      "protocol",
      "extensions",
      "binaryType"
    ])
      define(GatedWebSocket.prototype, name, {
        ...descriptor(GatedWebSocket.prototype, name),
        configurable: false
      });
    freezeValue(GatedWebSocket.prototype, "send", GatedWebSocket.prototype.send);
    freezeValue(
      GatedWebSocket.prototype,
      "close",
      GatedWebSocket.prototype.close
    );
    const constants = ["CONNECTING", "OPEN", "CLOSING", "CLOSED"];
    for (let index = 0; index < constants.length; index++) {
      freezeValue(GatedWebSocket, constants[index], index);
      freezeValue(GatedWebSocket.prototype, constants[index], index);
    }
    freezeValue(nativePrototype, "constructor", GatedWebSocket);
    freezeValue(GatedWebSocket.prototype, "constructor", GatedWebSocket);
    freezeValue(win, "WebSocket", GatedWebSocket);
  }

  // src/media.ts
  function installMediaPolicy(win, authorize) {
    const navigator2 = win.navigator;
    if (!navigator2) return;
    const devices = navigator2.mediaDevices;
    const nativeGetUserMedia = devices?.getUserMedia;
    const apply = Reflect.apply;
    const get = Reflect.get;
    const object = Object;
    const create = Object.create;
    const freeze = Object.freeze;
    const descriptor = Object.getOwnPropertyDescriptor;
    const prototypeOf = Object.getPrototypeOf;
    const NativePromise = win.Promise;
    const NativeTypeError = win.TypeError;
    const NativeDOMException = win.DOMException;
    function trackConstraints(value) {
      if (value === void 0) return false;
      if (value === null) return create(null);
      return object(value) === value ? value : !!value;
    }
    function snapshot(input) {
      if (input !== null && input !== void 0 && object(input) !== input) {
        throw new NativeTypeError("MediaStreamConstraints must be an object");
      }
      const missing = input === null || input === void 0;
      const audio = trackConstraints(
        missing ? void 0 : get(input, "audio", input)
      );
      const video = trackConstraints(
        missing ? void 0 : get(input, "video", input)
      );
      if (audio === false && video === false) {
        throw new NativeTypeError(
          "At least one of audio or video must be requested"
        );
      }
      const constraints = create(null);
      constraints.audio = audio;
      constraints.video = video;
      return freeze(constraints);
    }
    function capture(input, invoke, reject) {
      let settled = false;
      let cancel;
      try {
        let decided2 = function(allowed) {
          if (settled) return;
          settled = true;
          cancel?.();
          if (allowed !== true) {
            reject(
              new NativeDOMException(
                "Media capture is not allowed",
                "NotAllowedError"
              )
            );
            return;
          }
          try {
            invoke(constraints);
          } catch (error) {
            reject(error);
          }
        };
        var decided = decided2;
        const constraints = snapshot(input);
        if (typeof authorize !== "function") {
          decided2(false);
          return;
        }
        const cancellation = authorize(
          constraints.audio !== false,
          constraints.video !== false,
          decided2
        );
        if (settled) cancellation();
        else cancel = cancellation;
      } catch (error) {
        settled = true;
        cancel?.();
        reject(error);
      }
    }
    function lockMethod(target, name, method) {
      let owner = target;
      while (owner) {
        if (owner === target || descriptor(owner, name))
          freezeValue(owner, name, method);
        owner = prototypeOf(owner);
      }
    }
    if (typeof devices?.getDisplayMedia === "function") {
      lockMethod(devices, "getDisplayMedia", function() {
        return new NativePromise(
          (_resolve, reject) => {
            reject(
              new NativeDOMException(
                "Screen capture is not allowed",
                "NotAllowedError"
              )
            );
          }
        );
      });
    }
    if (typeof nativeGetUserMedia === "function") {
      lockMethod(devices, "getUserMedia", function(input) {
        return new NativePromise(
          (resolve, reject) => {
            if (this !== devices) {
              reject(new NativeTypeError("Invalid MediaDevices receiver"));
              return;
            }
            capture(
              input,
              (constraints) => resolve(apply(nativeGetUserMedia, devices, [constraints])),
              reject
            );
          }
        );
      });
    }
    for (const name of [
      "getUserMedia",
      "webkitGetUserMedia",
      "mozGetUserMedia",
      "msGetUserMedia"
    ]) {
      const native = navigator2[name];
      if (typeof native !== "function") continue;
      lockMethod(
        navigator2,
        name,
        function(input, success, failure) {
          if (this !== navigator2 || typeof success !== "function" || typeof failure !== "function") {
            throw new NativeTypeError(
              "Invalid getUserMedia receiver or callbacks"
            );
          }
          capture(
            input,
            (constraints) => {
              apply(native, navigator2, [constraints, success, failure]);
            },
            (error) => {
              apply(failure, void 0, [error]);
            }
          );
        }
      );
    }
  }

  // src/container.ts
  function installContainer(_authorize, options = {}) {
    const runtime = window;
    const _webSocketBackend = runtime.__truapi_websocket_connect__;
    freezeAndDelete(window, "__truapi_websocket_connect__");
    installWebSocketGate(window, _authorize.network, _webSocketBackend);
    if (!options.nativeHttp) {
      installFetchGate(window, _authorize.network);
      installXhrGate(window, _authorize.network);
    }
    installMediaPolicy(window, _authorize.media);
    freezeAndDelete(window, "EventSource");
    freezeAndDelete(window, "WebTransport");
    freezeValue(navigator, "sendBeacon", () => false);
    freezeAndDelete(window, "indexedDB");
    freezeAndDelete(window, "caches");
    freezeCustom(
      document,
      "cookie",
      { get: () => "", set: () => {
      } },
      (current) => current === ""
    );
    freezeAndDelete(window, "Worker");
    freezeAndDelete(window, "SharedWorker");
    if (navigator.serviceWorker) {
      const _stubServiceWorker = Object.freeze({
        register: () => {
          throw new Error("ServiceWorker is not available");
        }
      });
      freezeCustom(
        navigator,
        "serviceWorker",
        { value: _stubServiceWorker, writable: false },
        (current) => current === _stubServiceWorker
      );
    }
    const _createElement = document.createElement.bind(document);
    freezeValue(document, "createElement", (tagName, options2) => {
      if (tagName.toLowerCase() === "iframe") {
        throw new Error("iframe creation is not allowed");
      }
      return _createElement(tagName, options2);
    });
    installWebRtcPolicy(window, _authorize.webRtc);
    reportLockdownFailures();
  }

  // ../../node_modules/neverthrow/dist/index.es.js
  var defaultErrorConfig = {
    withStackTrace: false
  };
  var createNeverThrowError = (message, result, config2 = defaultErrorConfig) => {
    const data = result.isOk() ? { type: "Ok", value: result.value } : { type: "Err", value: result.error };
    const maybeStack = config2.withStackTrace ? new Error().stack : void 0;
    return {
      data,
      message,
      stack: maybeStack
    };
  };
  function __awaiter(thisArg, _arguments, P, generator) {
    function adopt(value) {
      return value instanceof P ? value : new P(function(resolve) {
        resolve(value);
      });
    }
    return new (P || (P = Promise))(function(resolve, reject) {
      function fulfilled(value) {
        try {
          step(generator.next(value));
        } catch (e) {
          reject(e);
        }
      }
      function rejected(value) {
        try {
          step(generator["throw"](value));
        } catch (e) {
          reject(e);
        }
      }
      function step(result) {
        result.done ? resolve(result.value) : adopt(result.value).then(fulfilled, rejected);
      }
      step((generator = generator.apply(thisArg, _arguments || [])).next());
    });
  }
  function __values(o) {
    var s = typeof Symbol === "function" && Symbol.iterator, m = s && o[s], i = 0;
    if (m) return m.call(o);
    if (o && typeof o.length === "number") return {
      next: function() {
        if (o && i >= o.length) o = void 0;
        return { value: o && o[i++], done: !o };
      }
    };
    throw new TypeError(s ? "Object is not iterable." : "Symbol.iterator is not defined.");
  }
  function __await(v) {
    return this instanceof __await ? (this.v = v, this) : new __await(v);
  }
  function __asyncGenerator(thisArg, _arguments, generator) {
    if (!Symbol.asyncIterator) throw new TypeError("Symbol.asyncIterator is not defined.");
    var g = generator.apply(thisArg, _arguments || []), i, q = [];
    return i = Object.create((typeof AsyncIterator === "function" ? AsyncIterator : Object).prototype), verb("next"), verb("throw"), verb("return", awaitReturn), i[Symbol.asyncIterator] = function() {
      return this;
    }, i;
    function awaitReturn(f) {
      return function(v) {
        return Promise.resolve(v).then(f, reject);
      };
    }
    function verb(n, f) {
      if (g[n]) {
        i[n] = function(v) {
          return new Promise(function(a, b) {
            q.push([n, v, a, b]) > 1 || resume(n, v);
          });
        };
        if (f) i[n] = f(i[n]);
      }
    }
    function resume(n, v) {
      try {
        step(g[n](v));
      } catch (e) {
        settle(q[0][3], e);
      }
    }
    function step(r) {
      r.value instanceof __await ? Promise.resolve(r.value.v).then(fulfill, reject) : settle(q[0][2], r);
    }
    function fulfill(value) {
      resume("next", value);
    }
    function reject(value) {
      resume("throw", value);
    }
    function settle(f, v) {
      if (f(v), q.shift(), q.length) resume(q[0][0], q[0][1]);
    }
  }
  function __asyncDelegator(o) {
    var i, p;
    return i = {}, verb("next"), verb("throw", function(e) {
      throw e;
    }), verb("return"), i[Symbol.iterator] = function() {
      return this;
    }, i;
    function verb(n, f) {
      i[n] = o[n] ? function(v) {
        return (p = !p) ? { value: __await(o[n](v)), done: false } : f ? f(v) : v;
      } : f;
    }
  }
  function __asyncValues(o) {
    if (!Symbol.asyncIterator) throw new TypeError("Symbol.asyncIterator is not defined.");
    var m = o[Symbol.asyncIterator], i;
    return m ? m.call(o) : (o = typeof __values === "function" ? __values(o) : o[Symbol.iterator](), i = {}, verb("next"), verb("throw"), verb("return"), i[Symbol.asyncIterator] = function() {
      return this;
    }, i);
    function verb(n) {
      i[n] = o[n] && function(v) {
        return new Promise(function(resolve, reject) {
          v = o[n](v), settle(resolve, reject, v.done, v.value);
        });
      };
    }
    function settle(resolve, reject, d, v) {
      Promise.resolve(v).then(function(v2) {
        resolve({ value: v2, done: d });
      }, reject);
    }
  }
  var ResultAsync = class _ResultAsync {
    constructor(res) {
      this._promise = res;
    }
    static fromSafePromise(promise) {
      const newPromise = promise.then((value) => new Ok(value));
      return new _ResultAsync(newPromise);
    }
    static fromPromise(promise, errorFn) {
      const newPromise = promise.then((value) => new Ok(value)).catch((e) => new Err(errorFn(e)));
      return new _ResultAsync(newPromise);
    }
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    static fromThrowable(fn, errorFn) {
      return (...args) => {
        return new _ResultAsync((() => __awaiter(this, void 0, void 0, function* () {
          try {
            return new Ok(yield fn(...args));
          } catch (error) {
            return new Err(errorFn ? errorFn(error) : error);
          }
        }))());
      };
    }
    static combine(asyncResultList) {
      return combineResultAsyncList(asyncResultList);
    }
    static combineWithAllErrors(asyncResultList) {
      return combineResultAsyncListWithAllErrors(asyncResultList);
    }
    map(f) {
      return new _ResultAsync(this._promise.then((res) => __awaiter(this, void 0, void 0, function* () {
        if (res.isErr()) {
          return new Err(res.error);
        }
        return new Ok(yield f(res.value));
      })));
    }
    andThrough(f) {
      return new _ResultAsync(this._promise.then((res) => __awaiter(this, void 0, void 0, function* () {
        if (res.isErr()) {
          return new Err(res.error);
        }
        const newRes = yield f(res.value);
        if (newRes.isErr()) {
          return new Err(newRes.error);
        }
        return new Ok(res.value);
      })));
    }
    andTee(f) {
      return new _ResultAsync(this._promise.then((res) => __awaiter(this, void 0, void 0, function* () {
        if (res.isErr()) {
          return new Err(res.error);
        }
        try {
          yield f(res.value);
        } catch (e) {
        }
        return new Ok(res.value);
      })));
    }
    orTee(f) {
      return new _ResultAsync(this._promise.then((res) => __awaiter(this, void 0, void 0, function* () {
        if (res.isOk()) {
          return new Ok(res.value);
        }
        try {
          yield f(res.error);
        } catch (e) {
        }
        return new Err(res.error);
      })));
    }
    mapErr(f) {
      return new _ResultAsync(this._promise.then((res) => __awaiter(this, void 0, void 0, function* () {
        if (res.isOk()) {
          return new Ok(res.value);
        }
        return new Err(yield f(res.error));
      })));
    }
    // eslint-disable-next-line @typescript-eslint/no-explicit-any, @typescript-eslint/explicit-module-boundary-types
    andThen(f) {
      return new _ResultAsync(this._promise.then((res) => {
        if (res.isErr()) {
          return new Err(res.error);
        }
        const newValue = f(res.value);
        return newValue instanceof _ResultAsync ? newValue._promise : newValue;
      }));
    }
    // eslint-disable-next-line @typescript-eslint/no-explicit-any, @typescript-eslint/explicit-module-boundary-types
    orElse(f) {
      return new _ResultAsync(this._promise.then((res) => __awaiter(this, void 0, void 0, function* () {
        if (res.isErr()) {
          return f(res.error);
        }
        return new Ok(res.value);
      })));
    }
    match(ok2, _err) {
      return this._promise.then((res) => res.match(ok2, _err));
    }
    unwrapOr(t) {
      return this._promise.then((res) => res.unwrapOr(t));
    }
    /**
     * @deprecated will be removed in 9.0.0.
     *
     * You can use `safeTry` without this method.
     * @example
     * ```typescript
     * safeTry(async function* () {
     *   const okValue = yield* yourResult
     * })
     * ```
     * Emulates Rust's `?` operator in `safeTry`'s body. See also `safeTry`.
     */
    safeUnwrap() {
      return __asyncGenerator(this, arguments, function* safeUnwrap_1() {
        return yield __await(yield __await(yield* __asyncDelegator(__asyncValues(yield __await(this._promise.then((res) => res.safeUnwrap()))))));
      });
    }
    // Makes ResultAsync implement PromiseLike<Result>
    then(successCallback, failureCallback) {
      return this._promise.then(successCallback, failureCallback);
    }
    [Symbol.asyncIterator]() {
      return __asyncGenerator(this, arguments, function* _a() {
        const result = yield __await(this._promise);
        if (result.isErr()) {
          yield yield __await(errAsync(result.error));
        }
        return yield __await(result.value);
      });
    }
  };
  function okAsync(value) {
    return new ResultAsync(Promise.resolve(new Ok(value)));
  }
  function errAsync(err2) {
    return new ResultAsync(Promise.resolve(new Err(err2)));
  }
  var fromPromise = ResultAsync.fromPromise;
  var fromSafePromise = ResultAsync.fromSafePromise;
  var fromAsyncThrowable = ResultAsync.fromThrowable;
  var combineResultList = (resultList) => {
    let acc = ok([]);
    for (const result of resultList) {
      if (result.isErr()) {
        acc = err(result.error);
        break;
      } else {
        acc.map((list) => list.push(result.value));
      }
    }
    return acc;
  };
  var combineResultAsyncList = (asyncResultList) => ResultAsync.fromSafePromise(Promise.all(asyncResultList)).andThen(combineResultList);
  var combineResultListWithAllErrors = (resultList) => {
    let acc = ok([]);
    for (const result of resultList) {
      if (result.isErr() && acc.isErr()) {
        acc.error.push(result.error);
      } else if (result.isErr() && acc.isOk()) {
        acc = err([result.error]);
      } else if (result.isOk() && acc.isOk()) {
        acc.value.push(result.value);
      }
    }
    return acc;
  };
  var combineResultAsyncListWithAllErrors = (asyncResultList) => ResultAsync.fromSafePromise(Promise.all(asyncResultList)).andThen(combineResultListWithAllErrors);
  var Result;
  (function(Result3) {
    function fromThrowable2(fn, errorFn) {
      return (...args) => {
        try {
          const result = fn(...args);
          return ok(result);
        } catch (e) {
          return err(errorFn ? errorFn(e) : e);
        }
      };
    }
    Result3.fromThrowable = fromThrowable2;
    function combine(resultList) {
      return combineResultList(resultList);
    }
    Result3.combine = combine;
    function combineWithAllErrors(resultList) {
      return combineResultListWithAllErrors(resultList);
    }
    Result3.combineWithAllErrors = combineWithAllErrors;
  })(Result || (Result = {}));
  function ok(value) {
    return new Ok(value);
  }
  function err(err2) {
    return new Err(err2);
  }
  var Ok = class {
    constructor(value) {
      this.value = value;
    }
    isOk() {
      return true;
    }
    isErr() {
      return !this.isOk();
    }
    map(f) {
      return ok(f(this.value));
    }
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    mapErr(_f) {
      return ok(this.value);
    }
    // eslint-disable-next-line @typescript-eslint/no-explicit-any, @typescript-eslint/explicit-module-boundary-types
    andThen(f) {
      return f(this.value);
    }
    // eslint-disable-next-line @typescript-eslint/no-explicit-any, @typescript-eslint/explicit-module-boundary-types
    andThrough(f) {
      return f(this.value).map((_value) => this.value);
    }
    andTee(f) {
      try {
        f(this.value);
      } catch (e) {
      }
      return ok(this.value);
    }
    orTee(_f) {
      return ok(this.value);
    }
    // eslint-disable-next-line @typescript-eslint/no-explicit-any, @typescript-eslint/explicit-module-boundary-types
    orElse(_f) {
      return ok(this.value);
    }
    asyncAndThen(f) {
      return f(this.value);
    }
    // eslint-disable-next-line @typescript-eslint/no-explicit-any, @typescript-eslint/explicit-module-boundary-types
    asyncAndThrough(f) {
      return f(this.value).map(() => this.value);
    }
    asyncMap(f) {
      return ResultAsync.fromSafePromise(f(this.value));
    }
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    unwrapOr(_v) {
      return this.value;
    }
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    match(ok2, _err) {
      return ok2(this.value);
    }
    safeUnwrap() {
      const value = this.value;
      return (function* () {
        return value;
      })();
    }
    _unsafeUnwrap(_) {
      return this.value;
    }
    _unsafeUnwrapErr(config2) {
      throw createNeverThrowError("Called `_unsafeUnwrapErr` on an Ok", this, config2);
    }
    // eslint-disable-next-line @typescript-eslint/no-this-alias, require-yield
    *[Symbol.iterator]() {
      return this.value;
    }
  };
  var Err = class {
    constructor(error) {
      this.error = error;
    }
    isOk() {
      return false;
    }
    isErr() {
      return !this.isOk();
    }
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    map(_f) {
      return err(this.error);
    }
    mapErr(f) {
      return err(f(this.error));
    }
    andThrough(_f) {
      return err(this.error);
    }
    andTee(_f) {
      return err(this.error);
    }
    orTee(f) {
      try {
        f(this.error);
      } catch (e) {
      }
      return err(this.error);
    }
    // eslint-disable-next-line @typescript-eslint/no-explicit-any, @typescript-eslint/explicit-module-boundary-types
    andThen(_f) {
      return err(this.error);
    }
    // eslint-disable-next-line @typescript-eslint/no-explicit-any, @typescript-eslint/explicit-module-boundary-types
    orElse(f) {
      return f(this.error);
    }
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    asyncAndThen(_f) {
      return errAsync(this.error);
    }
    asyncAndThrough(_f) {
      return errAsync(this.error);
    }
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    asyncMap(_f) {
      return errAsync(this.error);
    }
    unwrapOr(v) {
      return v;
    }
    match(_ok, err2) {
      return err2(this.error);
    }
    safeUnwrap() {
      const error = this.error;
      return (function* () {
        yield err(error);
        throw new Error("Do not use this generator out of `safeTry`");
      })();
    }
    _unsafeUnwrap(config2) {
      throw createNeverThrowError("Called `_unsafeUnwrap` on an Err", this, config2);
    }
    _unsafeUnwrapErr(_) {
      return this.error;
    }
    *[Symbol.iterator]() {
      const self = this;
      yield self;
      return self;
    }
  };
  var fromThrowable = Result.fromThrowable;

  // ../../node_modules/@noble/hashes/utils.js
  function isBytes(a) {
    return a instanceof Uint8Array || ArrayBuffer.isView(a) && a.constructor.name === "Uint8Array" && "BYTES_PER_ELEMENT" in a && a.BYTES_PER_ELEMENT === 1;
  }
  function abytes(value, length, title = "") {
    const bytes = isBytes(value);
    const len = value?.length;
    const needsLen = length !== void 0;
    if (!bytes || needsLen && len !== length) {
      const prefix = title && `"${title}" `;
      const ofLen = needsLen ? ` of length ${length}` : "";
      const got = bytes ? `length=${len}` : `type=${typeof value}`;
      const message = prefix + "expected Uint8Array" + ofLen + ", got " + got;
      if (!bytes)
        throw new TypeError(message);
      throw new RangeError(message);
    }
    return value;
  }
  var hasHexBuiltin = /* @__PURE__ */ (() => (
    // @ts-ignore
    typeof Uint8Array.from([]).toHex === "function" && typeof Uint8Array.fromHex === "function"
  ))();
  var hexes = /* @__PURE__ */ Array.from({ length: 256 }, (_, i) => i.toString(16).padStart(2, "0"));
  function bytesToHex(bytes) {
    abytes(bytes);
    if (hasHexBuiltin)
      return bytes.toHex();
    let hex = "";
    for (let i = 0; i < bytes.length; i++) {
      hex += hexes[bytes[i]];
    }
    return hex;
  }
  var asciis = { _0: 48, _9: 57, A: 65, F: 70, a: 97, f: 102 };
  function asciiToBase16(ch) {
    if (ch >= asciis._0 && ch <= asciis._9)
      return ch - asciis._0;
    if (ch >= asciis.A && ch <= asciis.F)
      return ch - (asciis.A - 10);
    if (ch >= asciis.a && ch <= asciis.f)
      return ch - (asciis.a - 10);
    return;
  }
  function hexToBytes(hex) {
    if (typeof hex !== "string")
      throw new TypeError("hex string expected, got " + typeof hex);
    if (hasHexBuiltin) {
      try {
        return Uint8Array.fromHex(hex);
      } catch (error) {
        if (error instanceof SyntaxError)
          throw new RangeError(error.message);
        throw error;
      }
    }
    const hl = hex.length;
    const al = hl / 2;
    if (hl % 2)
      throw new RangeError("hex string expected, got unpadded hex of length " + hl);
    const array = new Uint8Array(al);
    for (let ai = 0, hi = 0; ai < al; ai++, hi += 2) {
      const n1 = asciiToBase16(hex.charCodeAt(hi));
      const n2 = asciiToBase16(hex.charCodeAt(hi + 1));
      if (n1 === void 0 || n2 === void 0) {
        const char = hex[hi] + hex[hi + 1];
        throw new RangeError('hex string expected, got non-hex character "' + char + '" at index ' + hi);
      }
      array[ai] = n1 * 16 + n2;
    }
    return array;
  }
  function concatBytes(...arrays) {
    let sum = 0;
    for (let i = 0; i < arrays.length; i++) {
      const a = arrays[i];
      abytes(a);
      sum += a.length;
    }
    const res = new Uint8Array(sum);
    for (let i = 0, pad = 0; i < arrays.length; i++) {
      const a = arrays[i];
      res.set(a, pad);
      pad += a.length;
    }
    return res;
  }

  // ../../node_modules/scale-ts/dist/scale-ts.js
  var __defProp2 = Object.defineProperty;
  var __defNormalProp2 = (obj, key, value) => key in obj ? __defProp2(obj, key, { enumerable: true, configurable: true, writable: true, value }) : obj[key] = value;
  var __publicField2 = (obj, key, value) => {
    __defNormalProp2(obj, typeof key !== "symbol" ? key + "" : key, value);
    return value;
  };
  var HEX_MAP = {
    0: 0,
    1: 1,
    2: 2,
    3: 3,
    4: 4,
    5: 5,
    6: 6,
    7: 7,
    8: 8,
    9: 9,
    a: 10,
    b: 11,
    c: 12,
    d: 13,
    e: 14,
    f: 15,
    A: 10,
    B: 11,
    C: 12,
    D: 13,
    E: 14,
    F: 15
  };
  function fromHex(hexString) {
    const isOdd = hexString.length % 2;
    const base = (hexString[1] === "x" ? 2 : 0) + isOdd;
    const nBytes = (hexString.length - base) / 2 + isOdd;
    const bytes = new Uint8Array(nBytes);
    if (isOdd)
      bytes[0] = 0 | HEX_MAP[hexString[2]];
    for (let i = 0; i < nBytes; ) {
      const idx = base + i * 2;
      const a = HEX_MAP[hexString[idx]];
      const b = HEX_MAP[hexString[idx + 1]];
      bytes[isOdd + i++] = a << 4 | b;
    }
    return bytes;
  }
  var InternalUint8Array = class extends Uint8Array {
    constructor(buffer) {
      super(buffer);
      __publicField2(this, "i", 0);
      __publicField2(this, "v");
      this.v = new DataView(buffer);
    }
  };
  var toInternalBytes = (fn) => (buffer) => fn(buffer instanceof InternalUint8Array ? buffer : new InternalUint8Array(buffer instanceof Uint8Array ? buffer.buffer : typeof buffer === "string" ? fromHex(buffer).buffer : buffer));
  var mergeUint8 = (inputs) => {
    const len = inputs.length;
    let totalLen = 0;
    for (let i = 0; i < len; i++)
      totalLen += inputs[i].length;
    const result = new Uint8Array(totalLen);
    for (let idx = 0, at = 0; idx < len; idx++) {
      const current = inputs[idx];
      result.set(current, at);
      at += current.byteLength;
    }
    return result;
  };
  function mapObject(input, mapper) {
    const keys = Object.keys(input);
    const len = keys.length;
    const result = {};
    for (let i = 0; i < len; i++) {
      const key = keys[i];
      result[key] = mapper(input[key], key);
    }
    return result;
  }
  var createDecoder = toInternalBytes;
  var createCodec = (encoder, decoder) => {
    const result = [encoder, decoder];
    result.enc = encoder;
    result.dec = decoder;
    return result;
  };
  var enhanceEncoder = (encoder, mapper) => (value) => encoder(mapper(value));
  var enhanceDecoder = (decoder, mapper) => (value) => mapper(decoder(value));
  var enhanceCodec = ([encoder, decoder], toFrom, fromTo) => createCodec(enhanceEncoder(encoder, toFrom), enhanceDecoder(decoder, fromTo));
  function decodeInt(nBytes, getter2) {
    return toInternalBytes((bytes) => {
      const result = bytes.v[getter2](bytes.i, true);
      bytes.i += nBytes;
      return result;
    });
  }
  function encodeInt(nBytes, setter) {
    return (input) => {
      const result = new Uint8Array(nBytes);
      const dv = new DataView(result.buffer);
      dv[setter](0, input, true);
      return result;
    };
  }
  function intCodec(nBytes, getter2, setter) {
    return createCodec(encodeInt(nBytes, setter), decodeInt(nBytes, getter2));
  }
  var u8 = intCodec(1, "getUint8", "setUint8");
  var u16 = intCodec(2, "getUint16", "setUint16");
  var u32 = intCodec(4, "getUint32", "setUint32");
  var u64 = intCodec(8, "getBigUint64", "setBigUint64");
  var i8 = intCodec(1, "getInt8", "setInt8");
  var i16 = intCodec(2, "getInt16", "setInt16");
  var i32 = intCodec(4, "getInt32", "setInt32");
  var i64 = intCodec(8, "getBigInt64", "setBigInt64");
  var x128Enc = (value) => {
    const result = new Uint8Array(16);
    const dv = new DataView(result.buffer);
    dv.setBigInt64(0, value, true);
    dv.setBigInt64(8, value >> 64n, true);
    return result;
  };
  var create128Dec = (method) => toInternalBytes((input) => {
    const { v, i } = input;
    const right = v.getBigUint64(i, true);
    const left = v[method](i + 8, true);
    input.i += 16;
    return left << 64n | right;
  });
  var u128 = createCodec(x128Enc, create128Dec("getBigUint64"));
  var i128 = createCodec(x128Enc, create128Dec("getBigInt64"));
  var x256Enc = (value) => {
    const result = new Uint8Array(32);
    const dv = new DataView(result.buffer);
    dv.setBigInt64(0, value, true);
    dv.setBigInt64(8, value >> 64n, true);
    dv.setBigInt64(16, value >> 128n, true);
    dv.setBigInt64(24, value >> 192n, true);
    return result;
  };
  var create256Dec = (method) => toInternalBytes((input) => {
    let result = input.v.getBigUint64(input.i, true);
    input.i += 8;
    result |= input.v.getBigUint64(input.i, true) << 64n;
    input.i += 8;
    result |= input.v.getBigUint64(input.i, true) << 128n;
    input.i += 8;
    result |= input.v[method](input.i, true) << 192n;
    input.i += 8;
    return result;
  });
  var u256 = createCodec(x256Enc, create256Dec("getBigUint64"));
  var i256 = createCodec(x256Enc, create256Dec("getBigInt64"));
  var bool = enhanceCodec(u8, (value) => value ? 1 : 0, Boolean);
  var decoders = [u8[1], u16[1], u32[1]];
  var compactDec = toInternalBytes((bytes) => {
    const init = bytes[bytes.i];
    const kind = init & 3;
    if (kind < 3)
      return decoders[kind](bytes) >>> 2;
    const nBytes = (init >>> 2) + 4;
    bytes.i++;
    let result = 0n;
    const nU64 = nBytes / 8 | 0;
    let shift = 0n;
    for (let i = 0; i < nU64; i++) {
      result = u64[1](bytes) << shift | result;
      shift += 64n;
    }
    let nReminders = nBytes % 8;
    if (nReminders > 3) {
      result = BigInt(u32[1](bytes)) << shift | result;
      shift += 32n;
      nReminders -= 4;
    }
    if (nReminders > 1) {
      result = BigInt(u16[1](bytes)) << shift | result;
      shift += 16n;
      nReminders -= 2;
    }
    if (nReminders)
      result = BigInt(u8[1](bytes)) << shift | result;
    return result;
  });
  var MIN_U64 = 1n << 56n;
  var MIN_U32 = 1 << 24;
  var MIN_U16 = 256;
  var U32_MASK = 4294967295n;
  var SINGLE_BYTE_MODE_LIMIT = 1 << 6;
  var TWO_BYTE_MODE_LIMIT = 1 << 14;
  var FOUR_BYTE_MODE_LIMIT = 1 << 30;
  var compactEnc = (input) => {
    if (input < 0)
      throw new Error(`Wrong compact input (${input})`);
    const nInput = Number(input) << 2;
    if (input < SINGLE_BYTE_MODE_LIMIT)
      return u8[0](nInput);
    if (input < TWO_BYTE_MODE_LIMIT)
      return u16[0](nInput | 1);
    if (input < FOUR_BYTE_MODE_LIMIT)
      return u32[0](nInput | 2);
    let buffers = [new Uint8Array(1)];
    let bigValue = BigInt(input);
    while (bigValue >= MIN_U64) {
      buffers.push(u64[0](bigValue));
      bigValue >>= 64n;
    }
    if (bigValue >= MIN_U32) {
      buffers.push(u32[0](Number(bigValue & U32_MASK)));
      bigValue >>= 32n;
    }
    let smValue = Number(bigValue);
    if (smValue >= MIN_U16) {
      buffers.push(u16[0](smValue));
      smValue >>= 16;
    }
    smValue && buffers.push(u8[0](smValue));
    const result = mergeUint8(buffers);
    result[0] = result.length - 5 << 2 | 3;
    return result;
  };
  var compact = createCodec(compactEnc, compactDec);
  var textEncoder = new TextEncoder();
  var strEnc = (str2) => {
    const val = textEncoder.encode(str2);
    return mergeUint8([compact.enc(val.length), val]);
  };
  var textDecoder = new TextDecoder();
  var strDec = toInternalBytes((bytes) => {
    let nElements = compact.dec(bytes);
    const dv = new DataView(bytes.buffer, bytes.i, nElements);
    bytes.i += nElements;
    return textDecoder.decode(dv);
  });
  var str = createCodec(strEnc, strDec);
  var noop = () => {
  };
  var emptyArr = new Uint8Array(0);
  var _void = createCodec(() => emptyArr, noop);
  var BytesEnc = (nBytes) => nBytes === void 0 ? (bytes) => mergeUint8([compact.enc(bytes.length), bytes]) : (bytes) => bytes.length === nBytes ? bytes : bytes.slice(0, nBytes);
  var BytesDec = (nBytes) => toInternalBytes((bytes) => {
    const len = nBytes === void 0 ? compact.dec(bytes) : nBytes !== Infinity ? nBytes : bytes.byteLength - bytes.i;
    const result = new Uint8Array(bytes.buffer.slice(bytes.i, bytes.i + len));
    bytes.i += len;
    return result;
  });
  var Bytes = (nBytes) => createCodec(BytesEnc(nBytes), BytesDec(nBytes));
  Bytes.enc = BytesEnc;
  Bytes.dec = BytesDec;
  var enumEnc = (inner, x) => {
    const keys = Object.keys(inner);
    const mappedKeys = new Map(x?.map((actualIdx, idx) => [keys[idx], actualIdx]) ?? keys.map((key, idx) => [key, idx]));
    const getKey = (key) => mappedKeys.get(key);
    return ({ tag, value }) => mergeUint8([u8.enc(getKey(tag)), inner[tag](value)]);
  };
  var enumDec = (inner, x) => {
    const keys = Object.keys(inner);
    const mappedKeys = new Map(x?.map((actualIdx, idx) => [actualIdx, keys[idx]]) ?? keys.map((key, idx) => [idx, key]));
    return toInternalBytes((bytes) => {
      const idx = u8.dec(bytes);
      const tag = mappedKeys.get(idx);
      const innerDecoder = inner[tag];
      return {
        tag,
        value: innerDecoder(bytes)
      };
    });
  };
  var Enum = (inner, ...args) => createCodec(enumEnc(mapObject(inner, ([encoder]) => encoder), ...args), enumDec(mapObject(inner, ([, decoder]) => decoder), ...args));
  Enum.enc = enumEnc;
  Enum.dec = enumDec;
  var OptionDec = (inner) => toInternalBytes((bytes) => u8[1](bytes) > 0 ? inner(bytes) : void 0);
  var OptionEnc = (inner) => (value) => {
    const result = new Uint8Array(1);
    if (value === void 0)
      return result;
    result[0] = 1;
    return mergeUint8([result, inner(value)]);
  };
  var Option = (inner) => createCodec(OptionEnc(inner[0]), OptionDec(inner[1]));
  Option.enc = OptionEnc;
  Option.dec = OptionDec;
  var ResultDec = (okDecoder, koDecoder) => toInternalBytes((bytes) => {
    const success = u8[1](bytes) === 0;
    const decoder = success ? okDecoder : koDecoder;
    const value = decoder(bytes);
    return { success, value };
  });
  var ResultEnc = (okEncoder, koEncoder) => ({ success, value }) => mergeUint8([
    u8[0](success ? 0 : 1),
    (success ? okEncoder : koEncoder)(value)
  ]);
  var Result2 = (okCodec, koCodec) => createCodec(ResultEnc(okCodec[0], koCodec[0]), ResultDec(okCodec[1], koCodec[1]));
  Result2.dec = ResultDec;
  Result2.enc = ResultEnc;
  var TupleDec = (...decoders2) => toInternalBytes((bytes) => decoders2.map((decoder) => decoder(bytes)));
  var TupleEnc = (...encoders) => (values) => mergeUint8(encoders.map((enc, idx) => enc(values[idx])));
  var Tuple = (...codecs) => createCodec(TupleEnc(...codecs.map(([encoder]) => encoder)), TupleDec(...codecs.map(([, decoder]) => decoder)));
  Tuple.enc = TupleEnc;
  Tuple.dec = TupleDec;
  var StructEnc = (encoders) => {
    const keys = Object.keys(encoders);
    return enhanceEncoder(Tuple.enc(...Object.values(encoders)), (input) => keys.map((k) => input[k]));
  };
  var StructDec = (decoders2) => {
    const keys = Object.keys(decoders2);
    return enhanceDecoder(Tuple.dec(...Object.values(decoders2)), (tuple) => Object.fromEntries(tuple.map((value, idx) => [keys[idx], value])));
  };
  var Struct = (codecs) => createCodec(StructEnc(mapObject(codecs, (x) => x[0])), StructDec(mapObject(codecs, (x) => x[1])));
  Struct.enc = StructEnc;
  Struct.dec = StructDec;
  var VectorEnc = (inner, size) => size >= 0 ? (value) => mergeUint8(value.map(inner)) : (value) => mergeUint8([compact.enc(value.length), mergeUint8(value.map(inner))]);
  var VectorDec = (getter2, size) => toInternalBytes((bytes) => {
    const nElements = size >= 0 ? size : compact.dec(bytes);
    const result = new Array(nElements);
    for (let i = 0; i < nElements; i++) {
      result[i] = getter2(bytes);
    }
    return result;
  });
  var Vector = (inner, size) => createCodec(VectorEnc(inner[0], size), VectorDec(inner[1], size));
  Vector.enc = VectorEnc;
  Vector.dec = VectorDec;

  // ../packages/truapi/dist/scale.js
  var bool2 = enhanceCodec(u8, (value) => value ? 1 : 0, (byte) => {
    if (byte > 1)
      throw new Error("Invalid SCALE boolean");
    return byte === 1;
  });
  var OptionBool = enhanceCodec(u8, (value) => value === void 0 ? 0 : value ? 1 : 2, (byte) => {
    switch (byte) {
      case 0:
        return void 0;
      case 1:
        return true;
      case 2:
        return false;
      default:
        throw new Error(`Unknown OptionBool byte: ${byte}. Expected 0, 1, or 2.`);
    }
  });
  function bytesToHex2(bytes) {
    return `0x${bytesToHex(bytes)}`;
  }
  function hexToBytes2(hex) {
    return hexToBytes(hex.startsWith("0x") ? hex.slice(2) : hex);
  }
  function Hex(length) {
    return enhanceCodec(Bytes(length), hexToBytes2, bytesToHex2);
  }
  function TaggedUnion(inner) {
    return Enum(inner);
  }
  function CallError(domain) {
    return TaggedUnion({
      Domain: domain,
      Denied: _void,
      Unsupported: _void,
      MalformedFrame: Struct({ reason: str }),
      HostFailure: Struct({ reason: str }),
      // Appended last, mirroring the Rust enum: the variants above keep their
      // SCALE indices.
      Cancelled: _void
    });
  }
  function Status(...variants) {
    return enhanceCodec(u8, (value) => {
      const index = variants.indexOf(value);
      if (index === -1) {
        throw new Error(`Unknown status value: ${String(value)}`);
      }
      return index;
    }, (index) => {
      const value = variants[index];
      if (value === void 0) {
        throw new Error(`Unknown status index: ${index}`);
      }
      return value;
    });
  }
  function lazy(factory) {
    let resolved;
    const get = () => resolved ?? (resolved = factory());
    return createCodec((value) => get().enc(value), (input) => get().dec(input));
  }
  function indexedTaggedUnion(variants) {
    const byIndex = /* @__PURE__ */ new Map();
    for (const [tag, [index, codec]] of Object.entries(variants)) {
      if (!Number.isInteger(index) || index < 0 || index > 255) {
        throw new Error(`Invalid enum discriminant for ${tag}: ${index}`);
      }
      if (byIndex.has(index)) {
        throw new Error(`Duplicate enum discriminant: ${index}`);
      }
      byIndex.set(index, [tag, codec]);
    }
    return createCodec((value) => {
      const variant = variants[value.tag];
      if (!variant) {
        throw new Error(`Unknown enum variant: ${value.tag}`);
      }
      const [index, codec] = variant;
      const payload = codec.enc(value.value);
      const out = new Uint8Array(payload.length + 1);
      out[0] = index;
      out.set(payload, 1);
      return out;
    }, createDecoder((input) => {
      const index = u8.dec(input);
      const variant = byIndex.get(index);
      if (!variant) {
        throw new Error(`Unknown enum discriminant: ${index}`);
      }
      const [tag, codec] = variant;
      return { tag, value: codec.dec(input) };
    }));
  }

  // ../packages/truapi/dist/transport.js
  var PROTOCOL_ERROR_TRAIT_ID = 255;
  var PROTOCOL_ERROR_METHOD_ID = 255;
  var UnsupportedMessageError = class extends Error {
    constructor(traitId, methodId) {
      super(`Peer does not support wire message (${traitId}, ${methodId})`);
      /** Trait discriminant of the unsupported outbound frame. **/
      __publicField(this, "traitId");
      /** Method discriminant of the unsupported outbound frame. **/
      __publicField(this, "methodId");
      this.name = "UnsupportedMessageError";
      this.traitId = traitId;
      this.methodId = methodId;
    }
  };
  var ConnectionResetError = class extends Error {
    constructor(options) {
      super("TrUAPI host connection interrupted", options);
      this.name = "ConnectionResetError";
    }
  };
  function toError(error) {
    return error instanceof Error ? error : new Error(String(error));
  }
  var SubscriptionError = class extends Error {
    constructor(message, options) {
      super(message, options?.cause !== void 0 ? { cause: options.cause } : void 0);
      /**
       * Typed payload supplied by the peer when it interrupted the subscription.
       * `undefined` when the stream ended for any other reason (transport close,
       * decode failure, malformed interrupt payload).
       **/
      __publicField(this, "reason");
      this.name = "SubscriptionError";
      if (options?.reason !== void 0)
        this.reason = options.reason;
    }
  };
  var MESSAGE_TYPE_REQUEST = 0;
  var MESSAGE_TYPE_START = 0;
  var MESSAGE_TYPE_RESPONSE = 1;
  var MESSAGE_TYPE_RECEIVE = 1;
  var MESSAGE_TYPE_INTERRUPT = 2;
  var MESSAGE_TYPE_STOP = 3;
  var MESSAGE_TYPE_CANCEL = 4;
  function encodeWireMessage(message) {
    const { traitId, methodId, messageType } = message.payload;
    if (!Number.isInteger(traitId) || traitId < 0 || traitId > 255) {
      return err(new Error(`Invalid wire trait discriminant: ${traitId}`));
    }
    if (!Number.isInteger(methodId) || methodId < 0 || methodId > 255) {
      return err(new Error(`Invalid wire method discriminant: ${methodId}`));
    }
    if (!Number.isInteger(messageType) || messageType < 0 || messageType > 255) {
      return err(new Error(`Invalid wire message type: ${messageType}`));
    }
    return ok(concatBytes(str.enc(message.requestId), u8.enc(traitId), u8.enc(methodId), u8.enc(messageType), message.payload.value));
  }
  function decodeWireMessage(message) {
    if (message.length < 1) {
      return err(new Error("Wire frame too short: empty buffer"));
    }
    let cursor = message;
    const requestIdEndResult = scanStrEnd(cursor);
    if (requestIdEndResult.isErr()) {
      return err(requestIdEndResult.error);
    }
    const requestIdEnd = requestIdEndResult.value;
    const requestId = str.dec(cursor.subarray(0, requestIdEnd));
    cursor = cursor.subarray(requestIdEnd);
    if (cursor.length < 1) {
      return err(new Error("Wire frame too short: missing trait discriminant byte"));
    }
    if (cursor.length < 2) {
      return err(new Error("Wire frame too short: missing method discriminant byte"));
    }
    if (cursor.length < 3) {
      return err(new Error("Wire frame too short: missing message-type byte"));
    }
    const traitId = cursor[0];
    const methodId = cursor[1];
    const messageType = cursor[2];
    const value = cursor.subarray(3);
    const valueCopy = new Uint8Array(value.length);
    valueCopy.set(value);
    return ok({
      requestId,
      payload: { traitId, methodId, messageType, value: valueCopy }
    });
  }
  function scanStrEnd(bytes) {
    if (bytes.length < 1) {
      return err(new Error("compact-len: empty buffer"));
    }
    const first = bytes[0];
    const mode = first & 3;
    let lengthLen;
    let strLen;
    if (mode === 0) {
      lengthLen = 1;
      strLen = first >> 2;
    } else if (mode === 1) {
      if (bytes.length < 2) {
        return err(new Error("compact-len: truncated mode-1 prefix"));
      }
      lengthLen = 2;
      strLen = (first >> 2 | bytes[1] << 6) & 16383;
    } else if (mode === 2) {
      if (bytes.length < 4) {
        return err(new Error("compact-len: truncated mode-2 prefix"));
      }
      lengthLen = 4;
      strLen = (first >> 2 | bytes[1] << 6 | bytes[2] << 14 | bytes[3] << 22) >>> 0;
    } else {
      return err(new Error("compact big-int mode not supported in wire envelope"));
    }
    const total = lengthLen + strLen;
    if (total > bytes.length) {
      return err(new Error("compact-len: declared length exceeds buffer"));
    }
    return ok(total);
  }
  function createBaseProvider() {
    const listeners = /* @__PURE__ */ new Set();
    const closeListeners = /* @__PURE__ */ new Set();
    const onCloseCleanup = /* @__PURE__ */ new Set();
    let closedError = null;
    return {
      /** Current close error, or `null` while the provider is open. */
      closed: () => closedError,
      /** Dispatch an inbound frame to every active subscriber. */
      deliver(message) {
        if (closedError)
          return;
        for (const listener of [...listeners])
          listener(message);
      },
      /** Transition to the closed state. Idempotent. */
      close(error) {
        if (closedError)
          return;
        closedError = toError(error);
        for (const fn of [...onCloseCleanup]) {
          try {
            fn();
          } catch {
          }
        }
        onCloseCleanup.clear();
        for (const listener of [...closeListeners])
          listener(closedError);
        listeners.clear();
        closeListeners.clear();
      },
      /** Register a cleanup function to run exactly once when `close` fires. */
      onClose(fn) {
        if (closedError) {
          try {
            fn();
          } catch {
          }
          return;
        }
        onCloseCleanup.add(fn);
      },
      /** Register an inbound message listener. No-op after close. */
      subscribe(callback) {
        if (closedError)
          return () => {
          };
        listeners.add(callback);
        return () => {
          listeners.delete(callback);
        };
      },
      /**
       * Register a close listener. If the provider is already closed, the
       * callback fires immediately with the stored error.
       **/
      subscribeClose(callback) {
        if (closedError) {
          callback(closedError);
          return () => {
          };
        }
        closeListeners.add(callback);
        return () => {
          closeListeners.delete(callback);
        };
      }
    };
  }
  function createWebSocketProviderFactory() {
    const NativeWebSocket = WebSocket;
    const socketSend = NativeWebSocket.prototype.send;
    const socketClose = NativeWebSocket.prototype.close;
    const addEventListener = EventTarget.prototype.addEventListener;
    const messageData = Object.getOwnPropertyDescriptor(MessageEvent.prototype, "data").get;
    const setBinaryType = Object.getOwnPropertyDescriptor(NativeWebSocket.prototype, "binaryType").set;
    const apply = Reflect.apply;
    const bufferLength = Object.getOwnPropertyDescriptor(ArrayBuffer.prototype, "byteLength").get;
    return (url) => {
      const base = createBaseProvider();
      const socket = new NativeWebSocket(url);
      apply(setBinaryType, socket, ["arraybuffer"]);
      const pending = [];
      let open = false;
      const send = (frame) => apply(socketSend, socket, [frame]);
      let resolveOpened;
      let rejectOpened;
      const opened = new Promise((resolve, reject) => {
        resolveOpened = resolve;
        rejectOpened = reject;
      });
      opened.catch(() => {
      });
      apply(addEventListener, socket, [
        "open",
        () => {
          open = true;
          for (const frame of pending.splice(0))
            send(frame);
          resolveOpened();
        }
      ]);
      apply(addEventListener, socket, [
        "message",
        (event) => {
          let frame;
          try {
            const buffer = apply(messageData, event, []);
            frame = new Uint8Array(buffer, 0, apply(bufferLength, buffer, []));
          } catch (error) {
            base.close(error);
            return;
          }
          base.deliver(frame);
        }
      ]);
      apply(addEventListener, socket, [
        "error",
        () => {
          const error = new Error(`websocket error (${url})`);
          rejectOpened(error);
          base.close(error);
        }
      ]);
      apply(addEventListener, socket, [
        "close",
        () => {
          const error = new Error(`websocket closed (${url})`);
          rejectOpened(error);
          base.close(error);
        }
      ]);
      base.onClose(() => {
        try {
          apply(socketClose, socket, []);
        } catch {
        }
      });
      return {
        opened,
        postMessage(message) {
          const error = base.closed();
          if (error)
            throw error;
          if (open) {
            try {
              send(message);
            } catch (error2) {
              base.close(error2);
              throw toError(error2);
            }
          } else {
            pending.push(message);
          }
        },
        subscribe: base.subscribe,
        subscribeClose: base.subscribeClose,
        dispose() {
          base.close(new Error("websocket provider disposed"));
          pending.length = 0;
        }
      };
    };
  }

  // ../packages/truapi/dist/generated/types.js
  var ActionTrigger = lazy(() => Struct({ messageId: str, actionId: str, payload: Option(Hex()) }));
  var AllocatableResource = lazy(() => TaggedUnion({ StatementStoreAllowance: _void, BulletinAllowance: _void, SmartContractAllowance: DerivationIndex, AutoSigning: _void }));
  var AllocationOutcome = lazy(() => Status("Allocated", "Rejected", "NotAvailable"));
  var Arrangement = lazy(() => Status("Start", "End", "Center", "SpaceBetween", "SpaceAround", "SpaceEvenly"));
  var Background = lazy(() => Struct({ color: ColorToken, shape: Option(Shape) }));
  var BlendingMode = lazy(() => Status("Normal", "Multiply", "Screen", "Overlay", "Darken", "Lighten", "ColorDodge", "ColorBurn", "HardLight", "SoftLight", "Difference", "Exclusion", "Hue", "Saturation", "Color", "Luminosity"));
  var BorderStyle = lazy(() => Struct({ width: Size, color: ColorToken, shape: Option(Shape) }));
  var BoxProps = lazy(() => Struct({ contentAlignment: Option(ContentAlignment) }));
  var ButtonProps = lazy(() => Struct({ text: str, variant: Option(ButtonVariant), enabled: OptionBool, loading: OptionBool, clickAction: Option(str) }));
  var ButtonVariant = lazy(() => Status("Primary", "Secondary", "Text"));
  var Bytes32 = lazy(() => Hex(32));
  var ChainIdentifier = lazy(() => Status("Relay", "AssetHub", "People", "Bulletin"));
  var ChatAction = lazy(() => Struct({ actionId: str, title: str }));
  var ChatActionLayout = lazy(() => Status("Column", "Grid"));
  var ChatActionPayload = lazy(() => TaggedUnion({ MessagePosted: ChatMessageContent, ActionTriggered: ActionTrigger, Command: ChatCommand }));
  var ChatActions = lazy(() => Struct({ text: Option(str), actions: Vector(ChatAction), layout: ChatActionLayout }));
  var ChatBotRegistrationStatus = lazy(() => Status("New", "Exists"));
  var ChatCommand = lazy(() => Struct({ command: str, payload: str }));
  var ChatCustomMessage = lazy(() => Struct({ messageType: str, payload: Hex() }));
  var ChatFile = lazy(() => Struct({ url: str, fileName: str, mimeType: str, sizeBytes: u64, text: Option(str) }));
  var ChatMedia = lazy(() => Struct({ url: str }));
  var ChatMessageContent = lazy(() => TaggedUnion({ Text: Struct({ text: str }), RichText: ChatRichText, Actions: ChatActions, File: ChatFile, Reaction: ChatReaction, ReactionRemoved: ChatReaction, Custom: ChatCustomMessage }));
  var ChatReaction = lazy(() => Struct({ messageId: str, emoji: str }));
  var ChatRichText = lazy(() => Struct({ text: Option(str), media: Vector(ChatMedia) }));
  var ChatRoom = lazy(() => Struct({ roomId: str, participatingAs: ChatRoomParticipation }));
  var ChatRoomParticipation = lazy(() => Status("RoomHost", "Bot"));
  var ChatRoomRegistrationStatus = lazy(() => Status("New", "Exists"));
  var CodeFormat = lazy(() => Status("Qr", "Aztec", "DataMatrix", "Pdf417", "Ean13", "Ean8", "UpcE", "Code128", "Code39", "Code93", "Itf", "Codabar"));
  var CoinPaymentCheque = lazy(() => Struct({ id: Hex(32), amount: u32, encryptedSecrets: Hex() }));
  var CoinPaymentClearingReference = lazy(() => Struct({ root: Hex(32), leaves: Vector(Tuple(Hex(32), Hex(32))) }));
  var CoinPaymentError = lazy(() => Status("BalanceLow", "Denied", "BadCoins", "SnipedCoins", "PurseNotFound", "ReceivableNotFound", "UnsupportedChannel", "UserAgentCapabilityUnavailable", "Internal"));
  var CoinPaymentPurseInfo = lazy(() => Struct({ name: str, created: u64, creator: str, balance: u32 }));
  var CoinPaymentStatus = lazy(() => TaggedUnion({ Clearing: Struct({ clearing: u32, cleared: u32 }), Failed: Struct({ error: CoinPaymentError, cleared: u32, reference: CoinPaymentClearingReference }), Done: Struct({ cleared: u32, reference: CoinPaymentClearingReference }) }));
  var CoinPaymentTransmissionChannel = lazy(() => TaggedUnion({ Standard: Struct({ sssTopic: Hex(32) }) }));
  var ColorToken = lazy(() => Status("FgPrimary", "FgSecondary", "FgTertiary", "BgSurfaceMain", "BgSurfaceContainer", "BgSurfaceNested", "FgSuccess", "FgError", "FgWarning"));
  var ColumnProps = lazy(() => Struct({ horizontalAlignment: Option(HorizontalAlignment), verticalArrangement: Option(Arrangement) }));
  var ContactHandle = lazy(() => Struct({ bytes: Bytes32 }));
  var ContactPickOutcome = lazy(() => TaggedUnion({ Picked: Struct({ handle: ContactHandle }), Dismissed: _void, NoContacts: _void }));
  var ContentAlignment = lazy(() => Status("TopStart", "TopCenter", "TopEnd", "CenterStart", "Center", "CenterEnd", "BottomStart", "BottomCenter", "BottomEnd"));
  var ContextualAlias = lazy(() => Struct({ context: Hex(32), alias: Hex() }));
  var DerivationIndex = lazy(() => TaggedUnion({ Index: u32, Raw: Hex(32) }));
  var Dimensions = lazy(() => Struct({ top: Size, end: Size, bottom: Option(Size), start: Option(Size) }));
  var Effect = lazy(() => Status("Rainbow"));
  var EffectProps = lazy(() => Struct({ effect: Effect }));
  var GenericError = lazy(() => Struct({ reason: str }));
  var HorizontalAlignment = lazy(() => Status("Start", "Center", "End"));
  var VersionedHostAccountConnectionStatusSubscribeError = lazy(() => indexedTaggedUnion({ V1: [0, GenericError] }));
  var VersionedHostAccountConnectionStatusSubscribeItem = lazy(() => indexedTaggedUnion({ V1: [0, HostAccountConnectionStatusSubscribeItem] }));
  var VersionedHostAccountConnectionStatusSubscribeRequest = lazy(() => indexedTaggedUnion({ V1: [0, _void] }));
  var VersionedHostAccountCreateProofError = lazy(() => indexedTaggedUnion({ V1: [0, HostAccountCreateProofError] }));
  var VersionedHostAccountCreateProofRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostAccountCreateProofRequest] }));
  var VersionedHostAccountCreateProofResponse = lazy(() => indexedTaggedUnion({ V1: [0, HostAccountCreateProofResponse] }));
  var VersionedHostAccountGetAliasError = lazy(() => indexedTaggedUnion({ V1: [0, HostAccountGetAliasError] }));
  var VersionedHostAccountGetAliasRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostAccountGetAliasRequest] }));
  var VersionedHostAccountGetAliasResponse = lazy(() => indexedTaggedUnion({ V1: [0, ContextualAlias] }));
  var VersionedHostAccountGetError = lazy(() => indexedTaggedUnion({ V1: [0, HostAccountGetError] }));
  var VersionedHostAccountGetRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostAccountGetRequest] }));
  var VersionedHostAccountGetResponse = lazy(() => indexedTaggedUnion({ V1: [0, HostAccountGetResponse] }));
  var VersionedHostAccountListRingVrfKeysError = lazy(() => indexedTaggedUnion({ V1: [0, HostAccountListRingVrfKeysError] }));
  var VersionedHostAccountListRingVrfKeysRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostAccountListRingVrfKeysRequest] }));
  var VersionedHostAccountListRingVrfKeysResponse = lazy(() => indexedTaggedUnion({ V1: [0, Vector(RegisteredRingVrfKey)] }));
  var VersionedHostAccountRegisterRingVrfKeyError = lazy(() => indexedTaggedUnion({ V1: [0, HostAccountRegisterRingVrfKeyError] }));
  var VersionedHostAccountRegisterRingVrfKeyRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostAccountRegisterRingVrfKeyRequest] }));
  var VersionedHostAccountRegisterRingVrfKeyResponse = lazy(() => indexedTaggedUnion({ V1: [0, Hex(32)] }));
  var VersionedHostAccountRingVrfSignError = lazy(() => indexedTaggedUnion({ V1: [0, HostAccountRingVrfSignError] }));
  var VersionedHostAccountRingVrfSignRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostAccountRingVrfSignRequest] }));
  var VersionedHostAccountRingVrfSignResponse = lazy(() => indexedTaggedUnion({ V1: [0, Hex()] }));
  var VersionedHostAccountSignVrfError = lazy(() => indexedTaggedUnion({ V1: [0, HostAccountSignVrfError] }));
  var VersionedHostAccountSignVrfRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostAccountSignVrfRequest] }));
  var VersionedHostAccountSignVrfResponse = lazy(() => indexedTaggedUnion({ V1: [0, VrfSignature] }));
  var VersionedHostCancelNextGameError = lazy(() => indexedTaggedUnion({ V1: [0, GenericError] }));
  var VersionedHostCancelNextGameRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostCancelNextGameRequest] }));
  var VersionedHostCancelNextGameResponse = lazy(() => indexedTaggedUnion({ V1: [0, _void] }));
  var VersionedHostChatActionSubscribeError = lazy(() => indexedTaggedUnion({ V1: [0, GenericError] }));
  var VersionedHostChatActionSubscribeItem = lazy(() => indexedTaggedUnion({ V1: [0, HostChatActionSubscribeItem] }));
  var VersionedHostChatActionSubscribeRequest = lazy(() => indexedTaggedUnion({ V1: [0, _void] }));
  var VersionedHostChatCreateRoomError = lazy(() => indexedTaggedUnion({ V1: [0, HostChatCreateRoomError] }));
  var VersionedHostChatCreateRoomRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostChatCreateRoomRequest] }));
  var VersionedHostChatCreateRoomResponse = lazy(() => indexedTaggedUnion({ V1: [0, HostChatCreateRoomResponse] }));
  var VersionedHostChatListSubscribeError = lazy(() => indexedTaggedUnion({ V1: [0, GenericError] }));
  var VersionedHostChatListSubscribeItem = lazy(() => indexedTaggedUnion({ V1: [0, HostChatListSubscribeItem] }));
  var VersionedHostChatListSubscribeRequest = lazy(() => indexedTaggedUnion({ V1: [0, _void] }));
  var VersionedHostChatPostMessageError = lazy(() => indexedTaggedUnion({ V1: [0, HostChatPostMessageError] }));
  var VersionedHostChatPostMessageRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostChatPostMessageRequest] }));
  var VersionedHostChatPostMessageResponse = lazy(() => indexedTaggedUnion({ V1: [0, HostChatPostMessageResponse] }));
  var VersionedHostChatRegisterBotError = lazy(() => indexedTaggedUnion({ V1: [0, HostChatRegisterBotError] }));
  var VersionedHostChatRegisterBotRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostChatRegisterBotRequest] }));
  var VersionedHostChatRegisterBotResponse = lazy(() => indexedTaggedUnion({ V1: [0, HostChatRegisterBotResponse] }));
  var VersionedHostCoinPaymentCreateChequeError = lazy(() => indexedTaggedUnion({ V1: [0, CoinPaymentError] }));
  var VersionedHostCoinPaymentCreateChequeRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostCoinPaymentCreateChequeRequest] }));
  var VersionedHostCoinPaymentCreateChequeResponse = lazy(() => indexedTaggedUnion({ V1: [0, HostCoinPaymentCreateChequeResponse] }));
  var VersionedHostCoinPaymentCreatePurseError = lazy(() => indexedTaggedUnion({ V1: [0, CoinPaymentError] }));
  var VersionedHostCoinPaymentCreatePurseRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostCoinPaymentCreatePurseRequest] }));
  var VersionedHostCoinPaymentCreatePurseResponse = lazy(() => indexedTaggedUnion({ V1: [0, HostCoinPaymentCreatePurseResponse] }));
  var VersionedHostCoinPaymentCreateReceivableError = lazy(() => indexedTaggedUnion({ V1: [0, CoinPaymentError] }));
  var VersionedHostCoinPaymentCreateReceivableRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostCoinPaymentCreateReceivableRequest] }));
  var VersionedHostCoinPaymentCreateReceivableResponse = lazy(() => indexedTaggedUnion({ V1: [0, HostCoinPaymentCreateReceivableResponse] }));
  var VersionedHostCoinPaymentDeletePurseError = lazy(() => indexedTaggedUnion({ V1: [0, CoinPaymentError] }));
  var VersionedHostCoinPaymentDeletePurseItem = lazy(() => indexedTaggedUnion({ V1: [0, CoinPaymentStatus] }));
  var VersionedHostCoinPaymentDeletePurseRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostCoinPaymentDeletePurseRequest] }));
  var VersionedHostCoinPaymentDepositError = lazy(() => indexedTaggedUnion({ V1: [0, CoinPaymentError] }));
  var VersionedHostCoinPaymentDepositItem = lazy(() => indexedTaggedUnion({ V1: [0, CoinPaymentStatus] }));
  var VersionedHostCoinPaymentDepositRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostCoinPaymentDepositRequest] }));
  var VersionedHostCoinPaymentListenForError = lazy(() => indexedTaggedUnion({ V1: [0, CoinPaymentError] }));
  var VersionedHostCoinPaymentListenForItem = lazy(() => indexedTaggedUnion({ V1: [0, HostCoinPaymentListenForItem] }));
  var VersionedHostCoinPaymentListenForRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostCoinPaymentListenForRequest] }));
  var VersionedHostCoinPaymentQueryPurseError = lazy(() => indexedTaggedUnion({ V1: [0, CoinPaymentError] }));
  var VersionedHostCoinPaymentQueryPurseRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostCoinPaymentQueryPurseRequest] }));
  var VersionedHostCoinPaymentQueryPurseResponse = lazy(() => indexedTaggedUnion({ V1: [0, HostCoinPaymentQueryPurseResponse] }));
  var VersionedHostCoinPaymentRebalancePurseError = lazy(() => indexedTaggedUnion({ V1: [0, CoinPaymentError] }));
  var VersionedHostCoinPaymentRebalancePurseItem = lazy(() => indexedTaggedUnion({ V1: [0, CoinPaymentStatus] }));
  var VersionedHostCoinPaymentRebalancePurseRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostCoinPaymentRebalancePurseRequest] }));
  var VersionedHostCoinPaymentRefundError = lazy(() => indexedTaggedUnion({ V1: [0, CoinPaymentError] }));
  var VersionedHostCoinPaymentRefundItem = lazy(() => indexedTaggedUnion({ V1: [0, CoinPaymentStatus] }));
  var VersionedHostCoinPaymentRefundRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostCoinPaymentRefundRequest] }));
  var VersionedHostContactsPickError = lazy(() => indexedTaggedUnion({ V1: [0, HostContactsPickError] }));
  var VersionedHostContactsPickRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostContactsPickRequest] }));
  var VersionedHostContactsPickResponse = lazy(() => indexedTaggedUnion({ V1: [0, HostContactsPickResponse] }));
  var VersionedHostCreateTransactionError = lazy(() => indexedTaggedUnion({ V1: [0, HostCreateTransactionError] }));
  var VersionedHostCreateTransactionRequest = lazy(() => indexedTaggedUnion({ V1: [0, ProductAccountTxPayload] }));
  var VersionedHostCreateTransactionResponse = lazy(() => indexedTaggedUnion({ V1: [0, HostCreateTransactionResponse] }));
  var VersionedHostCreateTransactionWithLegacyAccountError = lazy(() => indexedTaggedUnion({ V1: [0, HostCreateTransactionError] }));
  var VersionedHostCreateTransactionWithLegacyAccountRequest = lazy(() => indexedTaggedUnion({ V1: [0, LegacyAccountTxPayload] }));
  var VersionedHostCreateTransactionWithLegacyAccountResponse = lazy(() => indexedTaggedUnion({ V1: [0, HostCreateTransactionWithLegacyAccountResponse] }));
  var VersionedHostDeriveEntropyError = lazy(() => indexedTaggedUnion({ V1: [0, HostDeriveEntropyError] }));
  var VersionedHostDeriveEntropyRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostDeriveEntropyRequest] }));
  var VersionedHostDeriveEntropyResponse = lazy(() => indexedTaggedUnion({ V1: [0, HostDeriveEntropyResponse] }));
  var VersionedHostDevicePermissionError = lazy(() => indexedTaggedUnion({ V1: [0, GenericError] }));
  var VersionedHostDevicePermissionRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostDevicePermissionRequest] }));
  var VersionedHostDevicePermissionResponse = lazy(() => indexedTaggedUnion({ V1: [0, HostDevicePermissionResponse] }));
  var VersionedHostFeatureSupportedError = lazy(() => indexedTaggedUnion({ V1: [0, GenericError] }));
  var VersionedHostFeatureSupportedRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostFeatureSupportedRequest] }));
  var VersionedHostFeatureSupportedResponse = lazy(() => indexedTaggedUnion({ V1: [0, HostFeatureSupportedResponse] }));
  var VersionedHostGetLegacyAccountsError = lazy(() => indexedTaggedUnion({ V1: [0, HostAccountGetError] }));
  var VersionedHostGetLegacyAccountsRequest = lazy(() => indexedTaggedUnion({ V1: [0, _void] }));
  var VersionedHostGetLegacyAccountsResponse = lazy(() => indexedTaggedUnion({ V1: [0, HostGetLegacyAccountsResponse] }));
  var VersionedHostGetProductContextError = lazy(() => indexedTaggedUnion({ V1: [0, GenericError] }));
  var VersionedHostGetProductContextRequest = lazy(() => indexedTaggedUnion({ V1: [0, _void] }));
  var VersionedHostGetProductContextResponse = lazy(() => indexedTaggedUnion({ V1: [0, HostGetProductContextResponse] }));
  var VersionedHostGetUserIdError = lazy(() => indexedTaggedUnion({ V1: [0, HostGetUserIdError] }));
  var VersionedHostGetUserIdRequest = lazy(() => indexedTaggedUnion({ V1: [0, _void] }));
  var VersionedHostGetUserIdResponse = lazy(() => indexedTaggedUnion({ V1: [0, HostGetUserIdResponse] }));
  var VersionedHostHandshakeError = lazy(() => indexedTaggedUnion({ V1: [0, HostHandshakeError] }));
  var VersionedHostHandshakeRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostHandshakeRequest] }));
  var VersionedHostHandshakeResponse = lazy(() => indexedTaggedUnion({ V1: [0, _void] }));
  var HostInfo = lazy(() => Struct({ platform: HostPlatform, name: str, version: str }));
  var VersionedHostInfoError = lazy(() => indexedTaggedUnion({ V1: [0, GenericError] }));
  var VersionedHostInfoRequest = lazy(() => indexedTaggedUnion({ V1: [0, _void] }));
  var VersionedHostInfoResponse = lazy(() => indexedTaggedUnion({ V1: [0, HostInfo] }));
  var VersionedHostLocalStorageChangeItem = lazy(() => indexedTaggedUnion({ V1: [0, HostLocalStorageChangeItem] }));
  var VersionedHostLocalStorageClearError = lazy(() => indexedTaggedUnion({ V1: [0, V01HostLocalStorageReadError] }));
  var VersionedHostLocalStorageClearRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostLocalStorageClearRequest] }));
  var VersionedHostLocalStorageClearResponse = lazy(() => indexedTaggedUnion({ V1: [0, _void] }));
  var VersionedHostLocalStorageReadError = lazy(() => indexedTaggedUnion({ V2: [1, HostLocalStorageReadError] }));
  var VersionedHostLocalStorageReadRequest = lazy(() => indexedTaggedUnion({ V2: [1, HostLocalStorageReadRequest] }));
  var VersionedHostLocalStorageReadResponse = lazy(() => indexedTaggedUnion({ V2: [1, HostLocalStorageReadResponse] }));
  var VersionedHostLocalStorageSubscribeError = lazy(() => indexedTaggedUnion({ V1: [0, GenericError] }));
  var VersionedHostLocalStorageSubscribeRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostLocalStorageSubscribeRequest] }));
  var VersionedHostLocalStorageWriteError = lazy(() => indexedTaggedUnion({ V1: [0, V01HostLocalStorageReadError] }));
  var VersionedHostLocalStorageWriteRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostLocalStorageWriteRequest] }));
  var VersionedHostLocalStorageWriteResponse = lazy(() => indexedTaggedUnion({ V1: [0, _void] }));
  var VersionedHostLocaleSubscribeError = lazy(() => indexedTaggedUnion({ V1: [0, GenericError] }));
  var VersionedHostLocaleSubscribeItem = lazy(() => indexedTaggedUnion({ V1: [0, HostLocaleSubscribeItem] }));
  var VersionedHostLocaleSubscribeRequest = lazy(() => indexedTaggedUnion({ V1: [0, _void] }));
  var VersionedHostNavigateToError = lazy(() => indexedTaggedUnion({ V1: [0, HostNavigateToError] }));
  var VersionedHostNavigateToRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostNavigateToRequest] }));
  var VersionedHostNavigateToResponse = lazy(() => indexedTaggedUnion({ V1: [0, _void] }));
  var VersionedHostPaymentBalanceSubscribeError = lazy(() => indexedTaggedUnion({ V1: [0, HostPaymentBalanceSubscribeError] }));
  var VersionedHostPaymentBalanceSubscribeItem = lazy(() => indexedTaggedUnion({ V1: [0, HostPaymentBalanceSubscribeItem] }));
  var VersionedHostPaymentBalanceSubscribeRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostPaymentBalanceSubscribeRequest] }));
  var VersionedHostPaymentError = lazy(() => indexedTaggedUnion({ V1: [0, HostPaymentError] }));
  var VersionedHostPaymentRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostPaymentRequest] }));
  var VersionedHostPaymentResponse = lazy(() => indexedTaggedUnion({ V1: [0, HostPaymentResponse] }));
  var VersionedHostPaymentStatusSubscribeError = lazy(() => indexedTaggedUnion({ V1: [0, HostPaymentStatusSubscribeError] }));
  var VersionedHostPaymentStatusSubscribeItem = lazy(() => indexedTaggedUnion({ V1: [0, HostPaymentStatusSubscribeItem] }));
  var VersionedHostPaymentStatusSubscribeRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostPaymentStatusSubscribeRequest] }));
  var VersionedHostPaymentTopUpError = lazy(() => indexedTaggedUnion({ V1: [0, HostPaymentTopUpError] }));
  var VersionedHostPaymentTopUpRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostPaymentTopUpRequest] }));
  var VersionedHostPaymentTopUpResponse = lazy(() => indexedTaggedUnion({ V1: [0, _void] }));
  var HostPlatform = lazy(() => Status("Web", "Android", "Ios", "Desktop", "Cli", "Unknown"));
  var VersionedHostPocketListSubscribeError = lazy(() => indexedTaggedUnion({ V1: [0, GenericError] }));
  var VersionedHostPocketListSubscribeItem = lazy(() => indexedTaggedUnion({ V1: [0, HostPocketListSubscribeItem] }));
  var VersionedHostPocketListSubscribeRequest = lazy(() => indexedTaggedUnion({ V1: [0, _void] }));
  var VersionedHostPocketRemoveCardError = lazy(() => indexedTaggedUnion({ V1: [0, HostPocketRemoveCardError] }));
  var VersionedHostPocketRemoveCardRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostPocketRemoveCardRequest] }));
  var VersionedHostPocketRemoveCardResponse = lazy(() => indexedTaggedUnion({ V1: [0, _void] }));
  var VersionedHostPushNotificationCancelError = lazy(() => indexedTaggedUnion({ V1: [0, GenericError] }));
  var VersionedHostPushNotificationCancelRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostPushNotificationCancelRequest] }));
  var VersionedHostPushNotificationCancelResponse = lazy(() => indexedTaggedUnion({ V1: [0, _void] }));
  var VersionedHostPushNotificationError = lazy(() => indexedTaggedUnion({ V1: [0, HostPushNotificationError] }));
  var VersionedHostPushNotificationRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostPushNotificationRequest] }));
  var VersionedHostPushNotificationResponse = lazy(() => indexedTaggedUnion({ V1: [0, HostPushNotificationResponse] }));
  var VersionedHostRemindNextGameError = lazy(() => indexedTaggedUnion({ V1: [0, HostRemindNextGameError] }));
  var VersionedHostRemindNextGameRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostRemindNextGameRequest] }));
  var VersionedHostRemindNextGameResponse = lazy(() => indexedTaggedUnion({ V1: [0, _void] }));
  var VersionedHostRendererActionSubscribeError = lazy(() => indexedTaggedUnion({ V1: [0, GenericError] }));
  var VersionedHostRendererActionSubscribeItem = lazy(() => indexedTaggedUnion({ V1: [0, HostRendererActionSubscribeItem] }));
  var VersionedHostRendererActionSubscribeRequest = lazy(() => indexedTaggedUnion({ V1: [0, _void] }));
  var VersionedHostRequestLoginError = lazy(() => indexedTaggedUnion({ V1: [0, HostRequestLoginError] }));
  var VersionedHostRequestLoginRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostRequestLoginRequest] }));
  var VersionedHostRequestLoginResponse = lazy(() => indexedTaggedUnion({ V1: [0, HostRequestLoginResponse] }));
  var VersionedHostRequestResourceAllocationError = lazy(() => indexedTaggedUnion({ V1: [0, ResourceAllocationError] }));
  var VersionedHostRequestResourceAllocationRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostRequestResourceAllocationRequest] }));
  var VersionedHostRequestResourceAllocationResponse = lazy(() => indexedTaggedUnion({ V1: [0, HostRequestResourceAllocationResponse] }));
  var VersionedHostScannerScanError = lazy(() => indexedTaggedUnion({ V1: [0, HostScannerScanError] }));
  var VersionedHostScannerScanRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostScannerScanRequest] }));
  var VersionedHostScannerScanResponse = lazy(() => indexedTaggedUnion({ V1: [0, HostScannerScanResponse] }));
  var HostSignPayloadData = lazy(() => Struct({ blockHash: Hex(), blockNumber: Hex(), era: Hex(), genesisHash: Hex(), method: Hex(), nonce: Hex(), specVersion: Hex(), tip: Hex(), transactionVersion: Hex(), signedExtensions: Vector(str), version: u32, assetId: Option(Hex()), metadataHash: Option(Hex()), mode: Option(u32), withSignedTransaction: OptionBool }));
  var VersionedHostSignPayloadError = lazy(() => indexedTaggedUnion({ V1: [0, HostSignPayloadError] }));
  var VersionedHostSignPayloadRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostSignPayloadRequest] }));
  var VersionedHostSignPayloadResponse = lazy(() => indexedTaggedUnion({ V1: [0, HostSignPayloadResponse] }));
  var VersionedHostSignPayloadWithLegacyAccountError = lazy(() => indexedTaggedUnion({ V1: [0, HostSignPayloadError] }));
  var VersionedHostSignPayloadWithLegacyAccountRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostSignPayloadWithLegacyAccountRequest] }));
  var VersionedHostSignPayloadWithLegacyAccountResponse = lazy(() => indexedTaggedUnion({ V1: [0, HostSignPayloadResponse] }));
  var VersionedHostSignRawError = lazy(() => indexedTaggedUnion({ V1: [0, HostSignPayloadError] }));
  var VersionedHostSignRawRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostSignRawRequest] }));
  var VersionedHostSignRawResponse = lazy(() => indexedTaggedUnion({ V1: [0, HostSignPayloadResponse] }));
  var VersionedHostSignRawWithLegacyAccountError = lazy(() => indexedTaggedUnion({ V1: [0, HostSignPayloadError] }));
  var VersionedHostSignRawWithLegacyAccountRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostSignRawWithLegacyAccountRequest] }));
  var VersionedHostSignRawWithLegacyAccountResponse = lazy(() => indexedTaggedUnion({ V1: [0, HostSignPayloadResponse] }));
  var VersionedHostThemeSubscribeError = lazy(() => indexedTaggedUnion({ V1: [0, GenericError] }));
  var VersionedHostThemeSubscribeItem = lazy(() => indexedTaggedUnion({ V1: [0, HostThemeSubscribeItem] }));
  var VersionedHostThemeSubscribeRequest = lazy(() => indexedTaggedUnion({ V1: [0, _void] }));
  var VersionedHostWorkerBeginOperationError = lazy(() => indexedTaggedUnion({ V1: [0, HostWorkerOperationError] }));
  var VersionedHostWorkerBeginOperationRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostWorkerBeginOperationRequest] }));
  var VersionedHostWorkerBeginOperationResponse = lazy(() => indexedTaggedUnion({ V1: [0, HostWorkerBeginOperationResponse] }));
  var VersionedHostWorkerEndOperationError = lazy(() => indexedTaggedUnion({ V1: [0, HostWorkerOperationError] }));
  var VersionedHostWorkerEndOperationRequest = lazy(() => indexedTaggedUnion({ V1: [0, HostWorkerEndOperationRequest] }));
  var VersionedHostWorkerEndOperationResponse = lazy(() => indexedTaggedUnion({ V1: [0, _void] }));
  var HostWorkerOperationError = lazy(() => TaggedUnion({ TooManyOpen: _void, Unknown: Struct({ reason: str }) }));
  var ImageFit = lazy(() => Status("None", "Fill", "Cover", "Contain", "ScaleDown"));
  var ImageProps = lazy(() => Struct({ source: ImageSource, fit: Option(ImageFit) }));
  var ImageSource = lazy(() => TaggedUnion({ Bulletin: str, Archive: str }));
  var LegacyAccount = lazy(() => Struct({ publicKey: Hex(), name: Option(str) }));
  var LegacyAccountTxPayload = lazy(() => Struct({ signer: Hex(32), genesisHash: Hex(32), callData: Hex(), extensions: Vector(TxPayloadExtension), txExtVersion: u8 }));
  var Modifier = lazy(() => TaggedUnion({ Margin: Dimensions, Padding: Dimensions, Background, Border: BorderStyle, Height: Size, Width: Size, MinWidth: Size, MinHeight: Size, FillWidth: bool2, FillHeight: bool2, Opacity: u8, BlendingMode }));
  var OperationStartedResult = lazy(() => TaggedUnion({ Started: Struct({ operationId: str }), LimitReached: _void }));
  var PaymentTopUpSource = lazy(() => TaggedUnion({ ProductAccount: Struct({ derivationIndex: DerivationIndex }), PrivateKey: Struct({ sr25519SecretKey: Hex(64) }), Coins: Struct({ sr25519SecretKeys: Vector(Hex(64)) }) }));
  var PocketCard = lazy(() => Struct({ cardId: str, privileged: bool2 }));
  var PreimageSubmitError = lazy(() => TaggedUnion({ Unknown: Struct({ reason: str }) }));
  var ProductAccount = lazy(() => Struct({ publicKey: Hex() }));
  var ProductAccountId = lazy(() => Struct({ dotNsIdentifier: str, derivationIndex: DerivationIndex }));
  var ProductAccountTxPayload = lazy(() => Struct({ signer: ProductAccountId, genesisHash: Hex(32), callData: Hex(), extensions: Vector(TxPayloadExtension), txExtVersion: u8, contacts: Vector(ContactHandle) }));
  var ProductProofContext = lazy(() => Struct({ productId: str, suffix: DerivationIndex }));
  var VersionedProductRendererRenderError = lazy(() => indexedTaggedUnion({ V1: [0, GenericError] }));
  var VersionedProductRendererRenderItem = lazy(() => indexedTaggedUnion({ V1: [0, RendererNode] }));
  var VersionedProductRendererRenderRequest = lazy(() => indexedTaggedUnion({ V1: [0, ProductRendererRenderRequest] }));
  var RawPayload = lazy(() => TaggedUnion({ Bytes: Struct({ bytes: Hex() }), Payload: Struct({ payload: str }) }));
  var RegisteredRingVrfKey = lazy(() => Struct({ handle: ProductAccountId, rings: Vector(RingLocation), publicKey: Option(Hex(32)) }));
  var VersionedRemoteChainHeadBodyError = lazy(() => indexedTaggedUnion({ V1: [0, GenericError] }));
  var VersionedRemoteChainHeadBodyRequest = lazy(() => indexedTaggedUnion({ V1: [0, RemoteChainHeadBodyRequest] }));
  var VersionedRemoteChainHeadBodyResponse = lazy(() => indexedTaggedUnion({ V1: [0, RemoteChainHeadBodyResponse] }));
  var VersionedRemoteChainHeadCallError = lazy(() => indexedTaggedUnion({ V1: [0, GenericError] }));
  var VersionedRemoteChainHeadCallRequest = lazy(() => indexedTaggedUnion({ V1: [0, RemoteChainHeadCallRequest] }));
  var VersionedRemoteChainHeadCallResponse = lazy(() => indexedTaggedUnion({ V1: [0, RemoteChainHeadCallResponse] }));
  var VersionedRemoteChainHeadContinueError = lazy(() => indexedTaggedUnion({ V1: [0, GenericError] }));
  var VersionedRemoteChainHeadContinueRequest = lazy(() => indexedTaggedUnion({ V1: [0, RemoteChainHeadContinueRequest] }));
  var VersionedRemoteChainHeadContinueResponse = lazy(() => indexedTaggedUnion({ V1: [0, _void] }));
  var VersionedRemoteChainHeadFollowError = lazy(() => indexedTaggedUnion({ V1: [0, GenericError] }));
  var VersionedRemoteChainHeadFollowItem = lazy(() => indexedTaggedUnion({ V1: [0, RemoteChainHeadFollowItem] }));
  var VersionedRemoteChainHeadFollowRequest = lazy(() => indexedTaggedUnion({ V1: [0, RemoteChainHeadFollowRequest] }));
  var VersionedRemoteChainHeadHeaderError = lazy(() => indexedTaggedUnion({ V1: [0, GenericError] }));
  var VersionedRemoteChainHeadHeaderRequest = lazy(() => indexedTaggedUnion({ V1: [0, RemoteChainHeadHeaderRequest] }));
  var VersionedRemoteChainHeadHeaderResponse = lazy(() => indexedTaggedUnion({ V1: [0, RemoteChainHeadHeaderResponse] }));
  var VersionedRemoteChainHeadStopOperationError = lazy(() => indexedTaggedUnion({ V1: [0, GenericError] }));
  var VersionedRemoteChainHeadStopOperationRequest = lazy(() => indexedTaggedUnion({ V1: [0, RemoteChainHeadStopOperationRequest] }));
  var VersionedRemoteChainHeadStopOperationResponse = lazy(() => indexedTaggedUnion({ V1: [0, _void] }));
  var VersionedRemoteChainHeadStorageError = lazy(() => indexedTaggedUnion({ V1: [0, GenericError] }));
  var VersionedRemoteChainHeadStorageRequest = lazy(() => indexedTaggedUnion({ V1: [0, RemoteChainHeadStorageRequest] }));
  var VersionedRemoteChainHeadStorageResponse = lazy(() => indexedTaggedUnion({ V1: [0, RemoteChainHeadStorageResponse] }));
  var VersionedRemoteChainHeadUnpinError = lazy(() => indexedTaggedUnion({ V1: [0, GenericError] }));
  var VersionedRemoteChainHeadUnpinRequest = lazy(() => indexedTaggedUnion({ V1: [0, RemoteChainHeadUnpinRequest] }));
  var VersionedRemoteChainHeadUnpinResponse = lazy(() => indexedTaggedUnion({ V1: [0, _void] }));
  var VersionedRemoteChainInfoError = lazy(() => indexedTaggedUnion({ V1: [0, RemoteChainInfoError] }));
  var VersionedRemoteChainInfoRequest = lazy(() => indexedTaggedUnion({ V1: [0, RemoteChainInfoRequest] }));
  var VersionedRemoteChainInfoResponse = lazy(() => indexedTaggedUnion({ V1: [0, RemoteChainInfoResponse] }));
  var VersionedRemoteChainSpecChainNameError = lazy(() => indexedTaggedUnion({ V1: [0, GenericError] }));
  var VersionedRemoteChainSpecChainNameRequest = lazy(() => indexedTaggedUnion({ V1: [0, RemoteChainSpecChainNameRequest] }));
  var VersionedRemoteChainSpecChainNameResponse = lazy(() => indexedTaggedUnion({ V1: [0, RemoteChainSpecChainNameResponse] }));
  var VersionedRemoteChainSpecGenesisHashError = lazy(() => indexedTaggedUnion({ V1: [0, GenericError] }));
  var VersionedRemoteChainSpecGenesisHashRequest = lazy(() => indexedTaggedUnion({ V1: [0, RemoteChainSpecGenesisHashRequest] }));
  var VersionedRemoteChainSpecGenesisHashResponse = lazy(() => indexedTaggedUnion({ V1: [0, RemoteChainSpecGenesisHashResponse] }));
  var VersionedRemoteChainSpecPropertiesError = lazy(() => indexedTaggedUnion({ V1: [0, GenericError] }));
  var VersionedRemoteChainSpecPropertiesRequest = lazy(() => indexedTaggedUnion({ V1: [0, RemoteChainSpecPropertiesRequest] }));
  var VersionedRemoteChainSpecPropertiesResponse = lazy(() => indexedTaggedUnion({ V1: [0, RemoteChainSpecPropertiesResponse] }));
  var VersionedRemoteChainTransactionBroadcastError = lazy(() => indexedTaggedUnion({ V1: [0, GenericError] }));
  var VersionedRemoteChainTransactionBroadcastRequest = lazy(() => indexedTaggedUnion({ V1: [0, RemoteChainTransactionBroadcastRequest] }));
  var VersionedRemoteChainTransactionBroadcastResponse = lazy(() => indexedTaggedUnion({ V1: [0, RemoteChainTransactionBroadcastResponse] }));
  var VersionedRemoteChainTransactionStopError = lazy(() => indexedTaggedUnion({ V1: [0, GenericError] }));
  var VersionedRemoteChainTransactionStopRequest = lazy(() => indexedTaggedUnion({ V1: [0, RemoteChainTransactionStopRequest] }));
  var VersionedRemoteChainTransactionStopResponse = lazy(() => indexedTaggedUnion({ V1: [0, _void] }));
  var RemotePermission = lazy(() => TaggedUnion({ Remote: Struct({ domains: Vector(str) }), WebRtc: _void, ChainSubmit: _void, PreimageSubmit: _void, StatementSubmit: _void }));
  var VersionedRemotePermissionError = lazy(() => indexedTaggedUnion({ V1: [0, GenericError] }));
  var VersionedRemotePermissionRequest = lazy(() => indexedTaggedUnion({ V1: [0, RemotePermissionRequest] }));
  var VersionedRemotePermissionResponse = lazy(() => indexedTaggedUnion({ V1: [0, RemotePermissionResponse] }));
  var VersionedRemotePreimageLookupSubscribeError = lazy(() => indexedTaggedUnion({ V1: [0, GenericError] }));
  var VersionedRemotePreimageLookupSubscribeItem = lazy(() => indexedTaggedUnion({ V1: [0, RemotePreimageLookupSubscribeItem] }));
  var VersionedRemotePreimageLookupSubscribeRequest = lazy(() => indexedTaggedUnion({ V1: [0, RemotePreimageLookupSubscribeRequest] }));
  var VersionedRemotePreimageSubmitError = lazy(() => indexedTaggedUnion({ V1: [0, PreimageSubmitError] }));
  var VersionedRemotePreimageSubmitRequest = lazy(() => indexedTaggedUnion({ V1: [0, Hex()] }));
  var VersionedRemotePreimageSubmitResponse = lazy(() => indexedTaggedUnion({ V1: [0, Hex()] }));
  var VersionedRemoteStatementStoreCreateProofAuthorizedError = lazy(() => indexedTaggedUnion({ V1: [0, RemoteStatementStoreCreateProofError] }));
  var VersionedRemoteStatementStoreCreateProofAuthorizedRequest = lazy(() => indexedTaggedUnion({ V1: [0, Statement] }));
  var VersionedRemoteStatementStoreCreateProofAuthorizedResponse = lazy(() => indexedTaggedUnion({ V1: [0, RemoteStatementStoreCreateProofResponse] }));
  var VersionedRemoteStatementStoreCreateProofError = lazy(() => indexedTaggedUnion({ V1: [0, RemoteStatementStoreCreateProofError] }));
  var VersionedRemoteStatementStoreCreateProofRequest = lazy(() => indexedTaggedUnion({ V1: [0, RemoteStatementStoreCreateProofRequest] }));
  var VersionedRemoteStatementStoreCreateProofResponse = lazy(() => indexedTaggedUnion({ V1: [0, RemoteStatementStoreCreateProofResponse] }));
  var VersionedRemoteStatementStoreSubmitError = lazy(() => indexedTaggedUnion({ V1: [0, GenericError] }));
  var VersionedRemoteStatementStoreSubmitRequest = lazy(() => indexedTaggedUnion({ V1: [0, SignedStatement] }));
  var VersionedRemoteStatementStoreSubmitResponse = lazy(() => indexedTaggedUnion({ V1: [0, _void] }));
  var VersionedRemoteStatementStoreSubscribeError = lazy(() => indexedTaggedUnion({ V1: [0, GenericError] }));
  var VersionedRemoteStatementStoreSubscribeItem = lazy(() => indexedTaggedUnion({ V1: [0, RemoteStatementStoreSubscribeItem] }));
  var VersionedRemoteStatementStoreSubscribeRequest = lazy(() => indexedTaggedUnion({ V1: [0, RemoteStatementStoreSubscribeRequest] }));
  var RenderContext = lazy(() => TaggedUnion({ ChatMessage: Struct({ roomId: str, messageId: str, messageType: str }), InputWidget: Struct({ candidateId: str }), PocketCard: Struct({ cardId: str }) }));
  var RendererNode = lazy(() => TaggedUnion({ Nil: _void, String: Struct({ text: str }), Box: Struct({ modifiers: Vector(Modifier), props: BoxProps, children: Vector(RendererNode) }), Column: Struct({ modifiers: Vector(Modifier), props: ColumnProps, children: Vector(RendererNode) }), Row: Struct({ modifiers: Vector(Modifier), props: RowProps, children: Vector(RendererNode) }), Spacer: Struct({ modifiers: Vector(Modifier) }), Text: Struct({ modifiers: Vector(Modifier), props: TextProps, children: Vector(RendererNode) }), Button: Struct({ modifiers: Vector(Modifier), props: ButtonProps, children: Vector(RendererNode) }), TextField: Struct({ modifiers: Vector(Modifier), props: TextFieldProps }), Image: Struct({ modifiers: Vector(Modifier), props: ImageProps }), Effect: Struct({ props: EffectProps, children: Vector(RendererNode) }) }));
  var ResourceAllocationError = lazy(() => TaggedUnion({ Unknown: Struct({ reason: str }) }));
  var RingLocation = lazy(() => Struct({ chainId: Hex(32), junctions: Vector(RingLocationJunction) }));
  var RingLocationJunction = lazy(() => TaggedUnion({ PalletInstance: u8, CollectionId: Hex() }));
  var RingVrfKeyDisclosure = lazy(() => Status("Anonymized", "PublicKey"));
  var RowProps = lazy(() => Struct({ verticalAlignment: Option(VerticalAlignment), horizontalArrangement: Option(Arrangement) }));
  var RuntimeApi = lazy(() => Struct({ name: str, version: u32 }));
  var RuntimeSpec = lazy(() => Struct({ specName: str, implName: str, specVersion: u32, implVersion: u32, transactionVersion: Option(u32), apis: Vector(RuntimeApi) }));
  var RuntimeType = lazy(() => TaggedUnion({ Valid: RuntimeSpec, Invalid: Struct({ error: str }) }));
  var ScanOutcome = lazy(() => TaggedUnion({ Scanned: Struct({ text: str, format: CodeFormat }), Dismissed: _void }));
  var Shape = lazy(() => TaggedUnion({ Rounded: Size, Circle: _void, Square: _void }));
  var SignedStatement = lazy(() => Struct({ proof: StatementProof, decryptionKey: Option(Hex(32)), expiry: Option(u64), channel: Option(Hex(32)), topics: Vector(Hex(32)), data: Option(Hex()) }));
  var Size = lazy(() => compact);
  var Statement = lazy(() => Struct({ proof: Option(StatementProof), decryptionKey: Option(Hex(32)), expiry: Option(u64), channel: Option(Hex(32)), topics: Vector(Hex(32)), data: Option(Hex()) }));
  var StatementProof = lazy(() => TaggedUnion({ Sr25519: Struct({ signature: Hex(64), signer: Hex(32) }), Ed25519: Struct({ signature: Hex(64), signer: Hex(32) }), Ecdsa: Struct({ signature: Hex(65), signer: Hex(33) }), OnChain: Struct({ who: Hex(32), blockHash: Hex(32), event: u64 }) }));
  var StorageQueryItem = lazy(() => Struct({ key: Hex(), queryType: StorageQueryType }));
  var StorageQueryType = lazy(() => Status("Value", "Hash", "ClosestDescendantMerkleValue", "DescendantsValues", "DescendantsHashes"));
  var StorageResultItem = lazy(() => Struct({ key: Hex(), value: Option(Hex()), hash: Option(Hex()), closestDescendantMerkleValue: Option(Hex()) }));
  var TextFieldProps = lazy(() => Struct({ text: str, placeholder: Option(str), label: Option(str), enabled: OptionBool, valueChangeAction: Option(str) }));
  var TextProps = lazy(() => Struct({ style: Option(TypographyStyle), color: Option(ColorToken) }));
  var ThemeName = lazy(() => TaggedUnion({ Custom: str, Default: _void }));
  var ThemeVariant = lazy(() => Status("Light", "Dark"));
  var TxPayloadExtension = lazy(() => Struct({ id: str, extra: Hex(), additionalSigned: Hex() }));
  var TypographyStyle = lazy(() => Status("HeadlineLarge", "TitleMediumRegular", "BodyLargeRegular", "BodyMediumRegular", "BodySmallRegular"));
  var HostAccountConnectionStatusSubscribeItem = lazy(() => Status("Disconnected", "Connected"));
  var HostAccountCreateProofError = lazy(() => TaggedUnion({ RingNotFound: _void, NotMember: _void, KeyNotRegistered: _void, KeyNotInRing: _void, NotAllowlisted: _void, Rejected: _void, Unknown: Struct({ reason: str }) }));
  var HostAccountCreateProofRequest = lazy(() => Struct({ keyHandle: ProductAccountId, context: ProductProofContext, ringLocation: RingLocation, message: Hex() }));
  var HostAccountCreateProofResponse = lazy(() => Struct({ proof: Hex(), contextualAlias: ContextualAlias, ringIndex: u32, ringRevision: u32 }));
  var HostAccountGetAliasError = lazy(() => TaggedUnion({ RingNotFound: _void, NotMember: _void, KeyNotRegistered: _void, KeyNotInRing: _void, Rejected: _void, Unknown: Struct({ reason: str }) }));
  var HostAccountGetAliasRequest = lazy(() => Struct({ keyHandle: ProductAccountId, context: ProductProofContext, ringLocation: RingLocation }));
  var HostAccountGetError = lazy(() => TaggedUnion({ NotConnected: _void, Rejected: _void, DomainNotValid: _void, Unknown: Struct({ reason: str }) }));
  var HostAccountGetRequest = lazy(() => Struct({ productAccountId: ProductAccountId }));
  var HostAccountGetResponse = lazy(() => Struct({ account: ProductAccount }));
  var HostAccountListRingVrfKeysError = lazy(() => TaggedUnion({ NotConnected: _void, Rejected: _void, Unknown: Struct({ reason: str }) }));
  var HostAccountListRingVrfKeysRequest = lazy(() => Struct({ owner: str, disclosure: RingVrfKeyDisclosure }));
  var HostAccountRegisterRingVrfKeyError = lazy(() => TaggedUnion({ NotConnected: _void, RingNotFound: _void, Rejected: _void, Unknown: Struct({ reason: str }) }));
  var HostAccountRegisterRingVrfKeyRequest = lazy(() => Struct({ index: DerivationIndex, ring: RingLocation }));
  var HostAccountRingVrfSignError = lazy(() => TaggedUnion({ NotConnected: _void, KeyNotRegistered: _void, NotAllowlisted: _void, Rejected: _void, Unknown: Struct({ reason: str }) }));
  var HostAccountRingVrfSignRequest = lazy(() => Struct({ keyHandle: ProductAccountId, message: Hex() }));
  var HostAccountSignVrfError = lazy(() => TaggedUnion({ NotConnected: _void, Rejected: _void, Unknown: Struct({ reason: str }) }));
  var HostAccountSignVrfRequest = lazy(() => Struct({ account: ProductAccountId, transcriptLabel: Hex(), items: Vector(VrfTranscriptItem) }));
  var HostCancelNextGameRequest = lazy(() => Struct({}));
  var HostChatActionSubscribeItem = lazy(() => Struct({ roomId: str, peer: str, payload: ChatActionPayload }));
  var HostChatCreateRoomError = lazy(() => TaggedUnion({ PermissionDenied: _void, Unknown: Struct({ reason: str }) }));
  var HostChatCreateRoomRequest = lazy(() => Struct({ roomId: str, name: str, icon: str }));
  var HostChatCreateRoomResponse = lazy(() => Struct({ status: ChatRoomRegistrationStatus }));
  var HostChatListSubscribeItem = lazy(() => Struct({ rooms: Vector(ChatRoom) }));
  var HostChatPostMessageError = lazy(() => TaggedUnion({ MessageTooLarge: _void, Unknown: Struct({ reason: str }) }));
  var HostChatPostMessageRequest = lazy(() => Struct({ roomId: str, payload: ChatMessageContent }));
  var HostChatPostMessageResponse = lazy(() => Struct({ messageId: str }));
  var HostChatRegisterBotError = lazy(() => TaggedUnion({ PermissionDenied: _void, Unknown: Struct({ reason: str }) }));
  var HostChatRegisterBotRequest = lazy(() => Struct({ botId: str, name: str, icon: str }));
  var HostChatRegisterBotResponse = lazy(() => Struct({ status: ChatBotRegistrationStatus }));
  var HostCoinPaymentCreateChequeRequest = lazy(() => Struct({ from: u32, to: Hex(32), amount: u32 }));
  var HostCoinPaymentCreateChequeResponse = lazy(() => Struct({ cheque: CoinPaymentCheque }));
  var HostCoinPaymentCreatePurseRequest = lazy(() => Struct({ name: str }));
  var HostCoinPaymentCreatePurseResponse = lazy(() => Struct({ purse: u32 }));
  var HostCoinPaymentCreateReceivableRequest = lazy(() => Struct({ into: u32 }));
  var HostCoinPaymentCreateReceivableResponse = lazy(() => Struct({ receivable: Hex(32) }));
  var HostCoinPaymentDeletePurseRequest = lazy(() => Struct({ target: u32, drainInto: u32 }));
  var HostCoinPaymentDepositRequest = lazy(() => Struct({ cheque: CoinPaymentCheque }));
  var HostCoinPaymentListenForItem = lazy(() => TaggedUnion({ Channel: CoinPaymentTransmissionChannel, Cheque: CoinPaymentCheque }));
  var HostCoinPaymentListenForRequest = lazy(() => Struct({ receivable: Hex(32) }));
  var HostCoinPaymentQueryPurseRequest = lazy(() => Struct({ purse: u32 }));
  var HostCoinPaymentQueryPurseResponse = lazy(() => Struct({ info: CoinPaymentPurseInfo }));
  var HostCoinPaymentRebalancePurseRequest = lazy(() => Struct({ from: u32, to: u32, amount: u32 }));
  var HostCoinPaymentRefundRequest = lazy(() => Struct({ receivable: Hex(32) }));
  var HostContactsPickError = lazy(() => TaggedUnion({ NotConnected: _void, Unknown: Struct({ reason: str }) }));
  var HostContactsPickRequest = lazy(() => Struct({}));
  var HostContactsPickResponse = lazy(() => Struct({ outcome: ContactPickOutcome }));
  var HostCreateTransactionError = lazy(() => TaggedUnion({ FailedToDecode: _void, Rejected: _void, NotSupported: Struct({ reason: str }), PermissionDenied: _void, Unknown: Struct({ reason: str }), UnknownContact: _void }));
  var HostCreateTransactionResponse = lazy(() => Struct({ transaction: Hex() }));
  var HostCreateTransactionWithLegacyAccountResponse = lazy(() => Struct({ transaction: Hex() }));
  var HostDeriveEntropyError = lazy(() => TaggedUnion({ Unknown: Struct({ reason: str }) }));
  var HostDeriveEntropyRequest = lazy(() => Struct({ context: Hex() }));
  var HostDeriveEntropyResponse = lazy(() => Struct({ entropy: Hex(32) }));
  var HostDevicePermissionRequest = lazy(() => Status("Notifications", "Camera", "Microphone", "Bluetooth", "NFC", "Location", "Clipboard", "OpenUrl", "Biometrics"));
  var HostDevicePermissionResponse = lazy(() => Struct({ granted: bool2 }));
  var HostFeatureSupportedRequest = lazy(() => TaggedUnion({ Chain: Struct({ genesisHash: Hex() }) }));
  var HostFeatureSupportedResponse = lazy(() => Struct({ supported: bool2 }));
  var HostGetLegacyAccountsResponse = lazy(() => Struct({ accounts: Vector(LegacyAccount) }));
  var HostGetProductContextResponse = lazy(() => Struct({ productId: str }));
  var HostGetUserIdError = lazy(() => TaggedUnion({ PermissionDenied: _void, NotConnected: _void, Unknown: Struct({ reason: str }) }));
  var HostGetUserIdResponse = lazy(() => Struct({ primaryUsername: str }));
  var HostHandshakeError = lazy(() => TaggedUnion({ Timeout: _void, UnsupportedProtocolVersion: _void, Unknown: GenericError }));
  var HostHandshakeRequest = lazy(() => Struct({ codecVersion: u8 }));
  var HostLocalStorageChangeItem = lazy(() => Struct({ value: Option(Hex()) }));
  var HostLocalStorageClearRequest = lazy(() => Struct({ key: str }));
  var V01HostLocalStorageReadError = lazy(() => TaggedUnion({ Full: _void, Unknown: Struct({ reason: str }) }));
  var V01HostLocalStorageReadRequest = lazy(() => Struct({ key: str }));
  var HostLocalStorageReadResponse = lazy(() => Struct({ value: Option(Hex()) }));
  var HostLocalStorageSubscribeRequest = lazy(() => Struct({ key: str }));
  var HostLocalStorageWriteRequest = lazy(() => Struct({ key: str, value: Hex() }));
  var HostLocaleSubscribeItem = lazy(() => Struct({ languageTag: str }));
  var HostNavigateToError = lazy(() => TaggedUnion({ PermissionDenied: _void, Unknown: Struct({ reason: str }) }));
  var HostNavigateToRequest = lazy(() => Struct({ url: str }));
  var HostPaymentBalanceSubscribeError = lazy(() => TaggedUnion({ PermissionDenied: _void, Unknown: Struct({ reason: str }) }));
  var HostPaymentBalanceSubscribeItem = lazy(() => Struct({ available: u128 }));
  var HostPaymentBalanceSubscribeRequest = lazy(() => Struct({ purse: Option(u32) }));
  var HostPaymentError = lazy(() => TaggedUnion({ Rejected: _void, InsufficientBalance: _void, Unknown: Struct({ reason: str }) }));
  var HostPaymentRequest = lazy(() => Struct({ from: Option(u32), amount: u128, destination: Hex(32) }));
  var HostPaymentResponse = lazy(() => Struct({ id: str }));
  var HostPaymentStatusSubscribeError = lazy(() => TaggedUnion({ PaymentNotFound: _void, Unknown: Struct({ reason: str }) }));
  var HostPaymentStatusSubscribeItem = lazy(() => TaggedUnion({ Processing: _void, Completed: _void, Failed: Struct({ reason: str }) }));
  var HostPaymentStatusSubscribeRequest = lazy(() => Struct({ paymentId: str }));
  var HostPaymentTopUpError = lazy(() => TaggedUnion({ InsufficientFunds: _void, InvalidSource: _void, PartialPayment: Struct({ credited: u128 }), Unknown: Struct({ reason: str }) }));
  var HostPaymentTopUpRequest = lazy(() => Struct({ into: Option(u32), amount: u128, source: PaymentTopUpSource }));
  var HostPocketListSubscribeItem = lazy(() => Struct({ cards: Vector(PocketCard) }));
  var HostPocketRemoveCardError = lazy(() => TaggedUnion({ Privileged: _void, Unknown: Struct({ reason: str }) }));
  var HostPocketRemoveCardRequest = lazy(() => Struct({ cardId: str }));
  var HostPushNotificationCancelRequest = lazy(() => Struct({ id: u32 }));
  var HostPushNotificationError = lazy(() => TaggedUnion({ ScheduleLimitReached: _void, Unknown: Struct({ reason: str }) }));
  var HostPushNotificationRequest = lazy(() => Struct({ text: str, deeplink: Option(str), scheduledAt: Option(u64) }));
  var HostPushNotificationResponse = lazy(() => Struct({ id: u32 }));
  var HostRemindNextGameError = lazy(() => Status("StartsInPast"));
  var HostRemindNextGameRequest = lazy(() => Struct({ startsAt: u64 }));
  var HostRendererActionSubscribeItem = lazy(() => Struct({ context: RenderContext, actionId: str, payload: Hex() }));
  var HostRequestLoginError = lazy(() => TaggedUnion({ Unknown: Struct({ reason: str }) }));
  var HostRequestLoginRequest = lazy(() => Struct({ reason: Option(str) }));
  var HostRequestLoginResponse = lazy(() => Status("Success", "AlreadyConnected", "Rejected"));
  var HostRequestResourceAllocationRequest = lazy(() => Struct({ resources: Vector(AllocatableResource) }));
  var HostRequestResourceAllocationResponse = lazy(() => Struct({ outcomes: Vector(AllocationOutcome) }));
  var HostScannerScanError = lazy(() => TaggedUnion({ CameraUnavailable: _void, Busy: _void, NotVisible: _void, InvalidRequest: Struct({ reason: str }), Unknown: Struct({ reason: str }) }));
  var HostScannerScanRequest = lazy(() => Struct({ formats: Vector(CodeFormat), prefix: Option(str), hint: Option(str) }));
  var HostScannerScanResponse = lazy(() => Struct({ outcome: ScanOutcome }));
  var HostSignPayloadError = lazy(() => TaggedUnion({ FailedToDecode: _void, Rejected: _void, PermissionDenied: _void, Unknown: Struct({ reason: str }) }));
  var HostSignPayloadRequest = lazy(() => Struct({ account: ProductAccountId, payload: HostSignPayloadData }));
  var HostSignPayloadResponse = lazy(() => Struct({ signature: Hex(), signedTransaction: Option(Hex()) }));
  var HostSignPayloadWithLegacyAccountRequest = lazy(() => Struct({ signer: str, payload: HostSignPayloadData }));
  var HostSignRawRequest = lazy(() => Struct({ account: ProductAccountId, payload: RawPayload }));
  var HostSignRawWithLegacyAccountRequest = lazy(() => Struct({ signer: str, payload: RawPayload }));
  var HostThemeSubscribeItem = lazy(() => Struct({ name: ThemeName, variant: ThemeVariant }));
  var HostWorkerBeginOperationRequest = lazy(() => Struct({ label: Option(str) }));
  var HostWorkerBeginOperationResponse = lazy(() => Struct({ id: u32 }));
  var HostWorkerEndOperationRequest = lazy(() => Struct({ id: u32 }));
  var ProductRendererRenderRequest = lazy(() => Struct({ context: RenderContext, payload: Hex() }));
  var RemoteChainHeadBodyRequest = lazy(() => Struct({ genesisHash: Hex(), followSubscriptionId: str, hash: Hex() }));
  var RemoteChainHeadBodyResponse = lazy(() => Struct({ operation: OperationStartedResult }));
  var RemoteChainHeadCallRequest = lazy(() => Struct({ genesisHash: Hex(), followSubscriptionId: str, hash: Hex(), function: str, callParameters: Hex() }));
  var RemoteChainHeadCallResponse = lazy(() => Struct({ operation: OperationStartedResult }));
  var RemoteChainHeadContinueRequest = lazy(() => Struct({ genesisHash: Hex(), followSubscriptionId: str, operationId: str }));
  var RemoteChainHeadFollowItem = lazy(() => TaggedUnion({ Initialized: Struct({ finalizedBlockHashes: Vector(Hex()), finalizedBlockRuntime: Option(RuntimeType) }), NewBlock: Struct({ blockHash: Hex(), parentBlockHash: Hex(), newRuntime: Option(RuntimeType) }), BestBlockChanged: Struct({ bestBlockHash: Hex() }), Finalized: Struct({ finalizedBlockHashes: Vector(Hex()), prunedBlockHashes: Vector(Hex()) }), OperationBodyDone: Struct({ operationId: str, value: Vector(Hex()) }), OperationCallDone: Struct({ operationId: str, output: Hex() }), OperationStorageItems: Struct({ operationId: str, items: Vector(StorageResultItem) }), OperationStorageDone: Struct({ operationId: str }), OperationWaitingForContinue: Struct({ operationId: str }), OperationInaccessible: Struct({ operationId: str }), OperationError: Struct({ operationId: str, error: str }), Stop: _void }));
  var RemoteChainHeadFollowRequest = lazy(() => Struct({ genesisHash: Hex(), withRuntime: bool2 }));
  var RemoteChainHeadHeaderRequest = lazy(() => Struct({ genesisHash: Hex(), followSubscriptionId: str, hash: Hex() }));
  var RemoteChainHeadHeaderResponse = lazy(() => Struct({ header: Option(Hex()) }));
  var RemoteChainHeadStopOperationRequest = lazy(() => Struct({ genesisHash: Hex(), followSubscriptionId: str, operationId: str }));
  var RemoteChainHeadStorageRequest = lazy(() => Struct({ genesisHash: Hex(), followSubscriptionId: str, hash: Hex(), items: Vector(StorageQueryItem), childTrie: Option(Hex()) }));
  var RemoteChainHeadStorageResponse = lazy(() => Struct({ operation: OperationStartedResult }));
  var RemoteChainHeadUnpinRequest = lazy(() => Struct({ genesisHash: Hex(), followSubscriptionId: str, hashes: Vector(Hex()) }));
  var RemoteChainInfoError = lazy(() => TaggedUnion({ NotSupported: _void, Unknown: GenericError }));
  var RemoteChainInfoRequest = lazy(() => Struct({ chain: ChainIdentifier }));
  var RemoteChainInfoResponse = lazy(() => Struct({ network: str, chain: ChainIdentifier, genesisHash: Hex(32) }));
  var RemoteChainSpecChainNameRequest = lazy(() => Struct({ genesisHash: Hex() }));
  var RemoteChainSpecChainNameResponse = lazy(() => Struct({ chainName: str }));
  var RemoteChainSpecGenesisHashRequest = lazy(() => Struct({ genesisHash: Hex() }));
  var RemoteChainSpecGenesisHashResponse = lazy(() => Struct({ genesisHash: Hex() }));
  var RemoteChainSpecPropertiesRequest = lazy(() => Struct({ genesisHash: Hex() }));
  var RemoteChainSpecPropertiesResponse = lazy(() => Struct({ properties: str }));
  var RemoteChainTransactionBroadcastRequest = lazy(() => Struct({ genesisHash: Hex(), transaction: Hex() }));
  var RemoteChainTransactionBroadcastResponse = lazy(() => Struct({ operationId: Option(str) }));
  var RemoteChainTransactionStopRequest = lazy(() => Struct({ genesisHash: Hex(), operationId: str }));
  var RemotePermissionRequest = lazy(() => Struct({ permission: RemotePermission }));
  var RemotePermissionResponse = lazy(() => Struct({ granted: bool2 }));
  var RemotePreimageLookupSubscribeItem = lazy(() => Struct({ value: Option(Hex()) }));
  var RemotePreimageLookupSubscribeRequest = lazy(() => Struct({ key: Hex() }));
  var RemoteStatementStoreCreateProofError = lazy(() => TaggedUnion({ UnableToSign: _void, UnknownAccount: _void, Unknown: Struct({ reason: str }) }));
  var RemoteStatementStoreCreateProofRequest = lazy(() => Struct({ productAccountId: ProductAccountId, statement: Statement }));
  var RemoteStatementStoreCreateProofResponse = lazy(() => Struct({ proof: StatementProof }));
  var RemoteStatementStoreSubscribeItem = lazy(() => Struct({ statements: Vector(SignedStatement), isComplete: bool2 }));
  var RemoteStatementStoreSubscribeRequest = lazy(() => TaggedUnion({ MatchAll: Vector(Hex(32)), MatchAny: Vector(Hex(32)) }));
  var HostLocalStorageReadError = lazy(() => TaggedUnion({ Full: _void, AccessNotGranted: _void, Unknown: Struct({ reason: str }) }));
  var HostLocalStorageReadRequest = lazy(() => Struct({ product: Option(str), key: str }));
  var VerticalAlignment = lazy(() => Status("Top", "Center", "Bottom"));
  var VrfSignature = lazy(() => Struct({ preOutput: Hex(32), proof: Hex(64) }));
  var VrfTranscriptItem = lazy(() => Struct({ label: Hex(), value: Hex() }));

  // ../packages/truapi/dist/generated/wire-table.js
  var wire_table_exports = {};
  __export(wire_table_exports, {
    ACCOUNT_CONNECTION_STATUS_SUBSCRIBE: () => ACCOUNT_CONNECTION_STATUS_SUBSCRIBE,
    ACCOUNT_CREATE_ACCOUNT_PROOF: () => ACCOUNT_CREATE_ACCOUNT_PROOF,
    ACCOUNT_GET_ACCOUNT: () => ACCOUNT_GET_ACCOUNT,
    ACCOUNT_GET_ACCOUNT_ALIAS: () => ACCOUNT_GET_ACCOUNT_ALIAS,
    ACCOUNT_GET_LEGACY_ACCOUNTS: () => ACCOUNT_GET_LEGACY_ACCOUNTS,
    ACCOUNT_GET_USER_ID: () => ACCOUNT_GET_USER_ID,
    ACCOUNT_LIST_RING_VRF_KEYS: () => ACCOUNT_LIST_RING_VRF_KEYS,
    ACCOUNT_REGISTER_RING_VRF_KEY: () => ACCOUNT_REGISTER_RING_VRF_KEY,
    ACCOUNT_REQUEST_LOGIN: () => ACCOUNT_REQUEST_LOGIN,
    ACCOUNT_RING_VRF_SIGN: () => ACCOUNT_RING_VRF_SIGN,
    ACCOUNT_SIGN_VRF: () => ACCOUNT_SIGN_VRF,
    CHAIN_BROADCAST_TRANSACTION: () => CHAIN_BROADCAST_TRANSACTION,
    CHAIN_CALL_HEAD: () => CHAIN_CALL_HEAD,
    CHAIN_CONTINUE_HEAD: () => CHAIN_CONTINUE_HEAD,
    CHAIN_FOLLOW_HEAD_SUBSCRIBE: () => CHAIN_FOLLOW_HEAD_SUBSCRIBE,
    CHAIN_GET_CHAIN_INFO: () => CHAIN_GET_CHAIN_INFO,
    CHAIN_GET_HEAD_BODY: () => CHAIN_GET_HEAD_BODY,
    CHAIN_GET_HEAD_HEADER: () => CHAIN_GET_HEAD_HEADER,
    CHAIN_GET_HEAD_STORAGE: () => CHAIN_GET_HEAD_STORAGE,
    CHAIN_GET_SPEC_CHAIN_NAME: () => CHAIN_GET_SPEC_CHAIN_NAME,
    CHAIN_GET_SPEC_GENESIS_HASH: () => CHAIN_GET_SPEC_GENESIS_HASH,
    CHAIN_GET_SPEC_PROPERTIES: () => CHAIN_GET_SPEC_PROPERTIES,
    CHAIN_STOP_HEAD_OPERATION: () => CHAIN_STOP_HEAD_OPERATION,
    CHAIN_STOP_TRANSACTION: () => CHAIN_STOP_TRANSACTION,
    CHAIN_UNPIN_HEAD: () => CHAIN_UNPIN_HEAD,
    CHAT_ACTION_SUBSCRIBE: () => CHAT_ACTION_SUBSCRIBE,
    CHAT_CREATE_ROOM: () => CHAT_CREATE_ROOM,
    CHAT_LIST_SUBSCRIBE: () => CHAT_LIST_SUBSCRIBE,
    CHAT_POST_MESSAGE: () => CHAT_POST_MESSAGE,
    CHAT_REGISTER_BOT: () => CHAT_REGISTER_BOT,
    COIN_PAYMENT_CREATE_CHEQUE: () => COIN_PAYMENT_CREATE_CHEQUE,
    COIN_PAYMENT_CREATE_PURSE: () => COIN_PAYMENT_CREATE_PURSE,
    COIN_PAYMENT_CREATE_RECEIVABLE: () => COIN_PAYMENT_CREATE_RECEIVABLE,
    COIN_PAYMENT_DELETE_PURSE: () => COIN_PAYMENT_DELETE_PURSE,
    COIN_PAYMENT_DEPOSIT: () => COIN_PAYMENT_DEPOSIT,
    COIN_PAYMENT_LISTEN_FOR_PAYMENT: () => COIN_PAYMENT_LISTEN_FOR_PAYMENT,
    COIN_PAYMENT_QUERY_PURSE: () => COIN_PAYMENT_QUERY_PURSE,
    COIN_PAYMENT_REBALANCE_PURSE: () => COIN_PAYMENT_REBALANCE_PURSE,
    COIN_PAYMENT_REFUND: () => COIN_PAYMENT_REFUND,
    CONTACTS_PICK: () => CONTACTS_PICK,
    ENTROPY_DERIVE: () => ENTROPY_DERIVE,
    GAME_CANCEL_NEXT_GAME: () => GAME_CANCEL_NEXT_GAME,
    GAME_REMIND_NEXT_GAME: () => GAME_REMIND_NEXT_GAME,
    LOCALE_SUBSCRIBE: () => LOCALE_SUBSCRIBE,
    LOCAL_STORAGE_CLEAR: () => LOCAL_STORAGE_CLEAR,
    LOCAL_STORAGE_READ: () => LOCAL_STORAGE_READ,
    LOCAL_STORAGE_SUBSCRIBE: () => LOCAL_STORAGE_SUBSCRIBE,
    LOCAL_STORAGE_WRITE: () => LOCAL_STORAGE_WRITE,
    NOTIFICATIONS_CANCEL_PUSH_NOTIFICATION: () => NOTIFICATIONS_CANCEL_PUSH_NOTIFICATION,
    NOTIFICATIONS_SEND_PUSH_NOTIFICATION: () => NOTIFICATIONS_SEND_PUSH_NOTIFICATION,
    PAYMENT_BALANCE_SUBSCRIBE: () => PAYMENT_BALANCE_SUBSCRIBE,
    PAYMENT_REQUEST: () => PAYMENT_REQUEST,
    PAYMENT_STATUS_SUBSCRIBE: () => PAYMENT_STATUS_SUBSCRIBE,
    PAYMENT_TOP_UP: () => PAYMENT_TOP_UP,
    PERMISSIONS_AUTHORIZE_DEVICE_PERMISSION: () => PERMISSIONS_AUTHORIZE_DEVICE_PERMISSION,
    PERMISSIONS_AUTHORIZE_REMOTE_PERMISSION: () => PERMISSIONS_AUTHORIZE_REMOTE_PERMISSION,
    PERMISSIONS_REQUEST_DEVICE_PERMISSION: () => PERMISSIONS_REQUEST_DEVICE_PERMISSION,
    PERMISSIONS_REQUEST_REMOTE_PERMISSION: () => PERMISSIONS_REQUEST_REMOTE_PERMISSION,
    POCKET_LIST_SUBSCRIBE: () => POCKET_LIST_SUBSCRIBE,
    POCKET_REMOVE_CARD: () => POCKET_REMOVE_CARD,
    PREIMAGE_LOOKUP_SUBSCRIBE: () => PREIMAGE_LOOKUP_SUBSCRIBE,
    PREIMAGE_SUBMIT: () => PREIMAGE_SUBMIT,
    RENDERER_ACTION_SUBSCRIBE: () => RENDERER_ACTION_SUBSCRIBE,
    RENDERER_RENDER: () => RENDERER_RENDER,
    RESOURCE_ALLOCATION_REQUEST: () => RESOURCE_ALLOCATION_REQUEST,
    SCANNER_SCAN: () => SCANNER_SCAN,
    SIGNING_CREATE_TRANSACTION: () => SIGNING_CREATE_TRANSACTION,
    SIGNING_CREATE_TRANSACTION_WITH_LEGACY_ACCOUNT: () => SIGNING_CREATE_TRANSACTION_WITH_LEGACY_ACCOUNT,
    SIGNING_SIGN_PAYLOAD: () => SIGNING_SIGN_PAYLOAD,
    SIGNING_SIGN_PAYLOAD_WITH_LEGACY_ACCOUNT: () => SIGNING_SIGN_PAYLOAD_WITH_LEGACY_ACCOUNT,
    SIGNING_SIGN_RAW: () => SIGNING_SIGN_RAW,
    SIGNING_SIGN_RAW_UNWATERMARKED_DEPRECATED: () => SIGNING_SIGN_RAW_UNWATERMARKED_DEPRECATED,
    SIGNING_SIGN_RAW_UNWATERMARKED_DEPRECATED_WITH_LEGACY_ACCOUNT: () => SIGNING_SIGN_RAW_UNWATERMARKED_DEPRECATED_WITH_LEGACY_ACCOUNT,
    SIGNING_SIGN_RAW_WITH_LEGACY_ACCOUNT: () => SIGNING_SIGN_RAW_WITH_LEGACY_ACCOUNT,
    STATEMENT_STORE_CREATE_PROOF: () => STATEMENT_STORE_CREATE_PROOF,
    STATEMENT_STORE_CREATE_PROOF_AUTHORIZED: () => STATEMENT_STORE_CREATE_PROOF_AUTHORIZED,
    STATEMENT_STORE_SUBMIT: () => STATEMENT_STORE_SUBMIT,
    STATEMENT_STORE_SUBSCRIBE: () => STATEMENT_STORE_SUBSCRIBE,
    SYSTEM_FEATURE_SUPPORTED: () => SYSTEM_FEATURE_SUPPORTED,
    SYSTEM_GET_PRODUCT_CONTEXT: () => SYSTEM_GET_PRODUCT_CONTEXT,
    SYSTEM_HANDSHAKE: () => SYSTEM_HANDSHAKE,
    SYSTEM_HOST_INFO: () => SYSTEM_HOST_INFO,
    SYSTEM_NAVIGATE_TO: () => SYSTEM_NAVIGATE_TO,
    THEME_SUBSCRIBE: () => THEME_SUBSCRIBE,
    WORKER_BEGIN_OPERATION: () => WORKER_BEGIN_OPERATION,
    WORKER_END_OPERATION: () => WORKER_END_OPERATION
  });
  var SYSTEM_HANDSHAKE = {
    trait: 1,
    method: 0,
    kind: "request"
  };
  var SYSTEM_FEATURE_SUPPORTED = {
    trait: 1,
    method: 1,
    kind: "request"
  };
  var SYSTEM_NAVIGATE_TO = {
    trait: 1,
    method: 2,
    kind: "request"
  };
  var SYSTEM_HOST_INFO = {
    trait: 1,
    method: 3,
    kind: "request"
  };
  var SYSTEM_GET_PRODUCT_CONTEXT = {
    trait: 1,
    method: 4,
    kind: "request"
  };
  var ACCOUNT_CONNECTION_STATUS_SUBSCRIBE = {
    trait: 2,
    method: 0,
    kind: "subscription"
  };
  var ACCOUNT_GET_ACCOUNT = {
    trait: 2,
    method: 1,
    kind: "request"
  };
  var ACCOUNT_GET_ACCOUNT_ALIAS = {
    trait: 2,
    method: 2,
    kind: "request"
  };
  var ACCOUNT_CREATE_ACCOUNT_PROOF = {
    trait: 2,
    method: 3,
    kind: "request"
  };
  var ACCOUNT_GET_LEGACY_ACCOUNTS = {
    trait: 2,
    method: 4,
    kind: "request"
  };
  var ACCOUNT_GET_USER_ID = {
    trait: 2,
    method: 5,
    kind: "request"
  };
  var ACCOUNT_REQUEST_LOGIN = {
    trait: 2,
    method: 6,
    kind: "request"
  };
  var ACCOUNT_SIGN_VRF = {
    trait: 2,
    method: 7,
    kind: "request"
  };
  var ACCOUNT_REGISTER_RING_VRF_KEY = {
    trait: 2,
    method: 8,
    kind: "request"
  };
  var ACCOUNT_LIST_RING_VRF_KEYS = {
    trait: 2,
    method: 9,
    kind: "request"
  };
  var ACCOUNT_RING_VRF_SIGN = {
    trait: 2,
    method: 10,
    kind: "request"
  };
  var CHAIN_FOLLOW_HEAD_SUBSCRIBE = {
    trait: 3,
    method: 0,
    kind: "subscription"
  };
  var CHAIN_GET_HEAD_HEADER = {
    trait: 3,
    method: 1,
    kind: "request"
  };
  var CHAIN_GET_HEAD_BODY = {
    trait: 3,
    method: 2,
    kind: "request"
  };
  var CHAIN_GET_HEAD_STORAGE = {
    trait: 3,
    method: 3,
    kind: "request"
  };
  var CHAIN_CALL_HEAD = {
    trait: 3,
    method: 4,
    kind: "request"
  };
  var CHAIN_UNPIN_HEAD = {
    trait: 3,
    method: 5,
    kind: "request"
  };
  var CHAIN_CONTINUE_HEAD = {
    trait: 3,
    method: 6,
    kind: "request"
  };
  var CHAIN_STOP_HEAD_OPERATION = {
    trait: 3,
    method: 7,
    kind: "request"
  };
  var CHAIN_GET_SPEC_GENESIS_HASH = {
    trait: 3,
    method: 8,
    kind: "request"
  };
  var CHAIN_GET_SPEC_CHAIN_NAME = {
    trait: 3,
    method: 9,
    kind: "request"
  };
  var CHAIN_GET_SPEC_PROPERTIES = {
    trait: 3,
    method: 10,
    kind: "request"
  };
  var CHAIN_BROADCAST_TRANSACTION = {
    trait: 3,
    method: 11,
    kind: "request"
  };
  var CHAIN_STOP_TRANSACTION = {
    trait: 3,
    method: 12,
    kind: "request"
  };
  var CHAIN_GET_CHAIN_INFO = {
    trait: 3,
    method: 13,
    kind: "request"
  };
  var CHAT_CREATE_ROOM = {
    trait: 4,
    method: 0,
    kind: "request"
  };
  var CHAT_REGISTER_BOT = {
    trait: 4,
    method: 1,
    kind: "request"
  };
  var CHAT_LIST_SUBSCRIBE = {
    trait: 4,
    method: 2,
    kind: "subscription"
  };
  var CHAT_POST_MESSAGE = {
    trait: 4,
    method: 3,
    kind: "request"
  };
  var CHAT_ACTION_SUBSCRIBE = {
    trait: 4,
    method: 4,
    kind: "subscription"
  };
  var COIN_PAYMENT_CREATE_PURSE = {
    trait: 5,
    method: 0,
    kind: "request"
  };
  var COIN_PAYMENT_QUERY_PURSE = {
    trait: 5,
    method: 1,
    kind: "request"
  };
  var COIN_PAYMENT_REBALANCE_PURSE = {
    trait: 5,
    method: 2,
    kind: "subscription"
  };
  var COIN_PAYMENT_DELETE_PURSE = {
    trait: 5,
    method: 3,
    kind: "subscription"
  };
  var COIN_PAYMENT_CREATE_RECEIVABLE = {
    trait: 5,
    method: 4,
    kind: "request"
  };
  var COIN_PAYMENT_CREATE_CHEQUE = {
    trait: 5,
    method: 5,
    kind: "request"
  };
  var COIN_PAYMENT_DEPOSIT = {
    trait: 5,
    method: 6,
    kind: "subscription"
  };
  var COIN_PAYMENT_REFUND = {
    trait: 5,
    method: 7,
    kind: "subscription"
  };
  var COIN_PAYMENT_LISTEN_FOR_PAYMENT = {
    trait: 5,
    method: 8,
    kind: "subscription"
  };
  var ENTROPY_DERIVE = {
    trait: 6,
    method: 0,
    kind: "request"
  };
  var LOCAL_STORAGE_READ = {
    trait: 7,
    method: 0,
    kind: "request"
  };
  var LOCAL_STORAGE_WRITE = {
    trait: 7,
    method: 1,
    kind: "request"
  };
  var LOCAL_STORAGE_CLEAR = {
    trait: 7,
    method: 2,
    kind: "request"
  };
  var LOCAL_STORAGE_SUBSCRIBE = {
    trait: 7,
    method: 3,
    kind: "subscription"
  };
  var NOTIFICATIONS_SEND_PUSH_NOTIFICATION = {
    trait: 8,
    method: 0,
    kind: "request"
  };
  var NOTIFICATIONS_CANCEL_PUSH_NOTIFICATION = {
    trait: 8,
    method: 1,
    kind: "request"
  };
  var PAYMENT_BALANCE_SUBSCRIBE = {
    trait: 9,
    method: 0,
    kind: "subscription"
  };
  var PAYMENT_TOP_UP = {
    trait: 9,
    method: 1,
    kind: "request"
  };
  var PAYMENT_REQUEST = {
    trait: 9,
    method: 2,
    kind: "request"
  };
  var PAYMENT_STATUS_SUBSCRIBE = {
    trait: 9,
    method: 3,
    kind: "subscription"
  };
  var PERMISSIONS_REQUEST_DEVICE_PERMISSION = {
    trait: 10,
    method: 0,
    kind: "request"
  };
  var PERMISSIONS_REQUEST_REMOTE_PERMISSION = {
    trait: 10,
    method: 1,
    kind: "request"
  };
  var PERMISSIONS_AUTHORIZE_REMOTE_PERMISSION = {
    trait: 10,
    method: 2,
    kind: "request"
  };
  var PERMISSIONS_AUTHORIZE_DEVICE_PERMISSION = {
    trait: 10,
    method: 3,
    kind: "request"
  };
  var PREIMAGE_LOOKUP_SUBSCRIBE = {
    trait: 11,
    method: 0,
    kind: "subscription"
  };
  var PREIMAGE_SUBMIT = {
    trait: 11,
    method: 1,
    kind: "request"
  };
  var RESOURCE_ALLOCATION_REQUEST = {
    trait: 12,
    method: 0,
    kind: "request"
  };
  var SIGNING_CREATE_TRANSACTION = {
    trait: 13,
    method: 0,
    kind: "request"
  };
  var SIGNING_CREATE_TRANSACTION_WITH_LEGACY_ACCOUNT = {
    trait: 13,
    method: 1,
    kind: "request"
  };
  var SIGNING_SIGN_RAW_WITH_LEGACY_ACCOUNT = {
    trait: 13,
    method: 2,
    kind: "request"
  };
  var SIGNING_SIGN_PAYLOAD_WITH_LEGACY_ACCOUNT = {
    trait: 13,
    method: 3,
    kind: "request"
  };
  var SIGNING_SIGN_RAW = {
    trait: 13,
    method: 4,
    kind: "request"
  };
  var SIGNING_SIGN_PAYLOAD = {
    trait: 13,
    method: 5,
    kind: "request"
  };
  var SIGNING_SIGN_RAW_UNWATERMARKED_DEPRECATED = {
    trait: 13,
    method: 6,
    kind: "request"
  };
  var SIGNING_SIGN_RAW_UNWATERMARKED_DEPRECATED_WITH_LEGACY_ACCOUNT = {
    trait: 13,
    method: 7,
    kind: "request"
  };
  var STATEMENT_STORE_SUBSCRIBE = {
    trait: 14,
    method: 0,
    kind: "subscription"
  };
  var STATEMENT_STORE_CREATE_PROOF = {
    trait: 14,
    method: 1,
    kind: "request"
  };
  var STATEMENT_STORE_SUBMIT = {
    trait: 14,
    method: 2,
    kind: "request"
  };
  var STATEMENT_STORE_CREATE_PROOF_AUTHORIZED = {
    trait: 14,
    method: 3,
    kind: "request"
  };
  var THEME_SUBSCRIBE = {
    trait: 15,
    method: 0,
    kind: "subscription"
  };
  var LOCALE_SUBSCRIBE = {
    trait: 16,
    method: 0,
    kind: "subscription"
  };
  var RENDERER_RENDER = {
    trait: 17,
    method: 0,
    kind: "subscription"
  };
  var RENDERER_ACTION_SUBSCRIBE = {
    trait: 17,
    method: 1,
    kind: "subscription"
  };
  var POCKET_LIST_SUBSCRIBE = {
    trait: 18,
    method: 0,
    kind: "subscription"
  };
  var POCKET_REMOVE_CARD = {
    trait: 18,
    method: 1,
    kind: "request"
  };
  var WORKER_BEGIN_OPERATION = {
    trait: 19,
    method: 0,
    kind: "request"
  };
  var WORKER_END_OPERATION = {
    trait: 19,
    method: 1,
    kind: "request"
  };
  var CONTACTS_PICK = {
    trait: 20,
    method: 0,
    kind: "request"
  };
  var GAME_REMIND_NEXT_GAME = {
    trait: 21,
    method: 0,
    kind: "request"
  };
  var GAME_CANCEL_NEXT_GAME = {
    trait: 21,
    method: 1,
    kind: "request"
  };
  var SCANNER_SCAN = {
    trait: 25,
    method: 0,
    kind: "request"
  };

  // ../packages/truapi/dist/generated/client.js
  var TRUAPI_CODEC_VERSION = 3;
  function toSubscriptionError(error) {
    if (error instanceof SubscriptionError)
      return error;
    const cause = error instanceof Error ? error : new Error(String(error));
    return new SubscriptionError(cause.message, { cause });
  }
  var HOST_INITIATED_DECLINE_PAYLOAD = new Uint8Array([
    1,
    4,
    44,
    117,
    110,
    97,
    118,
    97,
    105,
    108,
    97,
    98,
    108,
    101
  ]);
  var HOST_INITIATED_BUFFER_CAPACITY = 64;
  function interruptDecoder(reason) {
    const codec = Result2(_void, reason);
    return (payload) => {
      const decoded = codec.dec(payload);
      return decoded.success ? void 0 : decoded.value;
    };
  }
  function interruptEncoder(reason) {
    const codec = Result2(_void, reason);
    return (value) => codec.enc(value === void 0 ? { success: true, value: void 0 } : { success: false, value });
  }
  var OBSERVABLE_INTEROP = typeof Symbol === "function" && Symbol.observable || "@@observable";
  function createObservable({ transport, ids, payload, decodeItem, decodeInterrupt, onSubscribe }) {
    const observable = {
      subscribe(observer = {}) {
        let closed = false;
        let raw;
        let forwarding;
        const stopForwarding = () => {
          const active = forwarding;
          forwarding = void 0;
          active?.unsubscribe();
        };
        const fail = (error, stop = true) => {
          if (closed)
            return;
          closed = true;
          try {
            stopForwarding();
            if (stop)
              raw?.unsubscribe();
          } finally {
            observer.error?.(toSubscriptionError(error));
          }
        };
        raw = transport.subscribeRaw({
          ids,
          payload,
          onReceive: (payload2) => {
            if (closed)
              return;
            try {
              observer.next?.(decodeItem(payload2));
            } catch (error) {
              fail(error);
            }
          },
          onInterrupt: (payload2) => {
            if (closed)
              return;
            if (decodeInterrupt) {
              let reason;
              try {
                reason = decodeInterrupt(payload2);
              } catch (error) {
                fail(error, false);
                return;
              }
              if (reason === void 0) {
                closed = true;
                stopForwarding();
                observer.complete?.();
                return;
              }
              fail(new SubscriptionError("Subscription interrupted", { reason }), false);
              return;
            }
            closed = true;
            stopForwarding();
            observer.complete?.();
          },
          onClose: fail
        });
        if (!closed && onSubscribe) {
          try {
            forwarding = onSubscribe(raw);
          } catch (error) {
            raw.unsubscribe();
            throw error;
          }
          if (closed)
            stopForwarding();
        }
        return {
          get subscriptionId() {
            return raw?.subscriptionId ?? "";
          },
          unsubscribe: () => {
            if (closed)
              return;
            closed = true;
            stopForwarding();
            raw?.unsubscribe();
          }
        };
      },
      [OBSERVABLE_INTEROP]() {
        return observable;
      }
    };
    return observable;
  }
  var _transport;
  var AccountClient = class {
    constructor(transport) {
      __privateAdd(this, _transport);
      __privateSet(this, _transport, transport);
    }
    /** Subscribe to account connection status changes. */
    connectionStatusSubscribe() {
      return createObservable({
        transport: __privateGet(this, _transport),
        ids: ACCOUNT_CONNECTION_STATUS_SUBSCRIBE,
        payload: VersionedHostAccountConnectionStatusSubscribeRequest.enc({ tag: "V1", value: void 0 }),
        decodeItem: (payload) => VersionedHostAccountConnectionStatusSubscribeItem.dec(payload).value,
        decodeInterrupt: interruptDecoder(CallError(VersionedHostAccountConnectionStatusSubscribeError))
      });
    }
    /** Retrieve a product-scoped account. */
    getAccount(request, options) {
      return __privateGet(this, _transport).request({
        ids: ACCOUNT_GET_ACCOUNT,
        payload: VersionedHostAccountGetRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostAccountGetResponse, CallError(VersionedHostAccountGetError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /** Retrieve the contextual alias for a context and ring. */
    getAccountAlias(request, options) {
      return __privateGet(this, _transport).request({
        ids: ACCOUNT_GET_ACCOUNT_ALIAS,
        payload: VersionedHostAccountGetAliasRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostAccountGetAliasResponse, CallError(VersionedHostAccountGetAliasError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /** Generate a ring VRF proof with an explicitly registered member key. */
    createAccountProof(request, options) {
      return __privateGet(this, _transport).request({
        ids: ACCOUNT_CREATE_ACCOUNT_PROOF,
        payload: VersionedHostAccountCreateProofRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostAccountCreateProofResponse, CallError(VersionedHostAccountCreateProofError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /**
     * Produce an sr25519 (schnorrkel) VRF signature from a product account.
     *
     * The host builds a Merlin transcript from `transcriptLabel` and `items`
     * and signs it with the account's key, returning the VRF pre-output and
     * proof. Authorized like signing: local when `AutoSigning` covers the
     * account, otherwise a per-call user confirmation.
     */
    signVrf(request, options) {
      return __privateGet(this, _transport).request({
        ids: ACCOUNT_SIGN_VRF,
        payload: VersionedHostAccountSignVrfRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostAccountSignVrfResponse, CallError(VersionedHostAccountSignVrfError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /** Register a ring-VRF key owned by the calling product. */
    registerRingVrfKey(request, options) {
      return __privateGet(this, _transport).request({
        ids: ACCOUNT_REGISTER_RING_VRF_KEY,
        payload: VersionedHostAccountRegisterRingVrfKeyRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostAccountRegisterRingVrfKeyResponse, CallError(VersionedHostAccountRegisterRingVrfKeyError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /** List registered ring-VRF keys owned by a product. */
    listRingVrfKeys(request, options) {
      return __privateGet(this, _transport).request({
        ids: ACCOUNT_LIST_RING_VRF_KEYS,
        payload: VersionedHostAccountListRingVrfKeysRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostAccountListRingVrfKeysResponse, CallError(VersionedHostAccountListRingVrfKeysError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /** Sign bytes directly with a registered ring-VRF member key. */
    ringVrfSign(request, options) {
      return __privateGet(this, _transport).request({
        ids: ACCOUNT_RING_VRF_SIGN,
        payload: VersionedHostAccountRingVrfSignRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostAccountRingVrfSignResponse, CallError(VersionedHostAccountRingVrfSignError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /**
     * List non-product accounts the user owns.
     *
     * Current hosts do not expose non-product accounts, so the list is empty.
     */
    getLegacyAccounts(options) {
      return __privateGet(this, _transport).request({
        ids: ACCOUNT_GET_LEGACY_ACCOUNTS,
        payload: VersionedHostGetLegacyAccountsRequest.enc({ tag: "V1", value: void 0 }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostGetLegacyAccountsResponse, CallError(VersionedHostGetLegacyAccountsError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /** Fetch the user's primary identity. */
    getUserId(options) {
      return __privateGet(this, _transport).request({
        ids: ACCOUNT_GET_USER_ID,
        payload: VersionedHostGetUserIdRequest.enc({ tag: "V1", value: void 0 }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostGetUserIdResponse, CallError(VersionedHostGetUserIdError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /**
     * Request the host to present the login flow to the user.
     *
     * Products should call this in response to a user action (e.g. tapping a
     * "Sign in" button), not automatically on load.
     */
    requestLogin(request, options) {
      return __privateGet(this, _transport).request({
        ids: ACCOUNT_REQUEST_LOGIN,
        payload: VersionedHostRequestLoginRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostRequestLoginResponse, CallError(VersionedHostRequestLoginError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
  };
  _transport = new WeakMap();
  var _transport2;
  var ChainClient = class {
    constructor(transport) {
      __privateAdd(this, _transport2);
      __privateSet(this, _transport2, transport);
    }
    /** Follow the chain head and receive block events. */
    followHeadSubscribe({ request }) {
      return createObservable({
        transport: __privateGet(this, _transport2),
        ids: CHAIN_FOLLOW_HEAD_SUBSCRIBE,
        payload: VersionedRemoteChainHeadFollowRequest.enc({ tag: "V1", value: request }),
        decodeItem: (payload) => VersionedRemoteChainHeadFollowItem.dec(payload).value,
        decodeInterrupt: interruptDecoder(CallError(VersionedRemoteChainHeadFollowError))
      });
    }
    /** Fetch a block header. */
    getHeadHeader(request, options) {
      return __privateGet(this, _transport2).request({
        ids: CHAIN_GET_HEAD_HEADER,
        payload: VersionedRemoteChainHeadHeaderRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedRemoteChainHeadHeaderResponse, CallError(VersionedRemoteChainHeadHeaderError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /** Fetch a block body. */
    getHeadBody(request, options) {
      return __privateGet(this, _transport2).request({
        ids: CHAIN_GET_HEAD_BODY,
        payload: VersionedRemoteChainHeadBodyRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedRemoteChainHeadBodyResponse, CallError(VersionedRemoteChainHeadBodyError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /** Query runtime storage at a specific block. */
    getHeadStorage(request, options) {
      return __privateGet(this, _transport2).request({
        ids: CHAIN_GET_HEAD_STORAGE,
        payload: VersionedRemoteChainHeadStorageRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedRemoteChainHeadStorageResponse, CallError(VersionedRemoteChainHeadStorageError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /** Invoke a runtime call at a specific block. */
    callHead(request, options) {
      return __privateGet(this, _transport2).request({
        ids: CHAIN_CALL_HEAD,
        payload: VersionedRemoteChainHeadCallRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedRemoteChainHeadCallResponse, CallError(VersionedRemoteChainHeadCallError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /** Release pinned blocks. */
    unpinHead(request, options) {
      return __privateGet(this, _transport2).request({
        ids: CHAIN_UNPIN_HEAD,
        payload: VersionedRemoteChainHeadUnpinRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedRemoteChainHeadUnpinResponse, CallError(VersionedRemoteChainHeadUnpinError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /** Continue a paused chain-head operation. */
    continueHead(request, options) {
      return __privateGet(this, _transport2).request({
        ids: CHAIN_CONTINUE_HEAD,
        payload: VersionedRemoteChainHeadContinueRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedRemoteChainHeadContinueResponse, CallError(VersionedRemoteChainHeadContinueError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /** Stop a chain-head operation. */
    stopHeadOperation(request, options) {
      return __privateGet(this, _transport2).request({
        ids: CHAIN_STOP_HEAD_OPERATION,
        payload: VersionedRemoteChainHeadStopOperationRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedRemoteChainHeadStopOperationResponse, CallError(VersionedRemoteChainHeadStopOperationError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /** Fetch the canonical genesis hash for a chain. */
    getSpecGenesisHash(request, options) {
      return __privateGet(this, _transport2).request({
        ids: CHAIN_GET_SPEC_GENESIS_HASH,
        payload: VersionedRemoteChainSpecGenesisHashRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedRemoteChainSpecGenesisHashResponse, CallError(VersionedRemoteChainSpecGenesisHashError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /** Fetch the display name of a chain. */
    getSpecChainName(request, options) {
      return __privateGet(this, _transport2).request({
        ids: CHAIN_GET_SPEC_CHAIN_NAME,
        payload: VersionedRemoteChainSpecChainNameRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedRemoteChainSpecChainNameResponse, CallError(VersionedRemoteChainSpecChainNameError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /** Fetch the JSON-encoded properties of a chain. */
    getSpecProperties(request, options) {
      return __privateGet(this, _transport2).request({
        ids: CHAIN_GET_SPEC_PROPERTIES,
        payload: VersionedRemoteChainSpecPropertiesRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedRemoteChainSpecPropertiesResponse, CallError(VersionedRemoteChainSpecPropertiesError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /** Broadcast a signed transaction. */
    broadcastTransaction(request, options) {
      return __privateGet(this, _transport2).request({
        ids: CHAIN_BROADCAST_TRANSACTION,
        payload: VersionedRemoteChainTransactionBroadcastRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedRemoteChainTransactionBroadcastResponse, CallError(VersionedRemoteChainTransactionBroadcastError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /** Stop a transaction broadcast. */
    stopTransaction(request, options) {
      return __privateGet(this, _transport2).request({
        ids: CHAIN_STOP_TRANSACTION,
        payload: VersionedRemoteChainTransactionStopRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedRemoteChainTransactionStopResponse, CallError(VersionedRemoteChainTransactionStopError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /**
     * Resolve a chain identifier to its genesis hash against the host's
     * configured environment (RFC 0026).
     */
    getChainInfo(request, options) {
      return __privateGet(this, _transport2).request({
        ids: CHAIN_GET_CHAIN_INFO,
        payload: VersionedRemoteChainInfoRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedRemoteChainInfoResponse, CallError(VersionedRemoteChainInfoError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
  };
  _transport2 = new WeakMap();
  var _transport3;
  var ChatClient = class {
    constructor(transport) {
      __privateAdd(this, _transport3);
      __privateSet(this, _transport3, transport);
    }
    /** Create a chat room. */
    createRoom(request, options) {
      return __privateGet(this, _transport3).request({
        ids: CHAT_CREATE_ROOM,
        payload: VersionedHostChatCreateRoomRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostChatCreateRoomResponse, CallError(VersionedHostChatCreateRoomError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /** Register a chat bot. */
    registerBot(request, options) {
      return __privateGet(this, _transport3).request({
        ids: CHAT_REGISTER_BOT,
        payload: VersionedHostChatRegisterBotRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostChatRegisterBotResponse, CallError(VersionedHostChatRegisterBotError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /** Subscribe to the list of chat rooms. */
    listSubscribe() {
      return createObservable({
        transport: __privateGet(this, _transport3),
        ids: CHAT_LIST_SUBSCRIBE,
        payload: VersionedHostChatListSubscribeRequest.enc({ tag: "V1", value: void 0 }),
        decodeItem: (payload) => VersionedHostChatListSubscribeItem.dec(payload).value,
        decodeInterrupt: interruptDecoder(CallError(VersionedHostChatListSubscribeError))
      });
    }
    /**
     * Post a message to a chat room.
     *
     * The host bounds and screens what it forwards. Message text is capped at
     * 16 KiB and keeps line breaks and tabs, but is rejected for other
     * control characters and for bidirectional overrides. Identifiers and
     * display names are normalized and screened. A message carries at most 32
     * actions and 32 media items, a custom payload at most 256 KiB, and a URL
     * at most 2 KiB which must be `https` or an inline raster image. A
     * rejection reports `MessageTooLarge` when the body or custom payload is
     * over budget, and `Unknown` with a reason naming the field otherwise.
     *
     * The returned `messageId` is the correlation key for any action the
     * message carries: a later `actionSubscribe` trigger names it.
     */
    postMessage(request, options) {
      return __privateGet(this, _transport3).request({
        ids: CHAT_POST_MESSAGE,
        payload: VersionedHostChatPostMessageRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostChatPostMessageResponse, CallError(VersionedHostChatPostMessageError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /** Subscribe to received chat actions. */
    actionSubscribe() {
      return createObservable({
        transport: __privateGet(this, _transport3),
        ids: CHAT_ACTION_SUBSCRIBE,
        payload: VersionedHostChatActionSubscribeRequest.enc({ tag: "V1", value: void 0 }),
        decodeItem: (payload) => VersionedHostChatActionSubscribeItem.dec(payload).value,
        decodeInterrupt: interruptDecoder(CallError(VersionedHostChatActionSubscribeError))
      });
    }
  };
  _transport3 = new WeakMap();
  var _transport4;
  var CoinPaymentClient = class {
    constructor(transport) {
      __privateAdd(this, _transport4);
      __privateSet(this, _transport4, transport);
    }
    /** Create a new firewalled CoinPayment purse. */
    createPurse(request, options) {
      return __privateGet(this, _transport4).request({
        ids: COIN_PAYMENT_CREATE_PURSE,
        payload: VersionedHostCoinPaymentCreatePurseRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostCoinPaymentCreatePurseResponse, CallError(VersionedHostCoinPaymentCreatePurseError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /** Query product-visible purse metadata and balance. */
    queryPurse(request, options) {
      return __privateGet(this, _transport4).request({
        ids: COIN_PAYMENT_QUERY_PURSE,
        payload: VersionedHostCoinPaymentQueryPurseRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostCoinPaymentQueryPurseResponse, CallError(VersionedHostCoinPaymentQueryPurseError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /** Transfer balance between local purses. */
    rebalancePurse({ request }) {
      return createObservable({
        transport: __privateGet(this, _transport4),
        ids: COIN_PAYMENT_REBALANCE_PURSE,
        payload: VersionedHostCoinPaymentRebalancePurseRequest.enc({ tag: "V1", value: request }),
        decodeItem: (payload) => VersionedHostCoinPaymentRebalancePurseItem.dec(payload).value,
        decodeInterrupt: interruptDecoder(CallError(VersionedHostCoinPaymentRebalancePurseError))
      });
    }
    /** Delete a purse after draining its balance into another local purse. */
    deletePurse({ request }) {
      return createObservable({
        transport: __privateGet(this, _transport4),
        ids: COIN_PAYMENT_DELETE_PURSE,
        payload: VersionedHostCoinPaymentDeletePurseRequest.enc({ tag: "V1", value: request }),
        decodeItem: (payload) => VersionedHostCoinPaymentDeletePurseItem.dec(payload).value,
        decodeInterrupt: interruptDecoder(CallError(VersionedHostCoinPaymentDeletePurseError))
      });
    }
    /** Create a receivable public key for depositing into a purse. */
    createReceivable(request, options) {
      return __privateGet(this, _transport4).request({
        ids: COIN_PAYMENT_CREATE_RECEIVABLE,
        payload: VersionedHostCoinPaymentCreateReceivableRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostCoinPaymentCreateReceivableResponse, CallError(VersionedHostCoinPaymentCreateReceivableError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /** Create a cheque paying from a local purse to a receivable. */
    createCheque(request, options) {
      return __privateGet(this, _transport4).request({
        ids: COIN_PAYMENT_CREATE_CHEQUE,
        payload: VersionedHostCoinPaymentCreateChequeRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostCoinPaymentCreateChequeResponse, CallError(VersionedHostCoinPaymentCreateChequeError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /** Claim coins from a cheque into the receivable's purse. */
    deposit({ request }) {
      return createObservable({
        transport: __privateGet(this, _transport4),
        ids: COIN_PAYMENT_DEPOSIT,
        payload: VersionedHostCoinPaymentDepositRequest.enc({ tag: "V1", value: request }),
        decodeItem: (payload) => VersionedHostCoinPaymentDepositItem.dec(payload).value,
        decodeInterrupt: interruptDecoder(CallError(VersionedHostCoinPaymentDepositError))
      });
    }
    /** Attempt to return coins associated with a receivable. */
    refund({ request }) {
      return createObservable({
        transport: __privateGet(this, _transport4),
        ids: COIN_PAYMENT_REFUND,
        payload: VersionedHostCoinPaymentRefundRequest.enc({ tag: "V1", value: request }),
        decodeItem: (payload) => VersionedHostCoinPaymentRefundItem.dec(payload).value,
        decodeInterrupt: interruptDecoder(CallError(VersionedHostCoinPaymentRefundError))
      });
    }
    /** Listen for a cheque delivered through a standard transmission channel. */
    listenForPayment({ request }) {
      return createObservable({
        transport: __privateGet(this, _transport4),
        ids: COIN_PAYMENT_LISTEN_FOR_PAYMENT,
        payload: VersionedHostCoinPaymentListenForRequest.enc({ tag: "V1", value: request }),
        decodeItem: (payload) => VersionedHostCoinPaymentListenForItem.dec(payload).value,
        decodeInterrupt: interruptDecoder(CallError(VersionedHostCoinPaymentListenForError))
      });
    }
  };
  _transport4 = new WeakMap();
  var _transport5;
  var ContactsClient = class {
    constructor(transport) {
      __privateAdd(this, _transport5);
      __privateSet(this, _transport5, transport);
    }
    /**
     * Ask the host to let the user pick one contact.
     *
     * Resolves with the chosen contact's handle, or with why nothing was
     * chosen. A host that serves no picker answers `Unsupported`.
     *
     * The handle is not an address and cannot be turned into one. To pay the
     * person it names, put the handle where the recipient goes in the call and
     * list it in `contacts` on the transaction payload: the host replaces it
     * with their account before anything is signed or shown. A handle sent
     * anywhere else is 32 bytes that resolve to nobody.
     */
    pick(request, options) {
      return __privateGet(this, _transport5).request({
        ids: CONTACTS_PICK,
        payload: VersionedHostContactsPickRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostContactsPickResponse, CallError(VersionedHostContactsPickError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
  };
  _transport5 = new WeakMap();
  var _transport6;
  var EntropyClient = class {
    constructor(transport) {
      __privateAdd(this, _transport6);
      __privateSet(this, _transport6, transport);
    }
    /** Derive deterministic entropy. */
    derive(request, options) {
      return __privateGet(this, _transport6).request({
        ids: ENTROPY_DERIVE,
        payload: VersionedHostDeriveEntropyRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostDeriveEntropyResponse, CallError(VersionedHostDeriveEntropyError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
  };
  _transport6 = new WeakMap();
  var _transport7;
  var GameClient = class {
    constructor(transport) {
      __privateAdd(this, _transport7);
      __privateSet(this, _transport7, transport);
    }
    /**
     * Remind the user when this product's next game starts.
     *
     * Replaces the reminder this product already holds. Served only to the
     * game product: any other product, or a host that cannot hold reminders,
     * gets `Unsupported`. A `startsAt` that is not in the future fails with
     * `StartsInPast`, and a reminder the host cannot hold fails as a host
     * failure carrying its reason.
     */
    remindNextGame(request, options) {
      return __privateGet(this, _transport7).request({
        ids: GAME_REMIND_NEXT_GAME,
        payload: VersionedHostRemindNextGameRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostRemindNextGameResponse, CallError(VersionedHostRemindNextGameError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /** Drop the reminder. Safe to call whether one is held or not. */
    cancelNextGame(request, options) {
      return __privateGet(this, _transport7).request({
        ids: GAME_CANCEL_NEXT_GAME,
        payload: VersionedHostCancelNextGameRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostCancelNextGameResponse, CallError(VersionedHostCancelNextGameError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
  };
  _transport7 = new WeakMap();
  var _transport8;
  var LocalStorageClient = class {
    constructor(transport) {
      __privateAdd(this, _transport8);
      __privateSet(this, _transport8, transport);
    }
    /** Read a value by key. */
    read(request, options) {
      return __privateGet(this, _transport8).request({
        ids: LOCAL_STORAGE_READ,
        payload: VersionedHostLocalStorageReadRequest.enc({ tag: "V2", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostLocalStorageReadResponse, CallError(VersionedHostLocalStorageReadError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /** Write a value to a key. */
    write(request, options) {
      return __privateGet(this, _transport8).request({
        ids: LOCAL_STORAGE_WRITE,
        payload: VersionedHostLocalStorageWriteRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostLocalStorageWriteResponse, CallError(VersionedHostLocalStorageWriteError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /** Clear a value by key. */
    clear(request, options) {
      return __privateGet(this, _transport8).request({
        ids: LOCAL_STORAGE_CLEAR,
        payload: VersionedHostLocalStorageClearRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostLocalStorageClearResponse, CallError(VersionedHostLocalStorageClearError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /**
     * Subscribe to changes of one key in the product's own storage namespace.
     *
     * Emits the current value immediately, then one item per later write or
     * clear of the key by any of the product's runtimes. A write that leaves
     * the stored bytes unchanged emits nothing.
     */
    subscribe({ request }) {
      return createObservable({
        transport: __privateGet(this, _transport8),
        ids: LOCAL_STORAGE_SUBSCRIBE,
        payload: VersionedHostLocalStorageSubscribeRequest.enc({ tag: "V1", value: request }),
        decodeItem: (payload) => VersionedHostLocalStorageChangeItem.dec(payload).value,
        decodeInterrupt: interruptDecoder(CallError(VersionedHostLocalStorageSubscribeError))
      });
    }
  };
  _transport8 = new WeakMap();
  var _transport9;
  var LocaleClient = class {
    constructor(transport) {
      __privateAdd(this, _transport9);
      __privateSet(this, _transport9, transport);
    }
    /** Subscribe to the host's selected locale. */
    subscribe() {
      return createObservable({
        transport: __privateGet(this, _transport9),
        ids: LOCALE_SUBSCRIBE,
        payload: VersionedHostLocaleSubscribeRequest.enc({ tag: "V1", value: void 0 }),
        decodeItem: (payload) => VersionedHostLocaleSubscribeItem.dec(payload).value,
        decodeInterrupt: interruptDecoder(CallError(VersionedHostLocaleSubscribeError))
      });
    }
  };
  _transport9 = new WeakMap();
  var _transport10;
  var NotificationsClient = class {
    constructor(transport) {
      __privateAdd(this, _transport10);
      __privateSet(this, _transport10, transport);
    }
    /**
     * Send a push notification to the user.
     *
     * Returns a notification id that can be
     * passed to [`cancel_push_notification`](Self::cancel_push_notification)
     * to retract a scheduled notification. When `scheduled_at` is set the host
     * persists the notification across restarts and fires it through the
     * platform-native scheduler. See [RFC 0019].
     *
     * [RFC 0019]: https://github.com/paritytech/trinity-user-agents/blob/main/docs/rfcs/0019-scheduled-notifications.md
     */
    sendPushNotification(request, options) {
      return __privateGet(this, _transport10).request({
        ids: NOTIFICATIONS_SEND_PUSH_NOTIFICATION,
        payload: VersionedHostPushNotificationRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostPushNotificationResponse, CallError(VersionedHostPushNotificationError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /**
     * Cancels a previously issued push notification.
     *
     * Cancellation is idempotent: returns `Ok(())` whether the notification is
     * still pending, already fired, or was never issued. See [RFC 0019].
     *
     * [RFC 0019]: https://github.com/paritytech/trinity-user-agents/blob/main/docs/rfcs/0019-scheduled-notifications.md
     */
    cancelPushNotification(request, options) {
      return __privateGet(this, _transport10).request({
        ids: NOTIFICATIONS_CANCEL_PUSH_NOTIFICATION,
        payload: VersionedHostPushNotificationCancelRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostPushNotificationCancelResponse, CallError(VersionedHostPushNotificationCancelError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
  };
  _transport10 = new WeakMap();
  var _transport11;
  var PaymentClient = class {
    constructor(transport) {
      __privateAdd(this, _transport11);
      __privateSet(this, _transport11, transport);
    }
    /** Subscribe to payment balance updates. */
    balanceSubscribe({ request }) {
      return createObservable({
        transport: __privateGet(this, _transport11),
        ids: PAYMENT_BALANCE_SUBSCRIBE,
        payload: VersionedHostPaymentBalanceSubscribeRequest.enc({ tag: "V1", value: request }),
        decodeItem: (payload) => VersionedHostPaymentBalanceSubscribeItem.dec(payload).value,
        decodeInterrupt: interruptDecoder(CallError(VersionedHostPaymentBalanceSubscribeError))
      });
    }
    /** Request a payment from the user. */
    request(request, options) {
      return __privateGet(this, _transport11).request({
        ids: PAYMENT_REQUEST,
        payload: VersionedHostPaymentRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostPaymentResponse, CallError(VersionedHostPaymentError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /** Subscribe to payment lifecycle updates for a specific payment. */
    statusSubscribe({ request }) {
      return createObservable({
        transport: __privateGet(this, _transport11),
        ids: PAYMENT_STATUS_SUBSCRIBE,
        payload: VersionedHostPaymentStatusSubscribeRequest.enc({ tag: "V1", value: request }),
        decodeItem: (payload) => VersionedHostPaymentStatusSubscribeItem.dec(payload).value,
        decodeInterrupt: interruptDecoder(CallError(VersionedHostPaymentStatusSubscribeError))
      });
    }
    /** Top up the user's payment balance. */
    topUp(request, options) {
      return __privateGet(this, _transport11).request({
        ids: PAYMENT_TOP_UP,
        payload: VersionedHostPaymentTopUpRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostPaymentTopUpResponse, CallError(VersionedHostPaymentTopUpError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
  };
  _transport11 = new WeakMap();
  var _transport12;
  var PermissionsClient = class {
    constructor(transport) {
      __privateAdd(this, _transport12);
      __privateSet(this, _transport12, transport);
    }
    /** Request a device-capability permission from the user. */
    requestDevicePermission(request, options) {
      return __privateGet(this, _transport12).request({
        ids: PERMISSIONS_REQUEST_DEVICE_PERMISSION,
        payload: VersionedHostDevicePermissionRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostDevicePermissionResponse, CallError(VersionedHostDevicePermissionError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /**
     * Request a remote-operation permission.
     *
     * This example makes live requests to Frankfurter after permission is granted.
     */
    requestRemotePermission(request, options) {
      return __privateGet(this, _transport12).request({
        ids: PERMISSIONS_REQUEST_REMOTE_PERMISSION,
        payload: VersionedRemotePermissionRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedRemotePermissionResponse, CallError(VersionedRemotePermissionError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
  };
  _transport12 = new WeakMap();
  var _transport13;
  var PocketClient = class {
    constructor(transport) {
      __privateAdd(this, _transport13);
      __privateSet(this, _transport13, transport);
    }
    /**
     * Subscribe to the calling product's cards.
     *
     * Emits the whole set on subscribe and again after every change.
     */
    listSubscribe() {
      return createObservable({
        transport: __privateGet(this, _transport13),
        ids: POCKET_LIST_SUBSCRIBE,
        payload: VersionedHostPocketListSubscribeRequest.enc({ tag: "V1", value: void 0 }),
        decodeItem: (payload) => VersionedHostPocketListSubscribeItem.dec(payload).value,
        decodeInterrupt: interruptDecoder(CallError(VersionedHostPocketListSubscribeError))
      });
    }
    /**
     * Remove one of the calling product's cards.
     *
     * Removing a card that is not present succeeds. A privileged card is
     * refused with `Privileged`.
     */
    removeCard(request, options) {
      return __privateGet(this, _transport13).request({
        ids: POCKET_REMOVE_CARD,
        payload: VersionedHostPocketRemoveCardRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostPocketRemoveCardResponse, CallError(VersionedHostPocketRemoveCardError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
  };
  _transport13 = new WeakMap();
  var _transport14;
  var PreimageClient = class {
    constructor(transport) {
      __privateAdd(this, _transport14);
      __privateSet(this, _transport14, transport);
    }
    /** Subscribe to preimage lookups for a given key. */
    lookupSubscribe({ request }) {
      return createObservable({
        transport: __privateGet(this, _transport14),
        ids: PREIMAGE_LOOKUP_SUBSCRIBE,
        payload: VersionedRemotePreimageLookupSubscribeRequest.enc({ tag: "V1", value: request }),
        decodeItem: (payload) => VersionedRemotePreimageLookupSubscribeItem.dec(payload).value,
        decodeInterrupt: interruptDecoder(CallError(VersionedRemotePreimageLookupSubscribeError))
      });
    }
    /** Submit a preimage. Returns the preimage key (hash) on success. */
    submit(request, options) {
      return __privateGet(this, _transport14).request({
        ids: PREIMAGE_SUBMIT,
        payload: VersionedRemotePreimageSubmitRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedRemotePreimageSubmitResponse, CallError(VersionedRemotePreimageSubmitError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
  };
  _transport14 = new WeakMap();
  var _transport15, _renderRegistration;
  var RendererClient = class {
    constructor(transport) {
      __privateAdd(this, _transport15);
      __privateAdd(this, _renderRegistration);
      __privateSet(this, _transport15, transport);
      __privateSet(this, _renderRegistration, transport.registerHostInitiatedSubscription({
        ids: RENDERER_RENDER,
        decodeRequest: (payload) => VersionedProductRendererRenderRequest.dec(payload).value,
        encodeItem: (item) => VersionedProductRendererRenderItem.enc({ tag: "V1", value: item }),
        encodeInterrupt: interruptEncoder(CallError(VersionedProductRendererRenderError)),
        declinePayload: HOST_INITIATED_DECLINE_PAYLOAD,
        bufferCapacity: HOST_INITIATED_BUFFER_CAPACITY
      }));
    }
    /**
     * Streams renderer trees for one product-rendered body. Each item
     * replaces the previous tree. The stream stays open while the body is
     * displayed so the product can redraw in place.
     */
    onRender(handler) {
      return __privateGet(this, _renderRegistration).setHandler(handler);
    }
    /** Subscribe to actions triggered inside this product's rendered bodies. */
    actionSubscribe() {
      return createObservable({
        transport: __privateGet(this, _transport15),
        ids: RENDERER_ACTION_SUBSCRIBE,
        payload: VersionedHostRendererActionSubscribeRequest.enc({ tag: "V1", value: void 0 }),
        decodeItem: (payload) => VersionedHostRendererActionSubscribeItem.dec(payload).value,
        decodeInterrupt: interruptDecoder(CallError(VersionedHostRendererActionSubscribeError))
      });
    }
  };
  _transport15 = new WeakMap();
  _renderRegistration = new WeakMap();
  var _transport16;
  var ResourceAllocationClient = class {
    constructor(transport) {
      __privateAdd(this, _transport16);
      __privateSet(this, _transport16, transport);
    }
    /** Request the host to pre-allocate one or more resources. */
    request(request, options) {
      return __privateGet(this, _transport16).request({
        ids: RESOURCE_ALLOCATION_REQUEST,
        payload: VersionedHostRequestResourceAllocationRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostRequestResourceAllocationResponse, CallError(VersionedHostRequestResourceAllocationError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
  };
  _transport16 = new WeakMap();
  var _transport17;
  var ScannerClient = class {
    constructor(transport) {
      __privateAdd(this, _transport17);
      __privateSet(this, _transport17, transport);
    }
    /**
     * Ask the host to let the user scan one code.
     *
     * The host ignores codes outside `formats` or without `prefix` and keeps
     * the viewfinder open. A host with no scanner answers `Unsupported`, and
     * cancelling the call closes the viewfinder.
     */
    scan(request, options) {
      return __privateGet(this, _transport17).request({
        ids: SCANNER_SCAN,
        payload: VersionedHostScannerScanRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostScannerScanResponse, CallError(VersionedHostScannerScanError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
  };
  _transport17 = new WeakMap();
  var _transport18;
  var SigningClient = class {
    constructor(transport) {
      __privateAdd(this, _transport18);
      __privateSet(this, _transport18, transport);
    }
    /**
     * Construct a transaction for a product account.
     *
     * Served locally without a user confirmation when an RFC-0010 `AutoSigning`
     * grant covers the account; otherwise each call is confirmed by the user.
     *
     * Under Extrinsic V5, omitting `VerifyMultiSignature` from `extensions`
     * lets the host sign with the signer's key. Listing it — as `Disabled`,
     * with a proof in a later extension — encodes the given bytes verbatim and
     * returns an unsigned transaction.
     *
     * `txExtVersion` is the version of the transaction extensions in
     * `extensions`, as the runtime numbers them. The host picks the extrinsic
     * format from it. V4 always uses version 0, so a non-zero version builds a
     * V5 general transaction. Version 0 builds V5 when it includes
     * `VerifyMultiSignature`, and a signed V4 transaction otherwise.
     *
     * `contacts` lists the contact handles `callData` names, and the host
     * replaces each with the account it resolves to before the call is shown
     * or signed. A declared handle the call does not contain, or one no
     * contact matches, refuses the whole call as `UnknownContact` rather than
     * signing something that names somebody else. A call paying nobody from
     * the picker leaves it empty.
     */
    createTransaction(request, options) {
      return __privateGet(this, _transport18).request({
        ids: SIGNING_CREATE_TRANSACTION,
        payload: VersionedHostCreateTransactionRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostCreateTransactionResponse, CallError(VersionedHostCreateTransactionError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /**
     * Construct a transaction for a non-product (legacy) account.
     *
     * The V5 `VerifyMultiSignature` rule is the same as
     * [`Signing::create_transaction`]: omit it and the host signs, list it and
     * the given bytes are used with no host signature.
     */
    createTransactionWithLegacyAccount(request, options) {
      return __privateGet(this, _transport18).request({
        ids: SIGNING_CREATE_TRANSACTION_WITH_LEGACY_ACCOUNT,
        payload: VersionedHostCreateTransactionWithLegacyAccountRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostCreateTransactionWithLegacyAccountResponse, CallError(VersionedHostCreateTransactionWithLegacyAccountError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /** Sign raw bytes with a non-product account. */
    signRawWithLegacyAccount(request, options) {
      return __privateGet(this, _transport18).request({
        ids: SIGNING_SIGN_RAW_WITH_LEGACY_ACCOUNT,
        payload: VersionedHostSignRawWithLegacyAccountRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostSignRawWithLegacyAccountResponse, CallError(VersionedHostSignRawWithLegacyAccountError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /** Sign an extrinsic payload with a non-product account. */
    signPayloadWithLegacyAccount(request, options) {
      return __privateGet(this, _transport18).request({
        ids: SIGNING_SIGN_PAYLOAD_WITH_LEGACY_ACCOUNT,
        payload: VersionedHostSignPayloadWithLegacyAccountRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostSignPayloadWithLegacyAccountResponse, CallError(VersionedHostSignPayloadWithLegacyAccountError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /**
     * Sign raw bytes or a message.
     *
     * Served locally without a user confirmation when an RFC-0010 `AutoSigning`
     * grant covers the account; otherwise each call is confirmed by the user.
     */
    signRaw(request, options) {
      return __privateGet(this, _transport18).request({
        ids: SIGNING_SIGN_RAW,
        payload: VersionedHostSignRawRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostSignRawResponse, CallError(VersionedHostSignRawError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /**
     * Sign an extrinsic payload.
     *
     * Served locally without a user confirmation when an RFC-0010 `AutoSigning`
     * grant covers the account; otherwise each call is confirmed by the user.
     */
    signPayload(request, options) {
      return __privateGet(this, _transport18).request({
        ids: SIGNING_SIGN_PAYLOAD,
        payload: VersionedHostSignPayloadRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostSignPayloadResponse, CallError(VersionedHostSignPayloadError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /**
     * Sign the supplied data without adding or removing a watermark.
     *
     * Temporary compatibility API for runtime ownership proofs, including the
     * 32-byte Resources alias used by Humanity. Payload decoding matches
     * watermarked signing, but the decoded bytes are signed exactly as supplied.
     * This permits transaction-shaped data and requires signing authorization
     * and explicit user confirmation.
     *
     * @deprecated Temporary unwatermarked signing; migrate to watermarked signing when the runtime supports it. This API will be removed. See <https://github.com/paritytech/trinity-user-agents/issues/612>
     */
    signRawUnwatermarkedDeprecated(request, options) {
      return __privateGet(this, _transport18).request({
        ids: SIGNING_SIGN_RAW_UNWATERMARKED_DEPRECATED,
        payload: VersionedHostSignRawRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostSignRawResponse, CallError(VersionedHostSignRawError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /**
     * Sign the supplied data without adding or removing a watermark.
     *
     * Temporary compatibility API for runtime ownership proofs, including the
     * 32-byte Resources alias used by Humanity. Payload decoding matches
     * watermarked signing, but the decoded bytes are signed exactly as supplied.
     * This permits transaction-shaped data and requires signing authorization
     * and explicit user confirmation.
     *
     * @deprecated Temporary unwatermarked signing; migrate to watermarked signing when the runtime supports it. This API will be removed. See <https://github.com/paritytech/trinity-user-agents/issues/612>
     */
    signRawUnwatermarkedDeprecatedWithLegacyAccount(request, options) {
      return __privateGet(this, _transport18).request({
        ids: SIGNING_SIGN_RAW_UNWATERMARKED_DEPRECATED_WITH_LEGACY_ACCOUNT,
        payload: VersionedHostSignRawWithLegacyAccountRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostSignRawWithLegacyAccountResponse, CallError(VersionedHostSignRawWithLegacyAccountError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
  };
  _transport18 = new WeakMap();
  var _transport19;
  var StatementStoreClient = class {
    constructor(transport) {
      __privateAdd(this, _transport19);
      __privateSet(this, _transport19, transport);
    }
    /** Subscribe to statements matching a topic filter. */
    subscribe({ request }) {
      return createObservable({
        transport: __privateGet(this, _transport19),
        ids: STATEMENT_STORE_SUBSCRIBE,
        payload: VersionedRemoteStatementStoreSubscribeRequest.enc({ tag: "V1", value: request }),
        decodeItem: (payload) => VersionedRemoteStatementStoreSubscribeItem.dec(payload).value,
        decodeInterrupt: interruptDecoder(CallError(VersionedRemoteStatementStoreSubscribeError))
      });
    }
    /**
     * Create a proof for a statement.
     *
     * **Deprecated:** use [`create_proof_authorized`](Self::create_proof_authorized)
     * instead, which uses a pre-allocated allowance account and does not
     * require a per-call signing prompt. Pairing hosts may reject this method
     * when their signing channel cannot sign statement proof payloads exactly.
     */
    createProof(request, options) {
      return __privateGet(this, _transport19).request({
        ids: STATEMENT_STORE_CREATE_PROOF,
        payload: VersionedRemoteStatementStoreCreateProofRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedRemoteStatementStoreCreateProofResponse, CallError(VersionedRemoteStatementStoreCreateProofError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /**
     * Create a proof for a statement using a pre-allocated allowance account,
     * bypassing the per-call signing prompt.
     */
    createProofAuthorized(request, options) {
      return __privateGet(this, _transport19).request({
        ids: STATEMENT_STORE_CREATE_PROOF_AUTHORIZED,
        payload: VersionedRemoteStatementStoreCreateProofAuthorizedRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedRemoteStatementStoreCreateProofAuthorizedResponse, CallError(VersionedRemoteStatementStoreCreateProofAuthorizedError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /**
     * Submit a signed statement to the network. The request body is the
     * [`SignedStatement`](crate::v01::SignedStatement) directly (no wrapping
     * struct), matching upstream `triangle-js-sdks`.
     */
    submit(request, options) {
      return __privateGet(this, _transport19).request({
        ids: STATEMENT_STORE_SUBMIT,
        payload: VersionedRemoteStatementStoreSubmitRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedRemoteStatementStoreSubmitResponse, CallError(VersionedRemoteStatementStoreSubmitError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
  };
  _transport19 = new WeakMap();
  var _transport20;
  var SystemClient = class {
    constructor(transport) {
      __privateAdd(this, _transport20);
      __privateSet(this, _transport20, transport);
    }
    /** Negotiate the wire codec version with the product. */
    handshake(options) {
      return __privateGet(this, _transport20).request({
        ids: SYSTEM_HANDSHAKE,
        payload: VersionedHostHandshakeRequest.enc({ tag: "V1", value: { codecVersion: TRUAPI_CODEC_VERSION } }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostHandshakeResponse, CallError(VersionedHostHandshakeError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /** Query whether the host supports a specific feature. */
    featureSupported(request, options) {
      return __privateGet(this, _transport20).request({
        ids: SYSTEM_FEATURE_SUPPORTED,
        payload: VersionedHostFeatureSupportedRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostFeatureSupportedResponse, CallError(VersionedHostFeatureSupportedError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /**
     * Request the host to open a URL.
     *
     * An `http` or `https` URL outside the ecosystem needs a
     * `RemotePermission::Remote` grant for the target host, and prompts for one
     * on first use. dotNS names, `localhost`, and the app-handoff schemes
     * (`mailto:`, `tel:`, `polkadot:`, `dot:`) consume no grant. The grant is
     * per host and shared with outbound data access to that host, so approving
     * one covers the other.
     */
    navigateTo(request, options) {
      return __privateGet(this, _transport20).request({
        ids: SYSTEM_NAVIGATE_TO,
        payload: VersionedHostNavigateToRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostNavigateToResponse, CallError(VersionedHostNavigateToError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /**
     * Report the host's identity and version.
     *
     * Returns the host's platform, name, and version so a product knows
     * exactly which host — and which build of it — is running it: for
     * adapting to the host, telemetry, and attributing behaviour to a
     * concrete build in diagnostics and bug reports.
     */
    info(options) {
      return __privateGet(this, _transport20).request({
        ids: SYSTEM_HOST_INFO,
        payload: VersionedHostInfoRequest.enc({ tag: "V1", value: void 0 }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostInfoResponse, CallError(VersionedHostInfoError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /** Return the product context bound to the current host runtime. */
    getProductContext(options) {
      return __privateGet(this, _transport20).request({
        ids: SYSTEM_GET_PRODUCT_CONTEXT,
        payload: VersionedHostGetProductContextRequest.enc({ tag: "V1", value: void 0 }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostGetProductContextResponse, CallError(VersionedHostGetProductContextError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
  };
  _transport20 = new WeakMap();
  var _transport21;
  var ThemeClient = class {
    constructor(transport) {
      __privateAdd(this, _transport21);
      __privateSet(this, _transport21, transport);
    }
    /** Subscribe to host theme changes. */
    subscribe() {
      return createObservable({
        transport: __privateGet(this, _transport21),
        ids: THEME_SUBSCRIBE,
        payload: VersionedHostThemeSubscribeRequest.enc({ tag: "V1", value: void 0 }),
        decodeItem: (payload) => VersionedHostThemeSubscribeItem.dec(payload).value,
        decodeInterrupt: interruptDecoder(CallError(VersionedHostThemeSubscribeError))
      });
    }
  };
  _transport21 = new WeakMap();
  var _transport22;
  var WorkerClient = class {
    constructor(transport) {
      __privateAdd(this, _transport22);
      __privateSet(this, _transport22, transport);
    }
    /** Begin a pending operation. */
    beginOperation(request, options) {
      return __privateGet(this, _transport22).request({
        ids: WORKER_BEGIN_OPERATION,
        payload: VersionedHostWorkerBeginOperationRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostWorkerBeginOperationResponse, CallError(VersionedHostWorkerBeginOperationError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /**
     * End a pending operation. Idempotent: an unknown or already-ended id
     * succeeds, so a retry after an ambiguous failure is safe.
     */
    endOperation(request, options) {
      return __privateGet(this, _transport22).request({
        ids: WORKER_END_OPERATION,
        payload: VersionedHostWorkerEndOperationRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostWorkerEndOperationResponse, CallError(VersionedHostWorkerEndOperationError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
  };
  _transport22 = new WeakMap();
  function createClient(transport) {
    return {
      account: new AccountClient(transport),
      chain: new ChainClient(transport),
      chat: new ChatClient(transport),
      coinPayment: new CoinPaymentClient(transport),
      contacts: new ContactsClient(transport),
      entropy: new EntropyClient(transport),
      game: new GameClient(transport),
      localStorage: new LocalStorageClient(transport),
      locale: new LocaleClient(transport),
      notifications: new NotificationsClient(transport),
      payment: new PaymentClient(transport),
      permissions: new PermissionsClient(transport),
      pocket: new PocketClient(transport),
      preimage: new PreimageClient(transport),
      renderer: new RendererClient(transport),
      resourceAllocation: new ResourceAllocationClient(transport),
      scanner: new ScannerClient(transport),
      signing: new SigningClient(transport),
      statementStore: new StatementStoreClient(transport),
      system: new SystemClient(transport),
      theme: new ThemeClient(transport),
      worker: new WorkerClient(transport)
    };
  }

  // ../packages/truapi/dist/client.js
  var KNOWN_WIRE_IDS = new Set(Object.values(wire_table_exports).map((ids) => `${ids.trait}:${ids.method}`));
  var DEFAULT_REQUEST_TIMEOUT_MS = 12e4;
  var RequestTimeoutError = class extends Error {
    constructor(requestId, traitId, methodId, timeoutMs) {
      super(`TrUAPI request ${requestId} (wire ${traitId}, ${methodId}) timed out after ${timeoutMs}ms`);
      /** Transport-assigned request identifier. */
      __publicField(this, "requestId");
      /** Trait discriminant of the unanswered request. */
      __publicField(this, "traitId");
      /** Method discriminant of the unanswered request. */
      __publicField(this, "methodId");
      /** Deadline that elapsed, in milliseconds. */
      __publicField(this, "timeoutMs");
      this.name = "RequestTimeoutError";
      this.requestId = requestId;
      this.traitId = traitId;
      this.methodId = methodId;
      this.timeoutMs = timeoutMs;
    }
  };
  var STOP_FRAME = new Uint8Array();
  var CANCEL_FRAME = new Uint8Array();
  function reportProtocolViolation(detail) {
    console.warn(`[truapi] ${detail}`);
  }
  var HANDSHAKE_TIMEOUT_MS = 1e4;
  var HANDSHAKE_RESPONSE_CODEC = Result2(VersionedHostHandshakeResponse, CallError(VersionedHostHandshakeError));
  function encodeSuccessfulHandshakeResponse() {
    return HANDSHAKE_RESPONSE_CODEC.enc({
      success: true,
      value: { tag: "V1" }
    });
  }
  function encodeUnsupportedHandshakeResponse() {
    return HANDSHAKE_RESPONSE_CODEC.enc({
      success: false,
      value: {
        tag: "Domain",
        value: {
          tag: "V1",
          value: { tag: "UnsupportedProtocolVersion", value: void 0 }
        }
      }
    });
  }
  function pairKey(traitId, methodId) {
    return `${traitId}:${methodId}`;
  }
  function decodeUnsupportedMessage(payload) {
    if (payload.length === 0) {
      throw new Error("Malformed protocol error payload: empty");
    }
    if (payload[0] !== 0 || payload.length > 1 && payload[1] !== 0) {
      return void 0;
    }
    if (payload.length !== 4) {
      throw new Error(`Malformed protocol error payload: expected 4 bytes, received ${payload.length}`);
    }
    return { traitId: payload[2], methodId: payload[3] };
  }
  function createTransport(provider, options = {}) {
    const requestTimeoutMs = options.requestTimeoutMs ?? DEFAULT_REQUEST_TIMEOUT_MS;
    if (!Number.isFinite(requestTimeoutMs) || requestTimeoutMs <= 0) {
      throw new RangeError("requestTimeoutMs must be a positive finite number");
    }
    let idCounter = 0;
    const requestIdPrefix = options.requestIdPrefix ?? "p:";
    let closedError = null;
    const pending = /* @__PURE__ */ new Map();
    const subscriptions = /* @__PURE__ */ new Map();
    const hostRoutes = /* @__PURE__ */ new Map();
    function toError2(error) {
      return error instanceof Error ? error : new Error(String(error));
    }
    function takePending(requestId) {
      const entry = pending.get(requestId);
      if (!entry)
        return void 0;
      pending.delete(requestId);
      entry.cancelTimeout();
      entry.detachAbort();
      return entry;
    }
    function sendCancel(requestId, ids) {
      if (closedError)
        return;
      try {
        send({
          requestId,
          payload: {
            traitId: ids.trait,
            methodId: ids.method,
            messageType: MESSAGE_TYPE_CANCEL,
            value: CANCEL_FRAME
          }
        });
      } catch {
      }
    }
    function interruptOperations(error) {
      const requests = [...pending.keys()].map((requestId) => takePending(requestId));
      const streams = [...subscriptions.values()];
      subscriptions.clear();
      const instances = [];
      for (const route of hostRoutes.values()) {
        route.buffered.length = 0;
        instances.push(...route.instances.values());
        route.instances.clear();
      }
      for (const request of requests)
        request.reject(error);
      for (const instance of instances) {
        try {
          instance.unsubscribe();
        } catch {
        }
      }
      for (const subscription of streams) {
        try {
          subscription.onClose?.(error);
        } catch {
        }
      }
    }
    function closeWithError(error) {
      if (closedError)
        return;
      closedError = toError2(error);
      interruptOperations(closedError);
    }
    const onProtocolError = options.onProtocolError ?? closeWithError;
    const unsubscribeClose = provider.subscribeClose?.((error) => {
      closeWithError(error);
    });
    const unsubscribeReset = provider.subscribeReset?.((error) => {
      if (!closedError)
        interruptOperations(error);
    });
    const unsubscribeMessage = provider.subscribe((message) => {
      if (closedError) {
        return;
      }
      const decoded = decodeWireMessage(message);
      if (decoded.isErr()) {
        onProtocolError(decoded.error);
        return;
      }
      const { requestId, payload } = decoded.value;
      if (payload.traitId === PROTOCOL_ERROR_TRAIT_ID && payload.methodId === PROTOCOL_ERROR_METHOD_ID) {
        let unsupported;
        try {
          unsupported = decodeUnsupportedMessage(payload.value);
        } catch (error) {
          onProtocolError(toError2(error));
          return;
        }
        if (unsupported === void 0) {
          reportProtocolViolation(`unrecognised protocol error for request ${requestId}: discriminant ${payload.value[0]}`);
          if (pending.has(requestId)) {
            takePending(requestId)?.resolveUnsupported();
            return;
          }
          const stream = subscriptions.get(requestId);
          if (stream) {
            subscriptions.delete(requestId);
            stream.onClose?.(new UnsupportedMessageError(stream.ids.trait, stream.ids.method));
          }
          return;
        }
        const request = pending.get(requestId);
        if (request?.ids.trait === unsupported.traitId && request?.ids.method === unsupported.methodId) {
          takePending(requestId)?.resolveUnsupported();
          return;
        }
        const subscription2 = subscriptions.get(requestId);
        if (subscription2?.ids.trait === unsupported.traitId && subscription2?.ids.method === unsupported.methodId) {
          subscriptions.delete(requestId);
          subscription2.onClose?.(new UnsupportedMessageError(unsupported.traitId, unsupported.methodId));
        }
        return;
      }
      if (payload.traitId === SYSTEM_HANDSHAKE.trait && payload.methodId === SYSTEM_HANDSHAKE.method && payload.messageType === MESSAGE_TYPE_REQUEST) {
        let response;
        try {
          const request = VersionedHostHandshakeRequest.dec(payload.value);
          const requestedCodecVersion = request.value.codecVersion;
          response = requestedCodecVersion === TRUAPI_CODEC_VERSION ? encodeSuccessfulHandshakeResponse() : encodeUnsupportedHandshakeResponse();
        } catch (error) {
          reportProtocolViolation(`undecodable handshake request from the host (expected wire codec ${TRUAPI_CODEC_VERSION}): ${toError2(error).message}`);
          response = encodeUnsupportedHandshakeResponse();
        }
        try {
          send({
            requestId,
            payload: {
              traitId: SYSTEM_HANDSHAKE.trait,
              methodId: SYSTEM_HANDSHAKE.method,
              messageType: MESSAGE_TYPE_RESPONSE,
              value: response
            }
          });
        } catch {
        }
        return;
      }
      const hostRoute = hostRoutes.get(pairKey(payload.traitId, payload.methodId));
      if (hostRoute) {
        if (payload.messageType === MESSAGE_TYPE_START) {
          startHostSubscription(hostRoute, requestId, payload.value);
        } else if (payload.messageType === MESSAGE_TYPE_STOP) {
          const bufferedIndex = hostRoute.buffered.findIndex((start) => start.requestId === requestId);
          if (bufferedIndex >= 0)
            hostRoute.buffered.splice(bufferedIndex, 1);
          const instance = hostRoute.instances.get(requestId);
          if (instance) {
            hostRoute.instances.delete(requestId);
            instance.unsubscribe();
          }
        } else {
          reportProtocolViolation(`ignoring host-initiated frame for (${payload.traitId}, ${payload.methodId}): unexpected messageType ${payload.messageType}, expected Start (${MESSAGE_TYPE_START}) or Stop (${MESSAGE_TYPE_STOP})`);
        }
        return;
      }
      const p = pending.get(requestId);
      if (p) {
        if (payload.traitId !== p.ids.trait || payload.methodId !== p.ids.method) {
          reportProtocolViolation(`ignoring frame for request ${requestId}: got discriminant (${payload.traitId}, ${payload.methodId}), expected (${p.ids.trait}, ${p.ids.method})`);
        } else if (payload.messageType !== MESSAGE_TYPE_RESPONSE) {
          reportProtocolViolation(`ignoring frame for request ${requestId}: unexpected messageType ${payload.messageType}, expected Response (${MESSAGE_TYPE_RESPONSE}) on (${p.ids.trait}, ${p.ids.method})`);
          return;
        } else {
          takePending(requestId);
          try {
            p.resolve(payload.value);
          } catch (error) {
            p.reject(toError2(error));
          }
          return;
        }
      }
      const subscription = subscriptions.get(requestId);
      if (subscription) {
        if (payload.traitId === subscription.ids.trait && payload.methodId === subscription.ids.method && payload.messageType === MESSAGE_TYPE_RECEIVE) {
          try {
            subscription.onReceive(payload.value);
          } catch (error) {
            subscriptions.delete(requestId);
            subscription.onClose?.(toError2(error));
          }
        } else if (payload.traitId === subscription.ids.trait && payload.methodId === subscription.ids.method && payload.messageType === MESSAGE_TYPE_INTERRUPT) {
          subscriptions.delete(requestId);
          subscription.onInterrupt?.(payload.value);
        } else {
          reportProtocolViolation(`ignoring frame for subscription ${requestId}: got discriminant (${payload.traitId}, ${payload.methodId}) messageType ${payload.messageType}, expected receive (${MESSAGE_TYPE_RECEIVE}) or interrupt (${MESSAGE_TYPE_INTERRUPT}) on (${subscription.ids.trait}, ${subscription.ids.method})`);
        }
        return;
      }
      if (KNOWN_WIRE_IDS.has(`${payload.traitId}:${payload.methodId}`)) {
        if (payload.messageType === MESSAGE_TYPE_STOP || payload.messageType === MESSAGE_TYPE_INTERRUPT || payload.messageType === MESSAGE_TYPE_RECEIVE) {
          return;
        }
        if (payload.messageType !== MESSAGE_TYPE_REQUEST) {
          reportProtocolViolation(`ignoring frame for known pair (${payload.traitId}, ${payload.methodId}): unexpected messageType ${payload.messageType}`);
          return;
        }
      }
      reportProtocolViolation(`unsupported frame with discriminant (${payload.traitId}, ${payload.methodId}): request ${requestId} is not pending and has no subscription`);
      try {
        send({
          requestId,
          payload: {
            traitId: PROTOCOL_ERROR_TRAIT_ID,
            methodId: PROTOCOL_ERROR_METHOD_ID,
            messageType: MESSAGE_TYPE_RESPONSE,
            value: new Uint8Array([0, 0, payload.traitId, payload.methodId])
          }
        });
      } catch {
      }
    });
    function send(message) {
      if (closedError) {
        throw closedError;
      }
      const encoded = encodeWireMessage(message);
      if (encoded.isErr()) {
        closeWithError(encoded.error);
        throw encoded.error;
      }
      try {
        provider.postMessage(encoded.value);
      } catch (error) {
        closeWithError(error);
        throw toError2(error);
      }
    }
    function prepare(ids, ready, failed) {
      const onReady = () => {
        try {
          ready();
        } catch (error) {
          failed(error);
        }
      };
      try {
        if (options.prepare)
          void options.prepare(ids).then(onReady, failed);
        else
          onReady();
      } catch (error) {
        failed(error);
      }
    }
    function interruptHostSubscription(route, requestId, payload) {
      const instance = route.instances.get(requestId);
      if (instance) {
        route.instances.delete(requestId);
        instance.unsubscribe();
      }
      try {
        send({
          requestId,
          payload: {
            traitId: route.ids.trait,
            methodId: route.ids.method,
            messageType: MESSAGE_TYPE_INTERRUPT,
            value: payload
          }
        });
      } catch {
      }
    }
    function startHostSubscription(route, requestId, payload) {
      const previous = route.instances.get(requestId);
      if (previous) {
        route.instances.delete(requestId);
        previous.unsubscribe();
      }
      const handler = route.handler;
      if (!handler) {
        if (route.buffered.length === route.bufferCapacity) {
          const evicted = route.buffered.shift();
          if (evicted)
            interruptHostSubscription(route, evicted.requestId, route.declinePayload);
        }
        route.buffered.push({ requestId, payload });
        return;
      }
      let request;
      try {
        request = route.decodeRequest(payload);
      } catch {
        interruptHostSubscription(route, requestId, route.declinePayload);
        return;
      }
      let active = true;
      let teardown;
      const instance = {
        unsubscribe() {
          if (!active)
            return;
          active = false;
          teardown?.();
        }
      };
      route.instances.set(requestId, instance);
      const sendItem = (item) => {
        if (!active)
          return;
        try {
          send({
            requestId,
            payload: {
              traitId: route.ids.trait,
              methodId: route.ids.method,
              messageType: MESSAGE_TYPE_RECEIVE,
              value: route.encodeItem(item)
            }
          });
        } catch {
          interruptHostSubscription(route, requestId, route.declinePayload);
        }
      };
      const interrupt = (reason) => {
        if (!active)
          return;
        let encoded;
        try {
          encoded = route.encodeInterrupt(reason);
        } catch {
          encoded = route.declinePayload;
        }
        interruptHostSubscription(route, requestId, encoded);
      };
      try {
        teardown = handler(request, sendItem, interrupt);
        if (!active)
          teardown?.();
      } catch {
        interruptHostSubscription(route, requestId, route.declinePayload);
      }
    }
    return {
      /**
       * Send one request frame and resolve with the typed Ok/Err outcome
       * decoded from the response payload's `ResultPayload` envelope.
       */
      request({ ids, payload, decodeResponse, signal }) {
        const promise = new Promise((resolve, reject) => {
          if (closedError) {
            reject(closedError);
            return;
          }
          if (signal?.aborted) {
            reject(toError2(signal.reason));
            return;
          }
          const requestId = `${requestIdPrefix}${++idCounter}`;
          const isHandshake = ids.trait === SYSTEM_HANDSHAKE.trait && ids.method === SYSTEM_HANDSHAKE.method;
          const timeoutMs = isHandshake ? HANDSHAKE_TIMEOUT_MS : requestTimeoutMs;
          const deadline = setTimeout(() => {
            const entry = takePending(requestId);
            if (!entry)
              return;
            if (entry.sent)
              sendCancel(requestId, ids);
            reject(isHandshake ? new Error(`TrUAPI handshake timed out after ${HANDSHAKE_TIMEOUT_MS}ms; the host did not answer on wire codec ${TRUAPI_CODEC_VERSION}`) : new RequestTimeoutError(requestId, ids.trait, ids.method, timeoutMs));
          }, timeoutMs);
          const onAbort = () => {
            const entry = pending.get(requestId);
            if (!entry)
              return;
            if (entry.sent)
              sendCancel(requestId, ids);
            else
              takePending(requestId)?.reject(toError2(signal?.reason));
          };
          signal?.addEventListener("abort", onAbort, { once: true });
          pending.set(requestId, {
            ids,
            sent: false,
            resolve: (response) => resolve(decodeResponse(response)),
            resolveUnsupported: () => resolve({
              success: false,
              value: { tag: "Unsupported" }
            }),
            reject,
            // `takePending` cancels this for every settlement path, so no
            // deadline outlives the call it bounds.
            cancelTimeout: () => clearTimeout(deadline),
            detachAbort: () => signal?.removeEventListener("abort", onAbort)
          });
          prepare(ids, () => {
            const entry = pending.get(requestId);
            if (!entry)
              return;
            entry.sent = true;
            send({
              requestId,
              payload: {
                traitId: ids.trait,
                methodId: ids.method,
                messageType: MESSAGE_TYPE_REQUEST,
                value: payload
              }
            });
          }, (error) => takePending(requestId)?.reject(toError2(error)));
        });
        return ResultAsync.fromSafePromise(promise).andThen((result) => result.success ? okAsync(result.value) : errAsync(result.value));
      },
      /**
       * Start a raw subscription and route incoming receive/interrupt frames to
       * the supplied callbacks.
       */
      subscribeRaw({ ids, payload, onReceive, onInterrupt, onClose }) {
        if (closedError) {
          onClose?.(closedError);
          return { unsubscribe: () => {
          }, subscriptionId: "" };
        }
        const requestId = `${requestIdPrefix}${++idCounter}`;
        subscriptions.set(requestId, {
          ids,
          sent: false,
          onReceive,
          onInterrupt,
          onClose
        });
        prepare(ids, () => {
          const entry = subscriptions.get(requestId);
          if (!entry)
            return;
          entry.sent = true;
          send({
            requestId,
            payload: {
              traitId: ids.trait,
              methodId: ids.method,
              messageType: MESSAGE_TYPE_START,
              value: payload
            }
          });
        }, (error) => {
          if (subscriptions.delete(requestId))
            onClose?.(toError2(error));
        });
        return {
          subscriptionId: requestId,
          unsubscribe: () => {
            const entry = subscriptions.get(requestId);
            if (!entry)
              return;
            subscriptions.delete(requestId);
            if (!entry.sent)
              return;
            try {
              send({
                requestId,
                payload: {
                  traitId: ids.trait,
                  methodId: ids.method,
                  messageType: MESSAGE_TYPE_STOP,
                  value: STOP_FRAME
                }
              });
            } catch {
            }
          }
        };
      },
      registerHostInitiatedSubscription({ ids, decodeRequest, encodeItem, encodeInterrupt, declinePayload, bufferCapacity }) {
        const key = pairKey(ids.trait, ids.method);
        if (hostRoutes.has(key)) {
          throw new Error(`host-initiated subscription (${ids.trait}, ${ids.method}) is already registered`);
        }
        const route = {
          ids,
          decodeRequest,
          encodeItem,
          encodeInterrupt,
          declinePayload,
          bufferCapacity,
          buffered: [],
          instances: /* @__PURE__ */ new Map()
        };
        hostRoutes.set(key, route);
        return {
          setHandler(handler) {
            const installed = handler;
            route.handler = installed;
            for (const start of route.buffered.splice(0)) {
              startHostSubscription(route, start.requestId, start.payload);
            }
            return {
              unsubscribe() {
                if (route.handler === installed)
                  route.handler = void 0;
              }
            };
          }
        };
      },
      /**
       * Close this transport and detach its provider listeners.
       */
      dispose() {
        try {
          closeWithError(new Error("transport disposed"));
        } finally {
          unsubscribeMessage();
          unsubscribeClose?.();
          unsubscribeReset?.();
        }
      }
    };
  }

  // ../packages/truapi/dist/generated/internal-client.js
  var _transport23;
  var PermissionsClient2 = class {
    constructor(transport) {
      __privateAdd(this, _transport23);
      __privateSet(this, _transport23, transport);
    }
    /** Authorize one remote operation, consuming an available one-use grant. */
    authorizeRemotePermission(request, options) {
      return __privateGet(this, _transport23).request({
        ids: PERMISSIONS_AUTHORIZE_REMOTE_PERMISSION,
        payload: VersionedRemotePermissionRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedRemotePermissionResponse, CallError(VersionedRemotePermissionError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
    /** Authorize one device operation, consuming an available one-use grant. */
    authorizeDevicePermission(request, options) {
      return __privateGet(this, _transport23).request({
        ids: PERMISSIONS_AUTHORIZE_DEVICE_PERMISSION,
        payload: VersionedHostDevicePermissionRequest.enc({ tag: "V1", value: request }),
        signal: options?.signal,
        decodeResponse: (payload) => {
          const result = Result2(VersionedHostDevicePermissionResponse, CallError(VersionedHostDevicePermissionError)).dec(payload);
          return result.success ? { success: true, value: result.value.value } : result;
        }
      });
    }
  };
  _transport23 = new WeakMap();
  Object.freeze(PermissionsClient2.prototype);
  function createInternalClient(transport) {
    return Object.freeze({
      permissions: Object.freeze(new PermissionsClient2(transport))
    });
  }

  // ../packages/truapi/dist/host-connection.js
  var VISIBLE_RETRY_DELAYS_MS = [250, 1e3, 4e3];
  function createHostConnection(url, createProvider = createWebSocketProviderFactory()) {
    const now = performance.now.bind(performance);
    let current;
    let stopped = false;
    let receive;
    let reset;
    let legacy;
    let legacyPort;
    let status = "disconnected";
    let retries = 0;
    const listeners = /* @__PURE__ */ new Set();
    function setStatus(next) {
      if (status === next)
        return;
      status = next;
      for (const listener of [...listeners]) {
        if (status !== next)
          return;
        try {
          listener(next);
        } catch {
        }
      }
    }
    function retire(connection2, cause) {
      if (current !== connection2)
        return;
      current = void 0;
      const oldLegacy = legacy;
      legacy = void 0;
      oldLegacy?.close();
      try {
        reset?.(new ConnectionResetError({ cause }));
      } catch {
      }
      connection2.provider.dispose();
      if (!current)
        setStatus("disconnected");
      if (stopped)
        return;
      const delay = VISIBLE_RETRY_DELAYS_MS[retries];
      if (connection2.verified)
        setTimeout(activate, 0);
      else if (delay !== void 0 && page?.visibilityState === "visible") {
        retries += 1;
        setTimeout(activate, delay);
      }
    }
    function open() {
      if (stopped)
        throw new ConnectionResetError();
      if (current)
        return current;
      let provider;
      try {
        provider = createProvider(url);
      } catch (error) {
        setStatus("disconnected");
        throw error;
      }
      const connection2 = { provider, verified: false, checkedAt: 0 };
      current = connection2;
      provider.subscribe((frame) => {
        if (current !== connection2)
          return;
        connection2.checkedAt = now();
        if (legacy) {
          const decoded = decodeWireMessage(frame);
          if (decoded.isErr())
            return retire(connection2, decoded.error);
          if (!decoded.value.requestId.startsWith("host:"))
            return legacy.receive(frame);
        }
        receive?.(frame);
      });
      provider.subscribeClose?.((error) => retire(connection2, error));
      setStatus("connecting");
      return connection2;
    }
    function ready(connection2) {
      if (connection2.verified && now() - connection2.checkedAt < 1e4)
        return Promise.resolve();
      return connection2.checking ?? (connection2.checking = Promise.resolve(handshake()).then((result) => {
        if (result.isErr())
          throw result.error;
        if (current !== connection2)
          throw new ConnectionResetError();
        connection2.verified = true;
        connection2.checkedAt = now();
        retries = 0;
        setStatus("connected");
      }).catch((error) => {
        retire(connection2, error);
        throw error;
      }).finally(() => {
        connection2.checking = void 0;
      }));
    }
    function activate() {
      try {
        void ready(open()).catch(() => {
        });
      } catch {
      }
    }
    const page = typeof document === "undefined" ? void 0 : document;
    const onVisibilityChange = () => {
      if (page?.visibilityState !== "visible")
        return;
      retries = 0;
      activate();
    };
    page?.addEventListener("visibilitychange", onVisibilityChange);
    function stop() {
      page?.removeEventListener("visibilitychange", onVisibilityChange);
      stopped = true;
      if (current)
        retire(current);
    }
    const transport = createTransport({
      postMessage(frame) {
        const connection2 = current;
        if (!connection2)
          return;
        try {
          connection2.provider.postMessage(frame);
        } catch (error) {
          retire(connection2, error);
        }
      },
      subscribe(callback) {
        receive = callback;
        return () => {
          receive = void 0;
        };
      },
      subscribeReset(callback) {
        reset = callback;
        return () => {
          reset = void 0;
        };
      },
      dispose: stop
    }, {
      requestIdPrefix: "host:",
      onProtocolError(error) {
        if (current)
          retire(current, error);
      },
      prepare(ids) {
        const connection2 = open();
        return ids.trait === SYSTEM_HANDSHAKE.trait && ids.method === SYSTEM_HANDSHAKE.method ? connection2.provider.opened : ready(connection2);
      }
    });
    const client = createClient(transport);
    const handshake = client.system.handshake.bind(client.system);
    return {
      get client() {
        activate();
        return client;
      },
      internal: createInternalClient(transport),
      get legacyPort() {
        if (legacyPort)
          return legacyPort;
        const { port1: product, port2: host } = new MessageChannel();
        legacyPort = product;
        const adapter = {
          receive(frame) {
            host.postMessage(frame);
          },
          close() {
            host.close();
          }
        };
        legacy = adapter;
        const onMessage = (event) => {
          if (legacy !== adapter || !current?.verified)
            return;
          const { data: frame } = event;
          if (!(frame instanceof Uint8Array))
            return;
          const decoded = decodeWireMessage(frame);
          if (decoded.isErr() || decoded.value.requestId.startsWith("host:"))
            return;
          try {
            current.provider.postMessage(frame);
          } catch (error) {
            if (current)
              retire(current, error);
          }
        };
        try {
          void ready(open()).then(() => {
            if (legacy !== adapter)
              return;
            host.addEventListener("message", onMessage);
            host.start();
          }, () => adapter.close());
        } catch {
          adapter.close();
        }
        return product;
      },
      subscribeConnectionStatus(callback) {
        listeners.add(callback);
        callback(status);
        return () => {
          listeners.delete(callback);
        };
      },
      dispose() {
        try {
          stop();
        } finally {
          transport.dispose();
        }
      }
    };
  }

  // ../packages/truapi/dist/internal.js
  function freezeInternalResults() {
    for (const constructor of [Err, Ok, ResultAsync]) {
      Object.freeze(constructor.prototype);
      Object.freeze(constructor);
    }
  }

  // src/message-port-loss.ts
  var PROBE_TIMEOUT_MS = 1e3;
  var LATE_DEADLINE_MS = 250;
  var RELOADED_KEY = "__truapi_message_port_reload";
  function reloadAfterMessagePortLoss(win, subscribeConnectionStatus, canary = new MessageChannel()) {
    const now = win.performance.now.bind(win.performance);
    const storage = sessionStorageOf(win);
    let lost = storage?.getItem(RELOADED_KEY) != null;
    let deadline;
    function settle() {
      win.clearTimeout(deadline);
      deadline = void 0;
      canary.port1.onmessage = null;
    }
    function answered() {
      lost = false;
      storage?.removeItem(RELOADED_KEY);
      settle();
    }
    function check() {
      if (!lost || deadline !== void 0 || win.document.visibilityState !== "visible") return;
      const due = now() + PROBE_TIMEOUT_MS;
      deadline = win.setTimeout(() => {
        settle();
        if (now() - due > LATE_DEADLINE_MS) check();
        else reload();
      }, PROBE_TIMEOUT_MS);
      canary.port1.onmessage = answered;
      canary.port2.postMessage(null);
    }
    function reload() {
      if (storage?.getItem(RELOADED_KEY) != null) return;
      storage?.setItem(RELOADED_KEY, "1");
      win.location.reload();
    }
    let subscribed = false;
    const unsubscribe = subscribeConnectionStatus((status) => {
      if (!subscribed || status !== "disconnected") return;
      lost = true;
      check();
    });
    subscribed = true;
    win.document.addEventListener("visibilitychange", check);
    check();
    return () => {
      settle();
      unsubscribe();
      win.document.removeEventListener("visibilitychange", check);
    };
  }
  function sessionStorageOf(win) {
    try {
      return win.sessionStorage;
    } catch {
      return void 0;
    }
  }

  // src/network-transport.ts
  function createPermissionAuthorization(win, client) {
    const NativeURL = win.URL;
    const NativeAbortController = win.AbortController;
    const apply = Reflect.apply;
    const descriptor = Object.getOwnPropertyDescriptor;
    const hostname = descriptor(NativeURL.prototype, "hostname").get;
    const protocol = descriptor(NativeURL.prototype, "protocol").get;
    const indexOf = String.prototype.indexOf;
    function authorize(operation, decide) {
      const controller = new NativeAbortController();
      void (async () => {
        let allowed = false;
        try {
          if (client) allowed = await operation(controller.signal);
        } catch {
          allowed = false;
        }
        if (!controller.signal.aborted) {
          try {
            decide(allowed);
          } catch {
          }
        }
      })();
      return () => controller.abort();
    }
    function remote(permission, decide) {
      return authorize(async (signal) => {
        const result = await client.permissions.authorizeRemotePermission({ permission }, { signal });
        return result.isOk() && result.value.granted === true;
      }, decide);
    }
    return {
      network(url, decide) {
        try {
          const destination = new NativeURL(url);
          const scheme = apply(protocol, destination, []);
          const domain = apply(hostname, destination, []);
          if ((scheme === "http:" || scheme === "https:" || scheme === "ws:" || scheme === "wss:") && domain && apply(indexOf, domain, ["*"]) === -1) return remote({ tag: "Remote", value: { domains: [domain] } }, decide);
        } catch {
        }
        try {
          decide(false);
        } catch {
        }
        return () => {
        };
      },
      webRtc: client ? (decide) => remote({ tag: "WebRtc" }, decide) : false,
      media: client ? (audio, video, decide) => authorize(async (signal) => {
        if (!audio && !video) return false;
        if (video) {
          const result = await client.permissions.authorizeDevicePermission("Camera", { signal });
          if (result.isErr() || result.value.granted !== true || signal.aborted) return false;
        }
        if (audio) {
          const result = await client.permissions.authorizeDevicePermission("Microphone", { signal });
          if (result.isErr() || result.value.granted !== true) return false;
        }
        return true;
      }, decide) : false
    };
  }

  // src/permission-runtime.ts
  function freezePrototype(prototype) {
    for (const name of Reflect.ownKeys(prototype)) {
      const { value, writable, configurable } = Object.getOwnPropertyDescriptor(prototype, name);
      if (!writable || !configurable) continue;
      Object.defineProperty(prototype, name, {
        get: () => value,
        set(ownValue) {
          if (Object.isFrozen(this)) return;
          Object.defineProperty(this, name, { value: ownValue, writable: true, configurable: true, enumerable: true });
        }
      });
    }
    Object.freeze(prototype);
  }
  function freezePermissionRuntime() {
    Object.preventExtensions(Object.prototype);
    for (const name of [
      "Object",
      "Array",
      // Prevent forged decoded permission fields.
      "Map",
      "WeakMap",
      // Keep pending resolvers and private SDK transports inaccessible.
      "Set",
      // Prevent interception of private reply listeners.
      "Promise",
      // Prevent replacement of asynchronous permission results.
      "Number",
      "String",
      // Reserve host: request IDs for private permission replies.
      "Uint8Array",
      "DataView",
      // Prevent altered request and reply bytes.
      "MessageChannel",
      "MessagePort",
      "EventTarget"
      // Keep lazily created legacy endpoints private.
    ]) {
      const constructor = globalThis[name];
      if (name !== "Object" && name !== "Number") freezePrototype(constructor.prototype);
      Object.freeze(constructor);
      freezeValue(globalThis, name, constructor);
    }
    for (const prototype of [
      TextEncoder.prototype,
      TextDecoder.prototype,
      Object.getPrototypeOf(Uint8Array.prototype),
      Object.getPrototypeOf(Uint8Array),
      Object.getPrototypeOf([][Symbol.iterator]()),
      Object.getPrototypeOf(Object.getPrototypeOf([][Symbol.iterator]())),
      Object.getPrototypeOf((/* @__PURE__ */ new Map())[Symbol.iterator]()),
      Object.getPrototypeOf((/* @__PURE__ */ new Set())[Symbol.iterator]())
    ]) freezePrototype(prototype);
    freezeValue(globalThis, "BigInt", BigInt);
    freezeValue(globalThis, "setTimeout", setTimeout);
    freezeValue(globalThis, "clearTimeout", clearTimeout);
    freezeInternalResults();
  }

  // src/index.ts
  freezePermissionRuntime();
  var config = window.__truapi_localhost;
  freezeAndDelete(window, "__truapi_localhost");
  var connection = typeof config?.url === "string" ? createHostConnection(config.url) : void 0;
  if (connection) {
    freezeValue(window, "__HOST_WEBVIEW_MARK__", true);
    freezeValue(window, "__HOST_API_CLIENT__", Object.freeze({
      get client() {
        return connection.client;
      },
      subscribeConnectionStatus: connection.subscribeConnectionStatus
    }));
    Object.defineProperty(window, "__HOST_API_PORT__", {
      get: () => connection.legacyPort,
      set() {
      },
      configurable: false
    });
    const stopWatchingMessagePorts = reloadAfterMessagePortLoss(
      window,
      connection.subscribeConnectionStatus
    );
    window.addEventListener("pagehide", () => {
      stopWatchingMessagePorts();
      connection.dispose();
    });
  }
  installContainer(createPermissionAuthorization(window, connection?.internal), {
    nativeHttp: config?.nativeHttp === true
  });
})();
