// DSH 云端转发桌面桥（v1）
//
// 运行在跑 dsh web 的桌面机上：
//   relay(公网) ←─ WS(dsh-relay-v1) ─→ 本桥 ─→ http://127.0.0.1:3080（dsh web）
//
// 职责：
//   - 以 role=host 连接 relay（共享配对码），断线指数退避自动重连（单飞）；
//   - http-req → fetch 到本机 dsh web → http-res（剥离 hop-by-hop 头，Host 固定为
//     loopback 以通过 DSH 信任栅栏；手机的 Cookie 头透传）；
//   - ws-open → 连本机 WS 端点（如 /api/remote.mux），双向泵 ws-frame。
//
// 用法：
//   $env:RELAY_URL='wss://your-vps:8787'; $env:RELAY_CODE='<长随机配对码>'
//   $env:DSH_URL='http://127.0.0.1:3080'   # 可选，默认本机 dsh web
//   node bridge.mjs

import WebSocket from 'ws';

const RELAY_URL = process.env.RELAY_URL || 'ws://127.0.0.1:8787';
const RELAY_CODE = process.env.RELAY_CODE || '';
const DSH_URL = (process.env.DSH_URL || 'http://127.0.0.1:3080').replace(/\/+$/, '');

if (!RELAY_CODE || RELAY_CODE.length < 6) {
  console.error('[bridge] RELAY_CODE 未设置或过短（手机端必须输入同一配对码）');
  process.exit(1);
}

console.log(`[bridge] dsh=${DSH_URL} relay=${RELAY_URL}`);

let ws = null;
let closed = false;
let reconnectDelay = 1000;
const tunnels = new Map(); // rid → WebSocket
// v3 续传/批量：已处理最大 seq、relay batch 能力、出站合并队列
let lastSeq = 0;
let relayBatch = false;
let outbox = [];
let outboxTimer = null;

function send(obj) {
  if (outbox.length >= 2048) {
    console.error('[bridge] 断线缓冲溢出，重建隧道');
    outbox.length = 0;
    closeAllTunnels('relay-offline-overflow');
  }
  outbox.push(JSON.stringify(obj));
  if (outboxTimer) return;
  outboxTimer = setImmediate(() => {
    outboxTimer = null;
    flushOutbox();
  });
}

function flushOutbox() {
  if (!outbox.length || !(ws && ws.readyState === WebSocket.OPEN)) return;
  const frames = outbox.splice(0, outbox.length);
  if (relayBatch && frames.length > 1) {
    ws.send(JSON.stringify({ type: 'batch', frames }));
  } else {
    for (const f of frames) ws.send(f);
  }
}

function connect() {
  ws = new WebSocket(RELAY_URL, 'dsh-relay-v1');

  ws.on('open', () => {
    // 退避重置移到 welcome：open 即重置是 1/s 重连风暴根因（限流活锁喂养者）
    // hello 直发（outbox 是断线期积压，排在 hello 前会被 bad-hello）
    ws.send(JSON.stringify({ type: 'hello', role: 'host', code: RELAY_CODE, batch: true, resumeFrom: lastSeq }));
    console.log('[bridge] connected to relay');
  });

  ws.on('message', async (data) => {
    let frame;
    try {
      frame = JSON.parse(data.toString('utf8'));
    } catch {
      return;
    }
    handleFrame(frame);
  });

  ws.on('close', () => {
    // v3 续传：断线不拆隧道——本地 WS 继续收进 outbox，重连 flush/回放无缝续流
    if (closed) return;
    setTimeout(connect, reconnectDelay);
    reconnectDelay = Math.min(reconnectDelay * 2, 30_000);
    console.log('[bridge] relay disconnected, reconnecting (tunnels kept)...');
  });
  ws.on('error', (e) => console.error('[bridge] ws error:', e.message));
}

