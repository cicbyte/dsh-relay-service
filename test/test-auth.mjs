// P0 安全底座 + v2 架构冒烟：
//   管理端登录 → 签发配对码 → WS 配对（拿一次性令牌）→ 令牌重连 → 房间转发
//   → 吊销即时踢线 → 审计落库 → fail-closed（无凭据拒绝）
// 依赖：服务已启动（127.0.0.1:8788 管理面 / 8787 WS），ADMIN_PASSWORD=test-admin-pw-123
import { WebSocket } from 'ws';

const ADMIN = 'http://127.0.0.1:8788';
const WS = 'ws://127.0.0.1:8787';
const ROOM_CODE = 'smoke-room-' + Date.now();
let pass = 0, fail = 0;

function check(name, cond, extra = '') {
  if (cond) { pass++; console.log(`  ✅ ${name}`); }
  else { fail++; console.log(`  ❌ ${name} ${extra}`); }
}

async function api(path, token, opts = {}) {
  const res = await fetch(ADMIN + path, {
    method: opts.method || 'GET',
    headers: { 'Content-Type': 'application/json', ...(token ? { Authorization: 'Bearer ' + token } : {}) },
    body: opts.body ? JSON.stringify(opts.body) : undefined,
  });
  return res.json();
}

function wsHello(hello, timeoutMs = 8000) {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(WS, 'dsh-relay-v1');
    const seen = [];
    const timer = setTimeout(() => { ws.terminate(); reject(new Error('timeout; seen=' + JSON.stringify(seen))); }, timeoutMs);
    ws.on('open', () => ws.send(JSON.stringify(hello)));
    ws.on('message', (raw) => {
      const frame = JSON.parse(raw.toString());
      seen.push(frame);
      if (frame.type === 'welcome' || frame.type === 'reject') {
        clearTimeout(timer);
        resolve({ frame, ws, seen });
      }
    });
    ws.on('error', (e) => { clearTimeout(timer); reject(e); });
  });
}

async function waitFor(ws, pred, timeoutMs = 4000) {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('frame timeout')), timeoutMs);
    const handler = (raw) => {
      const frame = JSON.parse(raw.toString());
      if (pred(frame)) { clearTimeout(timer); ws.off('message', handler); resolve(frame); }
    };
    ws.on('message', handler);
  });
}

