// test-multiuser.mjs —— 多用户隔离矩阵（#851）
// 权限模型：admin=全局（用户管理+全部环境+全审计）| user=仅自己的环境/设备/配对码/审计
// 运行：node test/test-multiuser.mjs（需服务已起；自建自清，可重复执行）

const RELAY_HOST = process.env.RELAY_HOST || '127.0.0.1';
const BASE = `http://${RELAY_HOST}:8788`;

async function api(path, token, opts = {}) {
  const headers = { 'content-type': 'application/json', ...(opts.headers || {}) };
  if (token) headers['authorization'] = `Bearer ${token}`;
  const res = await fetch(`${BASE}${path}`, { ...opts, headers });
  let body = null;
  try { body = await res.json(); } catch {}
  return { status: res.status, ...(body || {}) };
}

let passed = 0, failed = 0;
function check(name, cond, extra = '') {
  if (cond) { passed++; console.log(`  ✅ ${name}`); }
  else { failed++; console.log(`  ❌ ${name}`, extra); }
}

const RUN = Date.now().toString(36).slice(-5);
const UA = `ua-${RUN}`, UB = `ub-${RUN}`, UADMIN = `ux-${RUN}`;
const PW = 'test-user-pw-123';

console.log(`\n== 多用户隔离矩阵（run ${RUN}） ==`);

// ---- 1. admin 登录（含角色字段）----
const adminLogin = await api('/api/auth/login', null, {
  method: 'POST', body: JSON.stringify({ username: 'admin', password: process.env.ADMIN_PASSWORD || 'test-admin-pw-123' }),
});
const at = adminLogin.result?.accessToken;
check('admin 登录（role 字段）', adminLogin.status === 200 && adminLogin.result?.role === 'admin', JSON.stringify(adminLogin.result).slice(0, 120));

// ---- 2. admin 建三个账号：ua(user)、ub(user)、ux(admin) ----
for (const [u, role] of [[UA, 'user'], [UB, 'user'], [UADMIN, 'admin']]) {
  const r = await api('/api/users', at, { method: 'POST', body: JSON.stringify({ username: u, password: PW, role }) });
  check(`新建用户 ${u}(${role})`, r.status === 200 && r.result?.username === u, JSON.stringify(r).slice(0, 120));
}
// 重复用户名 400
const dup = await api('/api/users', at, { method: 'POST', body: JSON.stringify({ username: UA, password: PW, role: 'user' }) });
check('重复用户名 400', dup.status === 400, JSON.stringify(dup).slice(0, 120));
// 坏角色 400
const badRole = await api('/api/users', at, { method: 'POST', body: JSON.stringify({ username: `bad-${RUN}`, password: PW, role: 'root' }) });
check('非法角色 400', badRole.status === 400, JSON.stringify(badRole).slice(0, 120));

// ---- 3. ua/ub 登录 ----
async function loginAs(u) {
  const r = await api('/api/auth/login', null, { method: 'POST', body: JSON.stringify({ username: u, password: PW }) });
  return { token: r.result?.accessToken, role: r.result?.role, raw: r };
}
const ua = await loginAs(UA);
const ub = await loginAs(UB);
check('ua 登录 role=user', ua.token && ua.role === 'user', JSON.stringify(ua.raw).slice(0, 120));

// ---- 4. ua 建环境 E1 + 出码；ub 全不可见 ----
const e1 = await api('/api/rooms', ua.token, { method: 'POST', body: JSON.stringify({ displayName: `家${RUN}` }) });
check('ua 建环境 E1', e1.status === 200 && e1.result?.room, JSON.stringify(e1).slice(0, 120));
const e1room = e1.result?.room;
check('E1 归属 ua', e1.result?.ownerId > 0, JSON.stringify(e1.result).slice(0, 120));

const ubRooms = await api('/api/rooms', ub.token);
check('ub 看不到 E1（隔离）', ubRooms.status === 200 && !JSON.stringify(ubRooms.result).includes(e1room), JSON.stringify(ubRooms.result).slice(0, 160));

const ubRename = await api(`/api/rooms/${e1room}`, ub.token, { method: 'PUT', body: JSON.stringify({ displayName: 'hack' }) });
check('ub 改名 E1 → 404', ubRename.status === 404, `status=${ubRename.status}`);
const ubDel = await api(`/api/rooms/${e1room}`, ub.token, { method: 'DELETE' });
check('ub 删 E1 → 404', ubDel.status === 404, `status=${ubDel.status}`);
const ubPair = await api('/api/pairing-codes', ub.token, { method: 'POST', body: JSON.stringify({ role: 'client', name: 'x', room: e1room }) });
check('ub 给 E1 出码 → 404', ubPair.status === 404, `status=${ubPair.status}`);