function handleFrame(frame) {
  switch (frame.type) {
    case 'welcome':
      reconnectDelay = 1000;
      relayBatch = !!frame.batch;
      console.log(`[bridge] welcomed (peerOnline=${frame.peerOnline})`);
      flushOutbox();
      return;
    case 'batch':
      for (const s of frame.frames || []) {
        try {
          handleFrame(JSON.parse(s));
        } catch {}
      }
      return;
    case 'resume':
      if (!frame.ok) {
        console.log(`[bridge] resume reset (${frame.reason || 'gap'})，重建隧道`);
        outbox.length = 0;
        closeAllTunnels('resume-reset');
      } else {
        console.log(`[bridge] resumed (replayed=${frame.count ?? 0})`);
      }
      return;
    case 'ping':
      send({ type: 'pong', t: frame.t });
      return;
    case 'bye':
      console.log(`[bridge] relay bye: ${frame.code}`);
      ws.close();
      return;
    case 'peer':
      console.log(`[bridge] phone ${frame.online ? 'online' : 'offline'}`);
      if (!frame.online) closeAllTunnels('peer-offline');
      return;
    case 'http-req':
    case 'ws-open':
    case 'ws-frame':
    case 'ws-close':
      if (Number.isInteger(frame.seq) && frame.seq > lastSeq) lastSeq = frame.seq;
      if (frame.type === 'http-req') return handleHttp(frame);
      if (frame.type === 'ws-open') return handleWsOpen(frame);
      if (frame.type === 'ws-frame') return handleWsFrame(frame);
      return handleWsClose(frame);
    case 'reject':
      console.error(`[bridge] rejected: ${frame.code}`);
      if (frame.code === 'rate-limited') {
        // 限流：长退避 + 尊重 retryAfterSecs（严禁 1/s 喂养限流窗口成活锁）
        reconnectDelay = Math.max(reconnectDelay, (Number(frame.retryAfterSecs) || 0) * 1000, 30_000);
      }
      return;
    default:
      return;
  }
}

async function handleHttp(frame) {
  const { rid, method = 'GET', path = '/', body, headers = {} } = frame;
  try {
    const url = new URL(path, DSH_URL + '/');
    const reqHeaders = {};
    for (const [k, v] of Object.entries(headers || {})) {
      const lk = k.toLowerCase();
      if (['cookie', 'content-type', 'accept', 'authorization'].includes(lk)) reqHeaders[lk] = v;
    }
    const resp = await fetch(url, {
      method,
      headers: reqHeaders,
      body: body === undefined || body === null ? undefined : String(body),
      redirect: 'manual',
    });
    const text = await resp.text();
    send({
      type: 'http-res',
      rid,
      status: resp.status,
      body: text,
      setCookie: resp.headers.getSetCookie ? resp.headers.getSetCookie() : [],
    });
  } catch (e) {
    send({ type: 'error', rid, code: 'bridge-http-failed', message: String(e?.message || e) });
  }
}

function handleWsOpen(frame) {
  const { rid, path = '/', headers = {} } = frame;
  // 幂等重开：同 rid 先拆旧隧道（避免残留隧道吞掉后续 open）
  if (tunnels.has(rid)) {
    try {
      tunnels.get(rid).close();
    } catch {}
    tunnels.delete(rid);
  }
  const target = DSH_URL.replace(/^http/, 'ws') + path;
  const t = new WebSocket(target, {
    headers: { cookie: headers.cookie || '' },
  });
  tunnels.set(rid, t);
  t.on('open', () => send({ type: 'ws-frame', rid, text: '__open__' }));
  t.on('message', (data, isBinary) => {
    if (isBinary) return; // dsh mux 均为文本帧
    send({ type: 'ws-frame', rid, text: data.toString('utf8') });
  });
  t.on('close', () => {
    tunnels.delete(rid);
    send({ type: 'ws-close', rid });
  });
  t.on('error', (e) => {
    send({ type: 'error', rid, code: 'bridge-ws-failed', message: String(e?.message || e) });
    tunnels.delete(rid);
    try {
      t.close();
    } catch {}
  });
}

function handleWsFrame(frame) {
  const t = tunnels.get(frame.rid);
  if (t && t.readyState === WebSocket.OPEN) t.send(frame.text);
}

function handleWsClose(frame) {
  const t = tunnels.get(frame.rid);
  if (t) {
    tunnels.delete(frame.rid);
    try {
      t.close();
    } catch {}
  }
}

function closeAllTunnels(reason) {
  for (const [rid, t] of tunnels) {
    console.log(`[bridge] close tunnel ${rid} (${reason})`);
    try {
      t.close();
    } catch {}
  }
  tunnels.clear();
}

process.on('SIGINT', () => {
  closed = true;
  ws?.close();
  process.exit(0);
});

connect();
