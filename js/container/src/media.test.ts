import { expect, it } from 'bun:test';

import { installMediaPolicy } from './media.js';

function realm() {
  class MediaDevices {
    getUserMedia(_constraints?: MediaStreamConstraints) {
      return Promise.resolve('native-stream');
    }
    getDisplayMedia() {
      return Promise.resolve('native-screen');
    }
    enumerateDevices() {
      return Promise.resolve([{ deviceId: 'camera-1', kind: 'videoinput', label: 'Native Camera' }]);
    }
  }
  class Navigator {
    mediaDevices = new MediaDevices();
    getUserMedia(
      _constraints: MediaStreamConstraints,
      success: (value: string) => void,
      _failure: (error: unknown) => void,
    ) {
      success('native-stream');
    }
    webkitGetUserMedia(
      constraints: MediaStreamConstraints,
      success: (value: string) => void,
      failure: (error: unknown) => void,
    ) {
      this.getUserMedia(constraints, success, failure);
    }
  }
  return { navigator: new Navigator(), MediaDevices, Navigator, Promise, TypeError, DOMException };
}

it('denies instance and recovered prototype capture, including screen capture', async () => {
  const win = realm();
  win.navigator.mediaDevices.getUserMedia = () => Promise.resolve('instance-stream');
  installMediaPolicy(win);
  for (const name of ['getUserMedia', 'getDisplayMedia'] as const) {
    await expect(win.navigator.mediaDevices[name]({ video: true })).rejects.toMatchObject({ name: 'NotAllowedError' });
    await expect(win.MediaDevices.prototype[name].call(win.navigator.mediaDevices, { video: true })).rejects.toMatchObject({ name: 'NotAllowedError' });
  }
  Reflect.set(win.navigator.mediaDevices, 'getUserMedia', () => Promise.resolve('replacement'));
  Reflect.defineProperty(win.MediaDevices.prototype, 'getUserMedia', { value: () => Promise.resolve('replacement') });
  await expect(win.navigator.mediaDevices.getUserMedia({ video: true })).rejects.toMatchObject({ name: 'NotAllowedError' });
});

it('denies instance and recovered prototype device enumeration', async () => {
  const win = realm();
  installMediaPolicy(win);
  await expect(win.navigator.mediaDevices.enumerateDevices()).rejects.toMatchObject({ name: 'NotAllowedError' });
  await expect(win.MediaDevices.prototype.enumerateDevices.call(win.navigator.mediaDevices))
    .rejects.toMatchObject({ name: 'NotAllowedError' });
  Reflect.set(win.navigator.mediaDevices, 'enumerateDevices', () => Promise.resolve([]));
  await expect(win.navigator.mediaDevices.enumerateDevices()).rejects.toMatchObject({ name: 'NotAllowedError' });
});

it('denies legacy callback capture even without MediaDevices', async () => {
  const win = realm();
  Reflect.deleteProperty(win.navigator, 'mediaDevices');
  installMediaPolicy(win);
  for (const name of ['getUserMedia', 'webkitGetUserMedia'] as const) {
    const capture = new Promise<string>((resolve, reject) => {
      win.Navigator.prototype[name].call(win.navigator, { audio: true }, resolve, reject);
    });
    await expect(capture).rejects.toMatchObject({ name: 'NotAllowedError' });
  }
});
