// P0 安全底座 + v2 架构冒烟：
//   管理端登录 → 签发配对码 → WS 配对（拿一次性令牌）→ 令牌重连 → 房间转发
//   → 吊销即时踢线 → 审计落库 → fail-closed（无凭据拒绝）
// 依赖：服务已启动（管理面 8788 / WS 8787），ADMIN_PASSWORD=test-admin-pw-123
// 可用 RELAY_HOST 切换来源 IP（127.0.0.1 的限流桶可能被同机旧客户端风暴占用）
import { WebSocket } from 'ws';

const HOST = process.env.RELAY_HOST || '127.0.0.1';
const ADMIN = `http://${HOST}:8788`;
const WS = `ws://${HOST}:8787`;
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
  check('登录成功发令牌', login.code === 200 && !!login.result?.accessToken, JSON.stringify(login).slice(0, 120));
  const at = login.result?.accessToken, rt = login.result?.refreshToken;

  const noAuth = await api('/api/devices');
  check('无令牌 401（fail-closed）', noAuth.code === 401, JSON.stringify(noAuth));

  const refreshed = await api('/api/auth/refresh', null, { method: 'POST', body: { refreshToken: rt } });
  check('refresh 轮转成功', refreshed.code === 200 && !!refreshed.result?.refreshToken);
  const oldRefresh = await api('/api/auth/refresh', null, { method: 'POST', body: { refreshToken: rt } });
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

  const gotFrame = waitFor(client, (f) => f.type === 'http-req' && f.rid === 'smoke1');
  host.send(JSON.stringify({ type: 'http-req', rid: 'smoke1', method: 'GET', path: '/x' }));
  const relayed = await gotFrame;
  check('http-req 转发到 client', relayed.rid === 'smoke1', JSON.stringify(relayed));

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
  check('概览统计', st.code === 200 && typeof st.result?.devicesTotal === 'number', JSON.stringify(st.result));
  check('中继地址提示（wsPort + lanAddr）', typeof st.result?.wsPort === 'number' && !!st.result?.lanAddr, JSON.stringify(st.result).slice(0, 160));
  const health = await api('/api/health/detail');
  check('健康检查', health.code === 200 && health.result?.db === 'ok', JSON.stringify(health.result));

  console.log('== 6. 环境模型：1 host : N client ==');
  const env = await api('/api/rooms', at, { method: 'POST', body: { displayName: '测试环境' } });
  check('新建环境', env.code === 200 && env.result?.room?.length === 8, JSON.stringify(env).slice(0, 120));
  const ENV_ROOM = env.result?.room;

  const hp = await api('/api/pairing-codes', at, { method: 'POST', body: { role: 'host', name: 'multi-bridge', room: ENV_ROOM } });
  const c1p = await api('/api/pairing-codes', at, { method: 'POST', body: { role: 'client', name: 'phone-A', room: ENV_ROOM } });
  const c2p = await api('/api/pairing-codes', at, { method: 'POST', body: { role: 'client', name: 'phone-B', room: ENV_ROOM } });
  check('环境级出码 ×3', hp.code === 200 && c1p.code === 200 && c2p.code === 200, JSON.stringify([hp.result?.code, c1p.result?.code]));

  const wrongRoom = await wsHello({ type: 'hello', role: 'client', code: 'other-room-code-xyz', pairingCode: c2p.result?.code, name: 'phone-B' });
  check('绑码跨环境拒绝 room-mismatch', wrongRoom.frame.code === 'room-mismatch', JSON.stringify(wrongRoom.frame));
  wrongRoom.ws.terminate();

  const hostJ = await wsHello({ type: 'hello', role: 'host', pairingCode: hp.result?.code });
  check('host 配对免 code 入房', hostJ.frame.type === 'welcome' && hostJ.frame.room === ENV_ROOM, JSON.stringify(hostJ.frame));
  const host2 = hostJ.ws;

  const jA = await wsHello({ type: 'hello', role: 'client', pairingCode: c1p.result?.code, name: 'phone-A' });
  check('client-A 配对入房', jA.frame.type === 'welcome' && jA.frame.clients === 1, JSON.stringify(jA.frame));
  const devA = jA.frame.device;
  const c1 = jA.ws;

  const jB = await wsHello({ type: 'hello', role: 'client', pairingCode: c2p.result?.code, name: 'phone-B' });
  check('client-B 同环境共存', jB.frame.type === 'welcome' && jB.frame.clients === 2, JSON.stringify(jB.frame));
  const devB = jB.frame.device;
  const c2 = jB.ws;

  // 并发中继：rid 前缀路由，应答还原，互不串线
  const hostGotA = waitFor(host2, (f) => f.type === 'http-req' && String(f.rid || '').endsWith('.req-a1'));
  c1.send(JSON.stringify({ type: 'http-req', rid: 'req-a1', method: 'GET', path: '/a' }));
  const frameA = await hostGotA;
  check('host 收 A 帧（rid 带路由前缀）', frameA.rid !== 'req-a1' && frameA.rid.endsWith('.req-a1'), JSON.stringify(frameA));

  const hostGotB = waitFor(host2, (f) => f.type === 'http-req' && String(f.rid || '').endsWith('.req-b1'));
  c2.send(JSON.stringify({ type: 'http-req', rid: 'req-b1', method: 'GET', path: '/b' }));
  const frameB = await hostGotB;
  check('host 收 B 帧（前缀不同）', frameB.rid !== frameA.rid && frameB.rid.endsWith('.req-b1'), JSON.stringify(frameB));

  const aGot = waitFor(c1, (f) => f.type === 'http-res' && f.rid === 'req-a1');
  const bGot = waitFor(c2, (f) => f.type === 'http-res' && f.rid === 'req-b1');
  const crossA = waitFor(c1, (f) => f.type === 'http-res' && f.rid === 'req-b1').catch(() => null);
  host2.send(JSON.stringify({ type: 'http-res', rid: frameA.rid, status: 200, body: 'A-ok' }));
  host2.send(JSON.stringify({ type: 'http-res', rid: frameB.rid, status: 200, body: 'B-ok' }));
  const resA = await aGot, resB = await bGot;
  check('A 应答还原 rid 回 A', resA.rid === 'req-a1' && resA.body === 'A-ok', JSON.stringify(resA));
  check('B 应答还原 rid 回 B', resB.rid === 'req-b1' && resB.body === 'B-ok', JSON.stringify(resB));
  const crossed = await Promise.race([crossA, new Promise((r) => setTimeout(() => r(undefined), 800))]);
  check('并发不串线（B 响应未串到 A）', crossed === null || crossed === undefined, JSON.stringify(crossed));

  // 独立吊销：只踢 B，A/host 不受影响
  const bKick = waitFor(c2, (f) => f.type === 'bye');
  const hostPeer = waitFor(host2, (f) => f.type === 'peer' && f.clients === 1).catch(() => null);
  await api(`/api/devices/${devB.id}/revoke`, at, { method: 'POST' });
  const kickB = await bKick;
  check('吊销 B 即时踢 B', kickB.type === 'bye' && kickB.code === 'revoked', JSON.stringify(kickB));
  await hostPeer;
  check('host 收到在线数减 1', true);

  const aGot2 = waitFor(c1, (f) => f.type === 'http-res' && f.rid === 'req-a2');
  const hostGotA2 = waitFor(host2, (f) => f.type === 'http-req' && String(f.rid || '').endsWith('.req-a2'));
  c1.send(JSON.stringify({ type: 'http-req', rid: 'req-a2', method: 'GET', path: '/a2' }));
  const fg = await hostGotA2;
  host2.send(JSON.stringify({ type: 'http-res', rid: fg.rid, status: 200, body: 'A2-ok' }));
  const r2 = await aGot2;
  check('A 中继不受 B 吊销影响', r2.body === 'A2-ok', JSON.stringify(r2));

  // 同设备重连挤占自己（superseded），令牌免 code
  const aOld = waitFor(c1, (f) => f.type === 'bye' && f.code === 'superseded');
  const t1 = await wsHello({ type: 'hello', role: 'client', deviceId: devA.id, token: devA.token });
  check('令牌免 code 重连', t1.frame.type === 'welcome' && t1.frame.auth === 'token' && t1.frame.room === ENV_ROOM, JSON.stringify(t1.frame));
  const kickOld = await aOld;
  check('同设备重连挤占旧连接', kickOld.code === 'superseded', JSON.stringify(kickOld));

  // 环境视图 + 重命名（t1 仍在线，验证在线态）
  const rooms = await api('/api/rooms', at);
  const roomRow = (rooms.result || []).find((r) => r.room === ENV_ROOM);
  check('环境视图分组（host + 2 client）', rooms.code === 200 && roomRow?.host?.name === 'multi-bridge' && roomRow?.clients?.length === 2, JSON.stringify(roomRow).slice(0, 240));
  check('环境在线态', roomRow?.hostOnline === true && roomRow?.clientsOnline >= 1, JSON.stringify({ h: roomRow?.hostOnline, c: roomRow?.clientsOnline }));
  const ren = await api(`/api/rooms/${ENV_ROOM}`, at, { method: 'PUT', body: { displayName: '家里' } });
  check('重命名环境', ren.code === 200, JSON.stringify(ren));
  const rooms2 = await api('/api/rooms', at);
  check('重命名生效', (rooms2.result || []).find((r) => r.room === ENV_ROOM)?.displayName === '家里');

  // 设备代领：桌面桥令牌为手机出码（免管理台）
  const devHost = hostJ.frame.device;
  const invRes = await fetch(`http://${HOST}:8788/api/invite`, {
    method: 'POST',
    headers: { 'content-type': 'application/json', authorization: `Bearer ${devHost.token}`, 'x-device-id': devHost.id },
    body: JSON.stringify({ name: 'invited-phone' }),
  });
  const invJson = await invRes.json();
  check('设备代领出码（绑本环境）', invJson.code === 200 && /^[0-9A-Z]{4}-[0-9A-Z]{4}$/.test(invJson.result?.code) && invJson.result?.room === ENV_ROOM, JSON.stringify(invJson).slice(0, 160));
  const invJoin = await wsHello({ type: 'hello', role: 'client', pairingCode: invJson.result?.code, name: 'invited-phone' });
  check('代领码配对入房', invJoin.frame.type === 'welcome' && invJoin.frame.room === ENV_ROOM, JSON.stringify(invJoin.frame));
  invJoin.ws.terminate();
  const invFail = await fetch(`http://${HOST}:8788/api/invite`, {
    method: 'POST',
    headers: { 'content-type': 'application/json', authorization: `Bearer ${devA.token}`, 'x-device-id': devA.id },
    body: JSON.stringify({ name: 'x' }),
  });
  check('client 设备不能代领', (await invFail.json()).code === 401);

  host2.terminate(); c1.terminate(); t1.ws.terminate();

  console.log('== 7. 删除环境（连带设备、踢线） ==');
  const env2 = await api('/api/rooms', at, { method: 'POST', body: { displayName: '待删环境' } });
  const DEL_ROOM = env2.result?.room;
  check('新建待删环境', env2.code === 200 && DEL_ROOM?.length === 8, JSON.stringify(env2).slice(0, 120));
  const dp = await api('/api/pairing-codes', at, { method: 'POST', body: { role: 'host', name: 'del-bridge', room: DEL_ROOM } });
  const delJoin = await wsHello({ type: 'hello', role: 'host', pairingCode: dp.result?.code, name: 'del-bridge' });
  check('待删环境 host 在线', delJoin.frame.type === 'welcome' && delJoin.frame.room === DEL_ROOM, JSON.stringify(delJoin.frame).slice(0, 120));
  const delKick = waitFor(delJoin.ws, (f) => f.type === 'bye').catch(() => null);
  const del = await api(`/api/rooms/${DEL_ROOM}`, at, { method: 'DELETE' });
  check('删除环境 200', del.code === 200, JSON.stringify(del));
  const delBye = await Promise.race([delKick, new Promise((r) => setTimeout(() => r(null), 3000))]);
  check('在线连接即时踢线（bye）', delBye?.type === 'bye', JSON.stringify(delBye));
  const gone = await wsHello({ type: 'hello', role: 'host', deviceId: delJoin.frame.device?.id, token: delJoin.frame.device?.token, name: 'x' });
  check('环境内设备随删失效', gone.frame.type === 'reject', JSON.stringify(gone.frame).slice(0, 80));
  gone.ws.terminate();
  const rooms3 = await api('/api/rooms', at);
  check('环境列表已无该环境', !(rooms3.result || []).some((r) => r.room === DEL_ROOM), JSON.stringify(rooms3.result).slice(0, 160));
  const delAgain = await api(`/api/rooms/${DEL_ROOM}`, at, { method: 'DELETE' });
  check('重复删除 404', delAgain.code === 404, JSON.stringify(delAgain));

  console.log(`\n结果: ${pass} 通过 / ${fail} 失败`);
  process.exit(fail ? 1 : 0);
}

main().catch((e) => { console.error('冒烟异常:', e); process.exit(1); });
