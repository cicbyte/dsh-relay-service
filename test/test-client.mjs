// relay 链路自测客户端：以 role=client 连 relay，经桌面桥打 session/list。
// 用法：node test-client.mjs [relayUrl] [code]
import WebSocket from 'ws';

const RELAY_URL = process.argv[2] || 'ws://127.0.0.1:8787';
const CODE = process.argv[3] || 'test-code-123456';

const ws = new WebSocket(RELAY_URL, 'dsh-relay-v1');
const send = (o) => ws.send(JSON.stringify(o));

ws.on('open', () => {
  console.log('[test] connected, hello');
  send({ type: 'hello', role: 'client', code: CODE });
});

ws.on('message', (data) => {
  const f = JSON.parse(data.toString('utf8'));
  switch (f.type) {
    case 'welcome':
      console.log('[test] welcome, peerOnline =', f.peerOnline);
      if (!f.peerOnline) {
        console.log('[test] 桥未在线，先启动 bridge.mjs');
        process.exit(2);
      }
      console.log('[test] → http-req POST /api/session/list');
      send({
        type: 'http-req',
        rid: 'r1',
        method: 'POST',
        path: '/api/session/list',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          type: 'client-request',
          rpcId: 'test-1',
          method: 'session/list',
          payload: { args: { _request: {} } },
        }),
      });
      return;
    case 'http-res': {
      let ok = false;
      let n = 0;
      try {
        const msg = JSON.parse(f.body);
        ok = msg?.result?.ok === true;
        n = (msg?.result?.value?.items || []).length;
      } catch {}
      console.log(`[test] http-res status=${f.status} rpcOk=${ok} sessions=${n} setCookie=${(f.setCookie || []).length}`);
      if (ok && n > 0) {
        console.log('[test] PASS：云端转发全链路通（手机→relay→桥→dsh）');
        process.exit(0);
      } else {
        console.log('[test] FAIL body head:', String(f.body).slice(0, 200));
        process.exit(1);
      }
    }
    case 'error':
      console.log('[test] error frame:', f);
      process.exit(1);
    case 'ping':
      send({ type: 'pong', t: f.t });
      return;
    default:
      console.log('[test] frame:', f.type);
  }
});

ws.on('error', (e) => {
  console.error('[test] ws error', e.message);
  process.exit(1);
});
setTimeout(() => {
  console.error('[test] timeout');
  process.exit(1);
}, 15000);