async function main() {
  console.log('== 1. 管理端登录 ==');
  const bad = await api('/api/auth/login', null, { method: 'POST', body: { username: 'admin', password: 'wrong' } });
  check('错误密码 401', bad.code === 401, JSON.stringify(bad));
  const login = await api('/api/auth/login', null, { method: 'POST', body: { username: 'admin', password: 'test-admin-pw-123' } });
  check('登录成功发令牌', login.code === 200 && !!login.result?.access_token, JSON.stringify(login).slice(0, 120));
  const at = login.result?.access_token, rt = login.result?.refresh_token;

  const noAuth = await api('/api/devices');
  check('无令牌 401（fail-closed）', noAuth.code === 401, JSON.stringify(noAuth));

  const refreshed = await api('/api/auth/refresh', null, { method: 'POST', body: { refresh_token: rt } });
  check('refresh 轮转成功', refreshed.code === 200 && !!refreshed.result?.refresh_token);
  const oldRefresh = await api('/api/auth/refresh', null, { method: 'POST', body: { refresh_token: rt } });
  check('旧 refresh 立即失效', oldRefresh.code === 401, JSON.stringify(oldRefresh));

  console.log('== 2. 配对 + 令牌重连 ==');
  const pair = await api('/api/pairing-codes', at, { method: 'POST', body: { role: 'client', name: 'smoke-phone' } });
  check('签发配对码', pair.code === 200 && /^[0-9A-Z]{4}-[0-9A-Z]{4}$/.test(pair.result?.code), JSON.stringify(pair));
  const pairCode = pair.result?.code;

  const denied = await wsHello({ type: 'hello', role: 'client', code: ROOM_CODE });
  check('无凭据 hello 拒绝 auth-required', denied.frame.code === 'auth-required', JSON.stringify(denied.frame));
  denied.ws.terminate();

  const paired = await wsHello({ type: 'hello', role: 'client', code: ROOM_CODE, pairingCode: pairCode, name: 'smoke-phone' });
  check('配对 welcome(auth=pairing)', paired.frame.type === 'welcome' && paired.frame.auth === 'pairing', JSON.stringify(paired.frame));
  const dev = paired.frame.device;
  check('签发一次性设备令牌', dev?.id?.startsWith('dev_') && dev?.token?.startsWith('tok_'), JSON.stringify(dev));
  paired.ws.terminate();

  const reused = await wsHello({ type: 'hello', role: 'client', code: ROOM_CODE, pairingCode: pairCode });
  check('配对码单次使用', reused.frame.code === 'pairing-used', JSON.stringify(reused.frame));
  reused.ws.terminate();

  const tokenHello = await wsHello({ type: 'hello', role: 'client', code: ROOM_CODE, deviceId: dev.id, token: dev.token });
  check('令牌重连 welcome(auth=token)', tokenHello.frame.type === 'welcome' && tokenHello.frame.auth === 'token', JSON.stringify(tokenHello.frame));
  const client = tokenHello.ws;

  const badToken = await wsHello({ type: 'hello', role: 'client', code: ROOM_CODE, deviceId: dev.id, token: 'tok_deadbeef' });
  check('坏令牌拒绝 bad-token', badToken.frame.code === 'bad-token', JSON.stringify(badToken.frame));
  badToken.ws.terminate();

  console.log('== 3. 房间转发 ==');
  const hostPair = await api('/api/pairing-codes', at, { method: 'POST', body: { role: 'host', name: 'smoke-bridge' } });
  const hostJoined = await wsHello({ type: 'hello', role: 'host', code: ROOM_CODE, pairingCode: hostPair.result?.code });
  check('host 入房', hostJoined.frame.type === 'welcome' && hostJoined.frame.role === 'host', JSON.stringify(hostJoined.frame));
  const host = hostJoined.ws;

  const gotFrame = waitFor(client, (f) => f.type === 'http-req' && f.id === 'smoke1');
  host.send(JSON.stringify({ type: 'http-req', id: 'smoke1', method: 'GET', url: '/x' }));
  const relayed = await gotFrame;
  check('http-req 转发到 client', relayed.id === 'smoke1', JSON.stringify(relayed));

  console.log('== 4. 吊销即时踢线 ==');
  const kicked = waitFor(client, (f) => f.type === 'bye' || f.type === 'reject');
  const rv = await api(`/api/devices/${dev.id}/revoke`, at, { method: 'POST' });
  check('吊销接口 200', rv.code === 200, JSON.stringify(rv));
  const kickFrame = await kicked;
  check('在线设备被踢（bye revoked）', kickFrame.type === 'bye' && kickFrame.code === 'revoked', JSON.stringify(kickFrame));

  const revoked = await wsHello({ type: 'hello', role: 'client', code: ROOM_CODE, deviceId: dev.id, token: dev.token });
  check('吊销后令牌失效', revoked.frame.code === 'revoked', JSON.stringify(revoked.frame));
  revoked.ws.terminate(); host.terminate(); client.terminate();

  console.log('== 5. 管理面数据 ==');
  const devices = await api('/api/devices', at);
  const row = (devices.result || []).find((d) => d.id === dev.id);
  check('设备列表含吊销记录', devices.code === 200 && row?.revoked === true, JSON.stringify(row));

  const audit = await api('/api/audit?limit=100', at);
  const events = (audit.result || []).map((r) => r.event);
  check('审计含 conn.open/close', events.includes('conn.open') && events.includes('conn.close'), JSON.stringify(events.slice(0, 8)));
  check('审计含 admin.op（oplog）', events.includes('admin.op'), JSON.stringify(events.slice(0, 8)));

  const st = await api('/api/status', at);
  check('概览统计', st.code === 200 && typeof st.result?.devices_total === 'number', JSON.stringify(st.result));
  const health = await api('/api/health/detail');
  check('健康检查', health.code === 200 && health.result?.db === 'ok', JSON.stringify(health.result));

  console.log(`\n结果: ${pass} 通过 / ${fail} 失败`);
  process.exit(fail ? 1 : 0);
}

main().catch((e) => { console.error('冒烟异常:', e); process.exit(1); });
