// v3 隧道连续性协议（回放缓冲 + 批量帧）：
//   seq 注入 → batch 信封（上行/下行）→ 断线回放续传 → 环溢出 resume-reset
//   → 稳定 cid 跨重连 → host 闪断上行缓冲 → 非 batch 端不收信封
// 依赖：服务已启动（管理面 8788 / WS 8787），ADMIN_PASSWORD=test-admin-pw-123
// 可用 RELAY_HOST 切换来源 IP
import { WebSocket } from 'ws';

const HOST = process.env.RELAY_HOST || '127.0.0.1';
const ADMIN = `http://${HOST}:8788`;
const WS = `ws://${HOST}:8787`;
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

// 收集扁平隧道帧（batch 信封展开），直到集满 n 条
function collectFlat(ws, n, timeoutMs = 4000) {
  return new Promise((resolve, reject) => {
    const out = [];
    const timer = setTimeout(() => reject(new Error('collect timeout; got=' + out.length)), timeoutMs);
    const handler = (raw) => {
      const frame = JSON.parse(raw.toString());
      const inner = frame.type === 'batch'
        ? (frame.frames || []).map((s) => JSON.parse(s))
        : [frame];
      for (const f of inner) {
        if (f.type === 'ws-frame' || f.type === 'ws-open' || f.type === 'ws-close') out.push(f);
      }
      if (out.length >= n) { clearTimeout(timer); ws.off('message', handler); resolve(out); }
    };
    ws.on('message', handler);
  });
}

