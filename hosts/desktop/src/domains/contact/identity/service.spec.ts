import { describe, expect, it } from 'vitest';

import { DeviceRosterEvent } from './schemas';
import { contactService } from './service';
import { type Contact, type Device } from './types';

const device = (statementAccountId: string, encryptionPublicKey = '04beef'): Device => ({
  statementAccountId,
  encryptionPublicKey,
});

const contactWith = (devices: Device[]): Contact => ({
  accountId: '0x01',
  identityChatPublicKey: '04abc',
  devices,
  lastUpdate: 0,
});

describe('contactService.hasKnownDevices', () => {
  it('returns false when the device list is empty', () => {
    expect(contactService.hasKnownDevices(contactWith([]))).toBe(false);
  });

  it('returns true when the device list is non-empty', () => {
    expect(contactService.hasKnownDevices(contactWith([device('0xaa')]))).toBe(true);
  });
});

describe('contactService.addDevice', () => {
  it('appends a new device to the list', () => {
    const c = contactWith([device('0xaa')]);
    const next = contactService.addDevice(c, device('0xbb'));

    expect(next.devices.map(d => d.statementAccountId)).toEqual(['0xaa', '0xbb']);
  });

  it('does not duplicate an existing device by statementAccountId', () => {
    const c = contactWith([device('0xaa', '04old')]);
    const next = contactService.addDevice(c, device('0xaa', '04new'));

    expect(next.devices).toHaveLength(1);
  });

  it('replaces the encryption key when a duplicate device is added', () => {
    const c = contactWith([device('0xaa', '04old')]);
    const next = contactService.addDevice(c, device('0xaa', '04new'));

    expect(next.devices[0]?.encryptionPublicKey).toBe('04new');
  });

  it('does not mutate the input contact', () => {
    const c = contactWith([device('0xaa')]);
    contactService.addDevice(c, device('0xbb'));

    expect(c.devices).toHaveLength(1);
  });
});

describe('contactService.removeDevice', () => {
  it('drops the device with the matching statementAccountId', () => {
    const c = contactWith([device('0xaa'), device('0xbb')]);
    const next = contactService.removeDevice(c, '0xaa');

    expect(next.devices.map(d => d.statementAccountId)).toEqual(['0xbb']);
  });

  it('is a no-op when the device is not present', () => {
    const c = contactWith([device('0xaa')]);
    const next = contactService.removeDevice(c, '0xzz');

    expect(next.devices).toEqual(c.devices);
  });

  it('does not mutate the input contact', () => {
    const c = contactWith([device('0xaa')]);
    contactService.removeDevice(c, '0xaa');

    expect(c.devices).toHaveLength(1);
  });
});

describe('contactService.applyRosterEvent', () => {
  const decode = (bytes: Uint8Array) => DeviceRosterEvent.dec(bytes);

  it('hex-encodes and adds the device on DeviceAdded', () => {
    const before = contactWith([]);
    const event = decode(
      DeviceRosterEvent.enc({
        tag: 'DeviceAdded' as const,
        value: {
          statementAccountId: new Uint8Array(32).fill(0xaa),
          encryptionPublicKey: new Uint8Array(32).fill(0x04),
        },
      }),
    );

    const after = contactService.applyRosterEvent(before, event);

    expect(after.devices).toHaveLength(1);
    expect(after.devices[0]?.statementAccountId).toBe(`0x${'aa'.repeat(32)}`);
    expect(after.devices[0]?.encryptionPublicKey).toBe(`0x${'04'.repeat(32)}`);
  });

  it('removes the matching device on DeviceRemoved', () => {
    const before = contactWith([device(`0x${'aa'.repeat(32)}`), device(`0x${'bb'.repeat(32)}`)]);
    const event = decode(
      DeviceRosterEvent.enc({
        tag: 'DeviceRemoved' as const,
        value: { statementAccountId: new Uint8Array(32).fill(0xaa) },
      }),
    );

    const after = contactService.applyRosterEvent(before, event);

    expect(after.devices.map(d => d.statementAccountId)).toEqual([`0x${'bb'.repeat(32)}`]);
  });

  it('replaces the encryption key when DeviceAdded fires for an existing device', () => {
    const existing = `0x${'aa'.repeat(32)}`;
    const before = contactWith([{ statementAccountId: existing, encryptionPublicKey: '04old' }]);
    const event = decode(
      DeviceRosterEvent.enc({
        tag: 'DeviceAdded' as const,
        value: {
          statementAccountId: new Uint8Array(32).fill(0xaa),
          encryptionPublicKey: new Uint8Array(32).fill(0x05),
        },
      }),
    );

    const after = contactService.applyRosterEvent(before, event);

    expect(after.devices).toHaveLength(1);
    expect(after.devices[0]?.encryptionPublicKey).toBe(`0x${'05'.repeat(32)}`);
  });

  it('is a no-op on DeviceRemoved when the device is unknown', () => {
    const before = contactWith([device(`0x${'bb'.repeat(32)}`)]);
    const event = decode(
      DeviceRosterEvent.enc({
        tag: 'DeviceRemoved' as const,
        value: { statementAccountId: new Uint8Array(32).fill(0xff) },
      }),
    );

    const after = contactService.applyRosterEvent(before, event);

    expect(after.devices).toEqual(before.devices);
  });

  it('does not mutate the input contact', () => {
    const before = contactWith([]);
    const event = decode(
      DeviceRosterEvent.enc({
        tag: 'DeviceAdded' as const,
        value: {
          statementAccountId: new Uint8Array(32).fill(0xaa),
          encryptionPublicKey: new Uint8Array(32).fill(0x04),
        },
      }),
    );

    contactService.applyRosterEvent(before, event);

    expect(before.devices).toEqual([]);
  });
});

const accountId = (fill: number) => new Uint8Array(32).fill(fill);

describe('contactService.computeRosterTopic', () => {
  it('returns a 32-byte topic', () => {
    expect(contactService.computeRosterTopic(accountId(0xa1)).length).toBe(32);
  });

  it('is deterministic for the same accountId', () => {
    const a = contactService.computeRosterTopic(accountId(0xa1));
    const b = contactService.computeRosterTopic(accountId(0xa1));
    expect(Buffer.from(a).equals(Buffer.from(b))).toBe(true);
  });

  it('differs for different accountIds', () => {
    const a = contactService.computeRosterTopic(accountId(0xa1));
    const b = contactService.computeRosterTopic(accountId(0xa2));
    expect(Buffer.from(a).equals(Buffer.from(b))).toBe(false);
  });
});

describe('contactService.computeRosterSubscriptionTopics', () => {
  it('returns one topic per known contact', () => {
    const topics = contactService.computeRosterSubscriptionTopics([accountId(0x01), accountId(0x02), accountId(0x03)]);
    expect(topics).toHaveLength(3);
    for (const t of topics) {
      expect(t.length).toBe(32);
    }
  });

  it('returns empty when there are no contacts', () => {
    expect(contactService.computeRosterSubscriptionTopics([])).toEqual([]);
  });

  it('topics line up with computeRosterTopic for each contact', () => {
    const a = accountId(0x01);
    const b = accountId(0x02);
    const subscribed = contactService.computeRosterSubscriptionTopics([a, b]);
    expect(Buffer.from(subscribed[0]!).equals(Buffer.from(contactService.computeRosterTopic(a)))).toBe(true);
    expect(Buffer.from(subscribed[1]!).equals(Buffer.from(contactService.computeRosterTopic(b)))).toBe(true);
  });
});
