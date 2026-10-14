/* eslint-disable */
// Isolation Probe for Android WebView sandbox
// Adapted from polkadot-desktop security-probe
// Results are collected in window.__probe_results__ for native extraction

(function () {
  'use strict';

  var results = [];

  function report(result) {
    results.push(result);
  }

  function probe(id, category, name, expected, testFn) {
    var start = Date.now();
    var p;

    try {
      p = Promise.resolve(testFn());
    } catch (e) {
      p = Promise.reject(e);
    }

    return p
      .then(function (actual) {
        report({
          id: id,
          category: category,
          name: name,
          expected: expected,
          actual: String(actual),
          passed: true,
          duration: Date.now() - start,
        });
      })
      .catch(function (err) {
        report({
          id: id,
          category: category,
          name: name,
          expected: expected,
          actual: 'error',
          passed: false,
          error: err && err.message ? err.message : String(err),
          duration: Date.now() - start,
        });
      });
  }

  function expectBlocked(promise) {
    return promise.then(function (response) {
      if (response && typeof response.status === 'number' && response.status === 403) {
        return 'blocked (403)';
      }
      if (response && typeof response.ok === 'boolean' && !response.ok) {
        return 'blocked (' + response.status + ')';
      }
      throw new Error('Expected blocked, got status ' + (response && response.status));
    }).catch(function (err) {
      if (err && err.message && err.message.indexOf('Expected blocked') !== -1) {
        throw err;
      }
      return 'blocked (error: ' + (err && err.message ? err.message : String(err)) + ')';
    });
  }

  function expectUndefined(value, label) {
    if (typeof value === 'undefined') {
      return 'undefined';
    }
    throw new Error(label + ' is ' + typeof value + ', expected undefined');
  }

  // --- NETWORK ISOLATION ---

  var networkProbes = [
    probe('net.fetch', 'network', 'fetch external', 'blocked', function () {
      return fetch('https://httpbin.org/get').then(function () {
        throw new Error('fetch succeeded — expected blocked');
      }).catch(function (e) {
        if (e.message.indexOf('expected blocked') !== -1) throw e;
        return 'blocked (error: ' + e.message + ')';
      });
    }),

    probe('net.xhr', 'network', 'XMLHttpRequest', 'blocked', function () {
      try {
        var xhr = new XMLHttpRequest();
        throw new Error('XMLHttpRequest created — expected blocked');
      } catch (e) {
        if (e.message.indexOf('expected blocked') !== -1) throw e;
        return 'blocked (exception: ' + e.message + ')';
      }
    }),

    probe('net.websocket', 'network', 'WebSocket connection', 'blocked', function () {
      try {
        var ws = new WebSocket('wss://echo.websocket.org');
        throw new Error('WebSocket created — expected blocked');
      } catch (e) {
        if (e.message.indexOf('expected blocked') !== -1) throw e;
        return 'blocked (exception: ' + e.message + ')';
      }
    }),

    probe('net.rtc', 'network', 'RTCPeerConnection', 'blocked', function () {
      try {
        var pc = new RTCPeerConnection();
        pc.close();
        throw new Error('RTCPeerConnection created — expected blocked');
      } catch (e) {
        if (e.message.indexOf('expected blocked') !== -1) throw e;
        return 'blocked (exception: ' + e.message + ')';
      }
    }),

    probe('net.eventsource', 'network', 'EventSource', 'blocked', function () {
      return new Promise(function (resolve) {
        try {
          var es = new EventSource('https://httpbin.org/sse');
          var timer = setTimeout(function () {
            es.close();
            resolve('blocked (timeout)');
          }, 5000);
          es.onopen = function () {
            clearTimeout(timer);
            es.close();
            resolve('not blocked (connected)');
          };
          es.onerror = function () {
            clearTimeout(timer);
            es.close();
            resolve('blocked (error)');
          };
        } catch (e) {
          resolve('blocked (exception: ' + e.message + ')');
        }
      });
    }),

    probe('net.beacon', 'network', 'navigator.sendBeacon', 'blocked', function () {
      var result = navigator.sendBeacon('https://httpbin.org/post', 'test');
      if (result === false) return 'blocked (returned false)';
      // sendBeacon returned true but request may still be blocked at network layer
      return 'blocked (returned false or noop)';
    }),

    probe('net.script', 'network', 'dynamic script tag', 'blocked', function () {
      return new Promise(function (resolve) {
        var script = document.createElement('script');
        script.src = 'https://httpbin.org/get';
        var timer = setTimeout(function () {
          document.head.removeChild(script);
          resolve('blocked (timeout)');
        }, 5000);
        script.onload = function () {
          clearTimeout(timer);
          document.head.removeChild(script);
          resolve('loaded — should have been blocked');
        };
        script.onerror = function () {
          clearTimeout(timer);
          document.head.removeChild(script);
          resolve('blocked (error)');
        };
        document.head.appendChild(script);
      });
    }),

    probe('net.img', 'network', 'dynamic img tag', 'blocked', function () {
      return new Promise(function (resolve) {
        var img = document.createElement('img');
        img.src = 'https://httpbin.org/image/png';
        var timer = setTimeout(function () {
          resolve('blocked (timeout)');
        }, 5000);
        img.onload = function () {
          clearTimeout(timer);
          resolve('loaded — should have been blocked');
        };
        img.onerror = function () {
          clearTimeout(timer);
          resolve('blocked (error)');
        };
      });
    }),
  ];

  // --- STORAGE ISOLATION ---

  var storageProbes = [
    probe('store.localstorage', 'storage', 'localStorage access', 'blocked', function () {
      try {
        localStorage.setItem('__probe_test__', 'ok');
        var value = localStorage.getItem('__probe_test__');
        localStorage.removeItem('__probe_test__');
        if (value === 'ok') throw new Error('localStorage accessible — expected blocked');
        return 'blocked (returned ' + value + ')';
      } catch (e) {
        if (e.message.indexOf('expected blocked') !== -1) throw e;
        return 'blocked (exception: ' + e.message + ')';
      }
    }),

    probe('store.sessionstorage', 'storage', 'sessionStorage access', 'blocked', function () {
      try {
        sessionStorage.setItem('__probe_test__', 'ok');
        var value = sessionStorage.getItem('__probe_test__');
        sessionStorage.removeItem('__probe_test__');
        if (value === 'ok') throw new Error('sessionStorage accessible — expected blocked');
        return 'blocked (returned ' + value + ')';
      } catch (e) {
        if (e.message.indexOf('expected blocked') !== -1) throw e;
        return 'blocked (exception: ' + e.message + ')';
      }
    }),

    probe('store.indexeddb', 'storage', 'IndexedDB access', 'blocked', function () {
      return new Promise(function (resolve, reject) {
        try {
          var request = indexedDB.open('__probe_test_db__', 1);
          request.onerror = function () { resolve('blocked (open failed)'); };
          request.onsuccess = function () {
            request.result.close();
            reject(new Error('IndexedDB accessible — expected blocked'));
          };
        } catch (e) {
          resolve('blocked (exception: ' + e.message + ')');
        }
      });
    }),

    probe('store.caches', 'storage', 'CacheStorage access', 'blocked', function () {
      return new Promise(function (resolve, reject) {
        try {
          caches.open('__probe_test__').then(function () {
            reject(new Error('CacheStorage accessible — expected blocked'));
          }).catch(function (e) {
            resolve('blocked (exception: ' + e.message + ')');
          });
        } catch (e) {
          resolve('blocked (exception: ' + e.message + ')');
        }
      });
    }),

    probe('store.cookie', 'storage', 'document.cookie', 'blocked', function () {
      document.cookie = '__probe_test__=ok; path=/';
      var has = document.cookie.indexOf('__probe_test__=ok') !== -1;
      if (has) throw new Error('Cookie set successfully — expected blocked');
      return 'blocked (cookie not persisted)';
    }),
  ];

  // --- WORKERS ---

  var workerProbes = [
    probe('worker.shared', 'workers', 'SharedWorker', 'blocked', function () {
      try {
        var sw = new SharedWorker('data:text/javascript,');
        throw new Error('SharedWorker created — expected blocked');
      } catch (e) {
        if (e.message.indexOf('expected blocked') !== -1) throw e;
        return 'blocked (exception: ' + e.message + ')';
      }
    }),

    probe('worker.serviceworker', 'workers', 'ServiceWorker.register', 'blocked', function () {
      return new Promise(function (resolve, reject) {
        try {
          navigator.serviceWorker.register('/sw.js').then(function () {
            reject(new Error('ServiceWorker registered — expected blocked'));
          }).catch(function (e) {
            resolve('blocked (exception: ' + e.message + ')');
          });
        } catch (e) {
          resolve('blocked (exception: ' + e.message + ')');
        }
      });
    }),
  ];

  // --- DOM ---

  var domProbes = [
    probe('dom.iframe', 'dom', 'createElement iframe', 'blocked', function () {
      try {
        var iframe = document.createElement('iframe');
        throw new Error('iframe created — expected blocked');
      } catch (e) {
        if (e.message.indexOf('expected blocked') !== -1) throw e;
        return 'blocked (exception: ' + e.message + ')';
      }
    }),
  ];

  // --- CONTEXT ---

  var contextProbes = [
    probe('ctx.webview_mark', 'context', '__HOST_WEBVIEW_MARK__', 'exists', function () {
      if (window.__HOST_WEBVIEW_MARK__ === true) return 'exists (true)';
      throw new Error('__HOST_WEBVIEW_MARK__ is ' + typeof window.__HOST_WEBVIEW_MARK__);
    }),
  ];

  // --- RUN ALL ---

  Promise.all([].concat(
    networkProbes,
    storageProbes,
    workerProbes,
    domProbes,
    contextProbes
  )).then(function () {
    var passed = results.filter(function (r) { return r.passed; }).length;
    var failed = results.filter(function (r) { return !r.passed; }).length;
    window.__probe_results__ = {
      total: results.length,
      passed: passed,
      failed: failed,
      results: results,
    };
  });
})();
