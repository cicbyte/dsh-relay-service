// relay WS 隧道自测：经 ws-open 隧道打开 /api/remote.mux 并订阅 session/follow 快照。
// 用法：node test-mux.mjs [relayUrl] [code] [sessionId]
import WebSocket from 'ws';
import { randomUUID } from 'node:crypto';

const RELAY_URL = process.argv[2] || 'ws://127.0.0.1:8787';
const CODE = process.argv[3] || 'test-code-123456';

const ws = new WebSocket(RELAY_URL, 'dsh-relay-v1');
const send = (o) => ws.send(JSON.stringify(o));
const RID = `mux-${randomUUID()}`;

ws.on('open', () => send({ type: 'hello', role: 'client', code: CODE }));

ws.on('message', (data) => {
  const f = JSON.parse(data.toString('utf8'));
  switch (f.type) {
    case 'welcome':
      console.log('[mux-test] welcome, opening ws tunnel /api/remote.mux');
      send({ type: 'ws-open', rid: RID, path: '/api/remote.mux', headers: {} });
      return;
    case 'ws-frame': {
      if (f.text === '__open__') {
        console.log('[mux-test] tunnel open → session/follow');
        send({
          type: 'ws-frame',
          rid: RID,
          text: JSON.stringify({
            type: 'open',
            streamId: randomUUID(),
            endpoint: 'session/follow',
            payload: {
              args: {
                request: {
                  address: { kind: 'session', sessionId: process.argv[4] || '' },
                  maxMessages: 5,
                },
              },
            },
          }),
        });
        return;
      }
      let msg;
      try {
        msg = JSON.parse(f.text);
      } catch {
        return;
      }
      if (msg.type === 'item') {
        const v = msg.value || {};
        if (v.type === 'snapshot') {
          console.log(`[mux-test] PASS：snapshot 帧到达，cursor=${v.cursor} records=${(v.records || []).length}`);
          process.exit(0);
        } else {
          console.log('[mux-test] item frame:', v.type);
        }
      } else if (msg.type === 'error') {
        console.log('[mux-test] stream error:', JSON.stringify(msg.error));
        process.exit(1);
      }
      return;
    }
    case 'error':
      console.log('[mux-test] relay error:', JSON.stringify(f));
      process.exit(1);
    case 'ping':
      send({ type: 'pong', t: f.t });
      return;
    case 'ws-close':
      console.log('[mux-test] tunnel closed');
      process.exit(1);
    default:
      return;
  }
});

ws.on('error', (e) => {
  console.error('[mux-test] ws error', e.message);
  process.exit(1);
});
setTimeout(() => {
  console.error('[mux-test] timeout');
  process.exit(1);
}, 15000);
