import { join } from 'path';

import { renderer } from '~config';
import { BrowserWindow, MessageChannelMain } from 'electron';

import { type CallWindowLaunch } from '@/shared/call-bridge';
import { ENVIRONMENT } from '../shared/constants/environment';

type CallWindowParams = {
  launch: CallWindowLaunch;
  iceConfig: { turnHost?: string; turnSecret?: string };
  mainWindow: BrowserWindow;
};

export function createCallWindow(params: CallWindowParams): BrowserWindow {
  const win = new BrowserWindow({
    width: 800,
    height: 660,
    resizable: false,
    center: true,
    autoHideMenuBar: true,
    webPreferences: {
      nodeIntegration: false,
      contextIsolation: true,
      preload: join(__dirname, 'call-window-preload.cjs'),
    },
  });

  win.webContents.session.setPermissionRequestHandler((_wc, permission, cb) => {
    cb(permission === 'media' || permission === 'mediaKeySystem');
  });

  // Load the main renderer bundle at the `/call` route (hash history). That
  // route is a sibling of the app's `_app` layout, so the call window shares the
  // bundle but runs no bootstrap / chat binding. Launch params + iceConfig + the
  // MessagePort are delivered below via the `call:init` handshake, not the URL.
  if (ENVIRONMENT.RENDERER_SOURCE === 'localhost') {
    win.loadURL(`${renderer.server.protocol}//${renderer.server.host}:${renderer.server.port}/#/call`);
  } else {
    win.loadURL(`file://${__dirname}/index.html#/call`);
  }

  win.webContents.on('did-finish-load', () => {
    const { port1, port2 } = new MessageChannelMain();
    params.mainWindow.webContents.postMessage('call:bridge-port', { launch: params.launch }, [port1]);
    win.webContents.postMessage('call:init', { launch: params.launch, iceConfig: params.iceConfig }, [port2]);
  });

  return win;
}
