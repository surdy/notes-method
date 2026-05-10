import test from 'node:test';
import assert from 'node:assert/strict';

import { registerHotkeys } from '../src/lib/hotkeys.ts';

function createWindowStub() {
  let keydownHandler: ((event: KeyboardEvent) => void) | undefined;

  return {
    window: {
      addEventListener(type: string, handler: EventListenerOrEventListenerObject) {
        if (type === 'keydown') {
          keydownHandler = handler as (event: KeyboardEvent) => void;
        }
      },
      removeEventListener(type: string, handler: EventListenerOrEventListenerObject) {
        if (type === 'keydown' && keydownHandler === handler) {
          keydownHandler = undefined;
        }
      }
    } as unknown as Window,
    dispatch(event: KeyboardEvent) {
      keydownHandler?.(event);
    }
  };
}

function createKeyboardEvent(init: {
  key: string;
  metaKey?: boolean;
  shiftKey?: boolean;
  altKey?: boolean;
}) {
  return {
    key: init.key,
    metaKey: init.metaKey ?? false,
    shiftKey: init.shiftKey ?? false,
    altKey: init.altKey ?? false,
    defaultPrevented: false,
    preventDefault() {
      this.defaultPrevented = true;
    }
  } as KeyboardEvent & { defaultPrevented: boolean };
}

test('registerHotkeys runs the matching action and prevents default', () => {
  const stub = createWindowStub();
  const originalWindow = globalThis.window;
  Object.defineProperty(globalThis, 'window', { value: stub.window, configurable: true, writable: true });

  let triggered = 0;
  registerHotkeys([{ key: 'k', meta: true, shift: true, action: () => triggered += 1 }]);

  const event = createKeyboardEvent({ key: 'K', metaKey: true, shiftKey: true });
  stub.dispatch(event);

  assert.equal(triggered, 1);
  assert.equal(event.defaultPrevented, true);

  Object.defineProperty(globalThis, 'window', { value: originalWindow, configurable: true, writable: true });
});

test('registerHotkeys cleanup removes the keydown handler', () => {
  const stub = createWindowStub();
  const originalWindow = globalThis.window;
  Object.defineProperty(globalThis, 'window', { value: stub.window, configurable: true, writable: true });

  let triggered = 0;
  const unregister = registerHotkeys([{ key: 'o', meta: true, action: () => triggered += 1 }]);
  unregister();

  stub.dispatch(createKeyboardEvent({ key: 'o', metaKey: true }));
  assert.equal(triggered, 0);

  Object.defineProperty(globalThis, 'window', { value: originalWindow, configurable: true, writable: true });
});
