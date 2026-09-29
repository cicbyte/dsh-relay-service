// #836 实机 e2e：双手机同房间并发不串线 + 吊销独立。
//
// 前置：MuMu 真机已以 client 身份连在房间 ROOM（A），独立桥 host 在线。
// 本脚本扮演「第二台手机」B：
//   1. 新配对码入房（welcome 取设备身份）；
//   2. 并发 N 个 http-req（各自 rid）过桥打 dsh web——逐 rid 校验回包归属（不串线）；
//   3. 管理台吊销 B → B 被踢（bye/reject），A 保持在线（吊销独立）。
// 环境变量：RELAY_HOST / ADMIN_PASSWORD / ROOM
import WebSocket from 'ws';

const HOST = process.env.RELAY_HOST || '127.0.0.1';
const ADMIN = process.env.ADMIN_URL || `http://127.0.0.1:8788`;
const ROOM = process.env.ROOM || 'f1f008b4';
const ADMIN_PASSWORD = process.env.ADMIN_PASSWORD || 'test-admin-pw-123';

let pass = 0, fail = 0;
const check = (ok, name, detail = '') => {
  if (ok) { pass++; console.log(`  ok ${name}`); }
  else { fail++; console.log(`FAIL ${name} ${detail}`); }
};

const api = async (path, at, opt = {}) => {
  const r = await fetch(ADMIN + path, {
    method: opt.method || 'GET',
    headers: { 'content-type': 'application/json', ...(at ? { authorization: 'Bearer ' + at } : {}) },
    body: opt.body ? JSON.stringify(opt.body) : undefined,
  });
  return r.json();
};

/** 连 relay → 发 hello → 等 welcome/reject（首帧）；返回 {ws, frame} */
const wsHello = (hello, timeoutMs = 8000) => new Promise((resolve, reject) => {
  const ws = new WebSocket(`ws://${HOST}:8787`, ['dsh-relay-v1']);
  const timer = setTimeout(() => { ws.terminate(); reject(new Error('hello timeout')); }, timeoutMs);
  ws.on('open', () => ws.send(JSON.stringify(hello)));
  ws.on('message', (data) => {
    let frame;
    try { frame = JSON.parse(String(data)); } catch { return; }
    if (frame.type === 'welcome' || frame.type === 'reject') {
      clearTimeout(timer);
      resolve({ ws, frame });
    }
  });
  ws.on('error', (e) => { clearTimeout(timer); reject(e); });
});

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// ---- 0. 基线：A（真机）与 host 在线 ----
const login = await api('/api/auth/login', null, { method: 'POST', body: { username: 'admin', password: ADMIN_PASSWORD } });
const at = login.result.accessToken;
const roomView = (await api('/api/rooms', at)).result.find((r) => r.room === ROOM);
check(roomView?.hostOnline === true, 'host(桥)在线');
check(roomView?.clientsOnline >= 1, '真机 A 在线', JSON.stringify(roomView?.clientsOnline));

// ---- 1. B 拿新配对码入房 ----
const pairB = await api('/api/pairing-codes', at, { method: 'POST', body: { role: 'client', name: 'sim-phone-B', room: ROOM } });
const b = await wsHello({ type: 'hello', role: 'client', pairingCode: pairB.result.code, name: 'sim-phone-B' });
check(b.frame.type === 'welcome' && b.frame.room === ROOM, 'B 配对入房', JSON.stringify(b.frame).slice(0, 120));
check(!!b.frame.device?.token, 'B 拿到一次性设备令牌');
const devB = b.frame.device;

const view2 = (await api('/api/rooms', at)).result.find((r) => r.room === ROOM);
check(view2.clientsOnline === 2, '双手机同房并发在线(A+B)', String(view2.clientsOnline));

// ---- 2. 并发请求不串线：B 发 N 个带 rid 的 http-req，回包必须原 rid 原值回来 ----
const N = 8;
const got = new Map();
const waiter = new Promise((resolve) => {
  b.ws.on('message', (data) => {
    let f;
    try { f = JSON.parse(String(data)); } catch { return; }
    if (f.type === 'http-res') {
      got.set(f.rid, f);
      if (got.size >= N) resolve();
    }
    if (f.type === 'error' && f.rid) got.set('err:' + f.rid, f);
  });
});
const sent = [];
for (let i = 0; i < N; i++) {
  const rid = `B-tag-${i}`;
  sent.push(rid);
  b.ws.send(JSON.stringify({ type: 'http-req', rid, method: 'GET', path: '/' }));
}
await Promise.race([waiter, sleep(8000)]);
check(got.size >= N, `B 并发 ${N} 请求全回包`, `got=${got.size}`);
let ridOk = 0, statusOk = 0;
for (const rid of sent) {
  const f = got.get(rid);
  if (!f) continue;
  if (f.rid === rid) ridOk++;
  if (f.status && f.status < 500) statusOk++;
}
check(ridOk === N, 'rid 逐条原样回（无串线/无改写）', `${ridOk}/${N}`);
check(statusOk === N, '回包状态健康（经桥→dsh web 全隧道）', `${statusOk}/${N}`);

// ---- 3. 吊销独立：吊销 B → B 被踢，A 不受影响 ----
const byePromise = new Promise((resolve) => {
  b.ws.on('message', (data) => {
    let f;
    try { f = JSON.parse(String(data)); } catch { return; }
    if (f.type === 'bye') resolve(f);
  });
  b.ws.on('close', () => resolve({ code: 'closed' }));
});
const rv = await api(`/api/devices/${devB.id}/revoke`, at, { method: 'POST' });
check(rv.code === 200, '吊销 B 成功');
const bye = await Promise.race([byePromise, sleep(5000)]);
check(bye && (bye.code === 'revoked' || bye.code === 'closed'), 'B 被踢（bye/closed）', JSON.stringify(bye).slice(0, 80));

// B 的令牌重连必须被拒（revoked）
const bre = await wsHello({ type: 'hello', role: 'client', deviceId: devB.id, token: devB.token, name: 'sim-phone-B' });
check(bre.frame.type === 'reject' && bre.frame.code === 'revoked', 'B 令牌重连被拒 revoked', JSON.stringify(bre.frame).slice(0, 80));
bre.ws.terminate();

const view3 = (await api('/api/rooms', at)).result.find((r) => r.room === ROOM);
check(view3.clientsOnline === 1, 'A 不受吊销影响仍在线', String(view3.clientsOnline));
check(view3.clients.some((c) => c.id !== devB.id && !c.revoked && c.online), '在线 client 是 A（非 B）');

b.ws.terminate();
console.log(`\n== ${pass} pass / ${fail} fail ==`);
process.exit(fail ? 1 : 0);
