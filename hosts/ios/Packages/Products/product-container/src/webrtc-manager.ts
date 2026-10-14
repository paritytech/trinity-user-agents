/**
 * Permission-gated RTCPeerConnection for the product sandbox.
 *
 * A peer connection is inert until an SDP/ICE method touches the network, so the
 * network-initiating async methods are gated behind a permission request rather than
 * the (synchronous) constructor. Access is requested once per connection on first use
 * and cached, matching the allowNetworkAccess flow.
 */
export type WebRtcAccessRequester = () => Promise<boolean>;

type AnyMethod = (...args: any[]) => any;

export class WebRtcManager {
  readonly connectionClass: typeof RTCPeerConnection;

  private readonly requestAccess: WebRtcAccessRequester;
  private readonly permissions = new WeakMap<object, Promise<boolean>>();

  constructor(nativeConnectionClass: typeof RTCPeerConnection, requestAccess: WebRtcAccessRequester) {
    this.requestAccess = requestAccess;
    this.connectionClass = this.makeConnectionClass(nativeConnectionClass);
  }

  private makeConnectionClass(nativeConnectionClass: typeof RTCPeerConnection): typeof RTCPeerConnection {
    const ensureAllowed = (connection: RTCPeerConnection, method: string) =>
      this.ensureAllowed(connection, method);

    return class GatedRTCPeerConnection extends nativeConnectionClass {
      createOffer(...args: any[]): any {
        return ensureAllowed(this, 'createOffer').then(() => (super.createOffer as AnyMethod)(...args));
      }

      createAnswer(...args: any[]): any {
        return ensureAllowed(this, 'createAnswer').then(() => (super.createAnswer as AnyMethod)(...args));
      }

      setLocalDescription(...args: any[]): any {
        return ensureAllowed(this, 'setLocalDescription').then(() => (super.setLocalDescription as AnyMethod)(...args));
      }

      setRemoteDescription(...args: any[]): any {
        return ensureAllowed(this, 'setRemoteDescription').then(() => (super.setRemoteDescription as AnyMethod)(...args));
      }

      addIceCandidate(...args: any[]): any {
        return ensureAllowed(this, 'addIceCandidate').then(() => (super.addIceCandidate as AnyMethod)(...args));
      }
    };
  }

  private async ensureAllowed(connection: RTCPeerConnection, method: string): Promise<void> {
    console.log('[webRtc] api accessed:', method);

    let permission = this.permissions.get(connection);
    if (!permission) {
      console.log('[webRtc] requesting access');
      permission = this.requestAccess();
      this.permissions.set(connection, permission);
    }

    if (!(await permission)) {
      console.warn('[webRtc] access denied, blocking:', method);
      connection.close();
      throw new TypeError('WebRTC access is not allowed');
    }

    console.log('[webRtc] access granted:', method);
  }
}