// 原始信封收集（判断是否以 batch 形式到达）
function collectEnvelopes(ws, n, timeoutMs = 4000) {
  return new Promise((resolve, reject) => {
    const out = [];
    const timer = setTimeout(() => reject(new Error('envelope timeout; got=' + out.length)), timeoutMs);
    const handler = (raw) => {
      out.push(JSON.parse(raw.toString()));
      const innerCount = out.reduce((a, f) => a + (f.type === 'batch' ? (f.frames || []).length : 1), 0);
      if (innerCount >= n) { clearTimeout(timer); ws.off('message', handler); resolve(out); }
    };
    ws.on('message', handler);
  });
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function main() {
  console.log('== 1. 配对入房（host + client 设备令牌） ==');
  const login = await api('/api/auth/login', null, { method: 'POST', body: { username: 'admin', password: 'test-admin-pw-123' } });
  const at = login.result?.accessToken;
  check('管理端登录', !!at);

  const env = await api('/api/rooms', at, { method: 'POST', body: { displayName: 'continuity-' + Date.now() } });
  const ROOM = env.result?.room;
  check('建房拿到 room', !!ROOM, JSON.stringify(env).slice(0, 120));

  const hp = await api('/api/pairing-codes', at, { method: 'POST', body: { role: 'host', name: 'cont-bridge', room: ROOM } });
  const cp = await api('/api/pairing-codes', at, { method: 'POST', body: { role: 'client', name: 'cont-phone', room: ROOM } });

  const host0 = await wsHello({ type: 'hello', role: 'host', pairingCode: hp.result?.code, name: 'cont-bridge', batch: true });
  check('host 配对入房', host0.frame.type === 'welcome' && host0.frame.batch === true, JSON.stringify(host0.frame).slice(0, 160));
  const hostCred = { deviceId: host0.frame.device?.id, token: host0.frame.device?.token };
  check('host 拿到设备令牌', !!hostCred.deviceId && !!hostCred.token);

  const c0 = await wsHello({ type: 'hello', role: 'client', pairingCode: cp.result?.code, name: 'cont-phone', batch: true, resumeFrom: 0 });
  check('client 配对入房', c0.frame.type === 'welcome', JSON.stringify(c0.frame).slice(0, 160));
  const cCred = { deviceId: c0.frame.device?.id, token: c0.frame.device?.token };
  const c0resume = await waitFor(c0.ws, (f) => f.type === 'resume');
  check('首连 resume{ok:true,count:0}', c0resume.ok === true && c0resume.count === 0, JSON.stringify(c0resume));

  console.log('== 2. seq 注入 + 稳定 cid ==');
  c0.ws.send(JSON.stringify({ type: 'ws-open', rid: 's1', path: '/api/remote.mux' }));
  const openF = await waitFor(host0.ws, (f) => f.type === 'ws-open');
  const CID = String(openF.rid).split('.')[0];
  check('上行 rid 带 cid 前缀', String(openF.rid) === CID + '.s1', JSON.stringify(openF));
  check('上行帧带 seq', Number.isInteger(openF.seq) && openF.seq >= 1, JSON.stringify(openF));

  host0.ws.send(JSON.stringify({ type: 'ws-frame', rid: CID + '.s1', text: 'back-1' }));
  const back1 = await waitFor(c0.ws, (f) => f.type === 'ws-frame');
  check('下行 rid 拆前缀还原', back1.rid === 's1' && back1.text === 'back-1', JSON.stringify(back1));
  check('下行帧带 seq', Number.isInteger(back1.seq) && back1.seq >= 1, JSON.stringify(back1));
  const lastDownSeq = back1.seq;

  console.log('== 3. batch 信封 ==');
  const batch3 = ['b1', 'b2', 'b3'].map((t) => JSON.stringify({ type: 'ws-frame', rid: CID + '.s1', text: t }));
  host0.ws.send(JSON.stringify({ type: 'batch', frames: batch3 }));
  const flat3 = await collectFlat(c0.ws, 3);
  check('下行 batch 三帧按序展开', flat3.map((f) => f.text).join(',') === 'b1,b2,b3', JSON.stringify(flat3.map((f) => f.text)));
  check('batch 内帧各带 seq', flat3.every((f) => Number.isInteger(f.seq)), JSON.stringify(flat3.map((f) => f.seq)));
  const lastDownSeq2 = flat3[2].seq;

  // 上行 batch：client → host
  const upBatch = ['u1', 'u2', 'u3'].map((t) => JSON.stringify({ type: 'ws-frame', rid: 's2', text: t }));
  c0.ws.send(JSON.stringify({ type: 'batch', frames: upBatch }));
  const upEnvs = await collectEnvelopes(host0.ws, 3);
  const upInner = upEnvs.flatMap((f) => (f.type === 'batch' ? f.frames.map((s) => JSON.parse(s)) : [f]));
  check('上行 batch 到 host 展开', upInner.filter((f) => f.type === 'ws-frame').slice(-3).map((f) => f.text).join(',') === 'u1,u2,u3',
    JSON.stringify(upInner.map((f) => f.text || f.type)));
  const upSeqs = upInner.filter((f) => f.type === 'ws-frame' && f.text?.startsWith('u')).map((f) => f.seq);
  check('上行 seq 连续递增', upSeqs.length === 3 && upSeqs[1] === upSeqs[0] + 1 && upSeqs[2] === upSeqs[1] + 1, JSON.stringify(upSeqs));
  const hostLastUpSeq = upSeqs[2];

  console.log('== 4. 断线回放续传（client 闪断） ==');
  c0.ws.terminate();
  await sleep(300);
  for (const t of ['r1', 'r2', 'r3']) {
    host0.ws.send(JSON.stringify({ type: 'ws-frame', rid: CID + '.s1', text: t }));
  }
  await sleep(300);
  const c1 = await wsHello({ type: 'hello', role: 'client', deviceId: cCred.deviceId, token: cCred.token, batch: true, resumeFrom: lastDownSeq2 });
  check('令牌重连 welcome', c1.frame.type === 'welcome', JSON.stringify(c1.frame).slice(0, 120));
  // 回放帧与 resume 同批到达：用 wsHello 自带的 seen 收集（后挂监听会漏同批事件）
  await sleep(400);
  const c1resume = c1.seen.find((f) => f.type === 'resume');
  check('resume{ok:true,count:3}', c1resume?.ok === true && c1resume.count === 3, JSON.stringify(c1resume));
  const replayed = c1.seen.filter((f) => f.type === 'ws-frame');
  check('回放三帧按序送达', replayed.map((f) => f.text).join(',') === 'r1,r2,r3', JSON.stringify(replayed.map((f) => f.text)));
  check('回放 seq 大于断点', replayed.every((f) => f.seq > lastDownSeq2), JSON.stringify(replayed.map((f) => f.seq)));

  console.log('== 5. 稳定 cid 跨重连 ==');
  host0.ws.send(JSON.stringify({ type: 'ws-frame', rid: CID + '.s1', text: 'stable-1' }));
  await sleep(400);
  const stable = c1.seen.find((f) => f.type === 'ws-frame' && f.text === 'stable-1');
  check('旧 rid 前缀仍路由到重连端', stable?.rid === 's1', JSON.stringify(stable));

  console.log('== 6. resume-reset（环溢出） ==');
  c1.ws.terminate();
  await sleep(300);
  for (let i = 0; i < 300; i++) {
    host0.ws.send(JSON.stringify({ type: 'ws-frame', rid: CID + '.s1', text: 'f' + i }));
  }
  await sleep(500);
  const c2 = await wsHello({ type: 'hello', role: 'client', deviceId: cCred.deviceId, token: cCred.token, batch: true, resumeFrom: 1 });
  await sleep(400);
  const c2resume = c2.seen.find((f) => f.type === 'resume');
  check('环溢出 → resume{ok:false}', c2resume?.ok === false, JSON.stringify(c2resume));

  console.log('== 7. host 闪断上行缓冲 ==');
  host0.ws.terminate();
  await sleep(300);
  for (const t of ['h1', 'h2']) {
    c2.ws.send(JSON.stringify({ type: 'ws-frame', rid: 's3', text: t }));
  }
  await sleep(300);
  const host1 = await wsHello({ type: 'hello', role: 'host', deviceId: hostCred.deviceId, token: hostCred.token, batch: true, resumeFrom: hostLastUpSeq });
  check('host 令牌重连', host1.frame.type === 'welcome', JSON.stringify(host1.frame).slice(0, 120));
  await sleep(400);
  const hResume = host1.seen.find((f) => f.type === 'resume');
  check('host 回放上行两帧', hResume?.ok === true && hResume.count === 2, JSON.stringify(hResume));
  const hFrames = host1.seen.filter((f) => f.type === 'ws-frame');
  check('上行两帧按序回放', hFrames.map((f) => f.text).join(',') === 'h1,h2', JSON.stringify(hFrames.map((f) => f.text)));

  console.log('== 8. 非 batch 端不收信封 ==');
  const c3 = await wsHello({ type: 'hello', role: 'client', deviceId: cCred.deviceId, token: cCred.token });
  // 同设备重连挤占 c2（superseded），属预期
  const plain = [];
  const plainDone = new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('plain timeout')), 4000);
    c3.ws.on('message', (raw) => {
      const f = JSON.parse(raw.toString());
      plain.push(f);
      if (f.type === 'ws-frame' && f.text === 'p3') { clearTimeout(timer); resolve(); }
    });
  });
  const pBatch = ['p1', 'p2', 'p3'].map((t) => JSON.stringify({ type: 'ws-frame', rid: CID + '.s1', text: t }));
  host1.ws.send(JSON.stringify({ type: 'batch', frames: pBatch }));
  await plainDone;
  check('未声明 batch → 逐帧投递', plain.filter((f) => f.type === 'ws-frame').map((f) => f.text).join(',') === 'p1,p2,p3'
    && !plain.some((f) => f.type === 'batch'), JSON.stringify(plain.map((f) => f.type)));

  console.log('== 9. 清理 ==');
  const del = await api('/api/rooms/' + ROOM, at, { method: 'DELETE' });
  check('删房（级联清缓冲）', del.code === 200, JSON.stringify(del));
  c3.ws.terminate(); host1.ws.terminate();

  console.log(`\n结果: ${pass} 通过 / ${fail} 失败`);
  process.exit(fail ? 1 : 0);
}

main().catch((e) => { console.error('FATAL', e); process.exit(1); });