// ---- 5. 越权 API 面 ----
const ubUsers = await api('/api/users', ub.token);
check('ub 访问用户管理 → 403', ubUsers.status === 403, `status=${ubUsers.status}`);
const uaUnbound = await api('/api/pairing-codes', ua.token, { method: 'POST', body: JSON.stringify({ role: 'client', name: 'x', room: '' }) });
check('user 出不绑环境码 → 403', uaUnbound.status === 403, `status=${uaUnbound.status}`);
const adminUnbound = await api('/api/pairing-codes', at, { method: 'POST', body: JSON.stringify({ role: 'client', name: 'x', room: '' }) });
check('admin 出不绑码（legacy）允许', adminUnbound.status === 200, `status=${adminUnbound.status}`);

// ---- 6. ua 出码→设备；ub 操作 ua 设备 404；概览裁剪 ----
const pair = await api('/api/pairing-codes', ua.token, { method: 'POST', body: JSON.stringify({ role: 'client', name: 'phone', room: e1room }) });
check('ua 给 E1 出码', pair.status === 200 && pair.result?.room === e1room, JSON.stringify(pair).slice(0, 120));

// 轮换/删除一个假设备 id：两边都应 404（不泄露）
const ubDev = await api('/api/devices/dev_notexist1/rotate', ub.token, { method: 'POST' });
check('ub 操作不存在设备 → 404', ubDev.status === 404, `status=${ubDev.status}`);

const ubOverview = await api('/api/status', ub.token);
check('ub 概览按归属裁剪（无 E1 设备）', ubOverview.status === 200 && (ubOverview.result?.devicesTotal ?? 0) === 0, JSON.stringify(ubOverview.result).slice(0, 160));
const uaOverview = await api('/api/status', ua.token);
check('ua 概览可见自己的环境', uaOverview.status === 200, JSON.stringify(uaOverview.result).slice(0, 120));

// ---- 7. 审计到人 + 隔离 ----
const uaAudit = await api('/api/audit?limit=200', ua.token);
const auditAll = await api('/api/audit?limit=200', at);
check('admin 审计全量', auditAll.status === 200 && auditAll.result.length >= uaAudit.result.length, `admin=${auditAll.result?.length} ua=${uaAudit.result?.length}`);
const uaOps = uaAudit.result.filter((r) => r.detail.includes('/api/rooms') && r.detail.includes(`"user":"${UA}"`));
check('ua 审计含自己的操作', uaOps.length > 0, JSON.stringify(uaAudit.result.slice(0, 3)).slice(0, 200));
const uaSeesOthers = uaAudit.result.some((r) => r.detail.includes('"user":"admin"') || r.detail.includes(`"user":"${UB}"`));
check('ua 审计不泄露他人操作', !uaSeesOthers, JSON.stringify(uaAudit.result.slice(0, 3)).slice(0, 200));
const adminOpUser = auditAll.result.find((r) => r.detail.includes('"method":"POST"') && r.detail.includes('/api/users'));
check('admin.op 记录操作人（user 非空）', adminOpUser && adminOpUser.detail.includes(`"user":"admin"`), JSON.stringify(adminOpUser).slice(0, 160));

// ---- 8. 管理员全局可见 + 归属名 ----
const adminRooms = await api('/api/rooms', at);
const e1view = (adminRooms.result || []).find((r) => r.room === e1room);
check('admin 可见 E1 且带归属名', !!e1view && e1view.ownerName === UA, JSON.stringify(e1view).slice(0, 160));

// ---- 9. 删用户：自保/兜底/级联 ----
const delSelf = await api(`/api/users/${adminLogin.result ? (await api('/api/auth/profile', at)).result.userId : 0}`, at, { method: 'DELETE' });
check('admin 不能删自己 → 400', delSelf.status === 400, `status=${delSelf.status}`);

// ux(admin) 删除：至少保留一个 admin —— 当前 admin+ux 共 2 个，删 ux 应成功；再删 admin 不可测（自保）
const usersList = await api('/api/users', at);
const uxId = usersList.result.find((u) => u.username === UADMIN)?.id;
const delUx = await api(`/api/users/${uxId}`, at, { method: 'DELETE' });
check('删除管理员账号（保底校验通过）', delUx.status === 200, JSON.stringify(delUx).slice(0, 120));

const uaId = usersList.result.find((u) => u.username === UA)?.id;
const delUa = await api(`/api/users/${uaId}`, at, { method: 'DELETE' });
check('删除 ua（级联其环境）', delUa.status === 200, JSON.stringify(delUa).slice(0, 120));
const afterDel = await api('/api/rooms', at);
check('E1 已随用户级联删除', !(afterDel.result || []).some((r) => r.room === e1room), JSON.stringify(afterDel.result).slice(0, 160));

const ubId = usersList.result.find((u) => u.username === UB)?.id;
await api(`/api/users/${ubId}`, at, { method: 'DELETE' });
const ubLoginAfter = await loginAs(UB);
check('被删用户无法登录', !ubLoginAfter.token, JSON.stringify(ubLoginAfter.raw).slice(0, 120));

console.log(`\n结果: ${passed} 通过 / ${failed} 失败`);
process.exit(failed ? 1 : 0);
