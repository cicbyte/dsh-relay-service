//! 配对码：签发（短时效/单次/失败熔断）→ 核销创建设备并签发令牌

use relay_common::util::{now_secs, rand_hex};
use relay_common::{AppError, AppState};
use relay_entity::pairing_code;
use sea_orm::{
    sea_query::Expr, ActiveModelTrait, ColumnTrait, EntityTrait, Insert, IntoActiveModel,
    QueryFilter, Set,
};
use serde::Serialize;

/// 配对码有效期（秒）
pub const PAIR_TTL_SECS: i64 = 600;
/// 核销失败熔断阈值
pub const PAIR_MAX_ATTEMPTS: i32 = 5;

/// 核销错误码（对应协议 reject code）
#[derive(Debug)]
pub enum PairErr {
    Invalid,
    Expired,
    Used,
    Burned,
    RoomMismatch,
}

impl PairErr {
    pub fn code(&self) -> &'static str {
        match self {
            PairErr::Invalid => "pairing-invalid",
            PairErr::Expired => "pairing-expired",
            PairErr::Used => "pairing-used",
            PairErr::Burned => "pairing-burned",
            PairErr::RoomMismatch => "room-mismatch",
        }
    }
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PairingView {
    pub code: String,
    pub role: String,
    pub name: String,
    /// 绑定的环境（room hex8；空串=未绑，核销时按 hello.code 派生）
    pub room: String,
    pub expires_at: i64,
}

/// 签发一次性配对码（XXXX-XXXX，易读字母表）。room 非空时出码即绑环境。
pub async fn issue(
    state: &AppState,
    role: &str,
    name: &str,
    room: &str,
) -> Result<PairingView, AppError> {
    if role != "host" && role != "client" {
        return Err(AppError::bad_request("role 只能是 host 或 client"));
    }
    if !room.is_empty() {
        // 环境必须存在（管理台环境视图出码）
        if relay_entity::room::Entity::find_by_id(room)
            .one(&state.db)
            .await?
            .is_none()
        {
            return Err(AppError::bad_request("环境不存在"));
        }
    }
    let code = new_pair_code();
    let now = now_secs();
    let row = pairing_code::ActiveModel {
        code: Set(code.clone()),
        role: Set(role.to_string()),
        name: Set(name.to_string()),
        room: Set(Some(room.to_string())),
        created_at: Set(now),
        expires_at: Set(now + PAIR_TTL_SECS),
        used_by: Set(None),
        attempts: Set(0),
    };
    // String 主键走 exec_without_returning（insert() 的回查对非自增主键不可靠）
    Insert::one(row).exec_without_returning(&state.db).await?;
    Ok(PairingView {
        code,
        role: role.to_string(),
        name: name.to_string(),
        room: room.to_string(),
        expires_at: now + PAIR_TTL_SECS,
    })
}

/// 核销配对码：创建设备 + 签发明文令牌（只此一次）
pub async fn redeem(
    state: &AppState,
    code: &str,
    role: &str,
    room: &str,
    name: &str,
) -> Result<(relay_entity::device::Model, String), PairErr> {
    let Ok(Some(row)) = pairing_code::Entity::find_by_id(code).one(&state.db).await else {
        return Err(PairErr::Invalid);
    };

    // 失败计数（先记后判，防枚举）
    let mut am = row.clone().into_active_model();
    am.attempts = Set(row.attempts + 1);
    let _ = am.update(&state.db).await;

    if row.attempts >= PAIR_MAX_ATTEMPTS {
        return Err(PairErr::Burned);
    }
    if row.used_by.is_some() {
        return Err(PairErr::Used);
    }
    if now_secs() > row.expires_at {
        return Err(PairErr::Expired);
    }
    if !row.role.is_empty() && row.role != role {
        return Err(PairErr::Invalid);
    }
    // 房间决议：出码绑了房间则以码为准（hello.code 只做寻址交叉校验）；
    // 未绑码（legacy）必须由 hello.code 派生
    let bound = row.room.clone().unwrap_or_default();
    let final_room = if !bound.is_empty() {
        if !room.is_empty() && bound != room {
            return Err(PairErr::RoomMismatch);
        }
        bound
    } else {
        if room.is_empty() {
            return Err(PairErr::Invalid);
        }
        room.to_string()
    };

    let token = crate::device::new_token();
    let dev_id = format!("dev_{}", rand_hex(6));
    let pending = format!("pending:{dev_id}");

    // 原子核销占位：条件 UPDATE ... WHERE used_by IS NULL——并发双花只有一个赢家
    // （此前 read-check-write 有窗口：两请求都能通过 used_by 检查、各建一台设备）
    let claim = pairing_code::Entity::update_many()
        .col_expr(pairing_code::Column::UsedBy, Expr::value(pending.clone()))
        .filter(pairing_code::Column::Code.eq(code))
        .filter(pairing_code::Column::UsedBy.is_null())
        .exec(&state.db)
        .await
        .map_err(|_| PairErr::Invalid)?;
    if claim.rows_affected == 0 {
        return Err(PairErr::Used);
    }

    let dev_name = if !name.is_empty() {
        name.to_string()
    } else if !row.name.is_empty() {
        row.name.clone()
    } else {
        "device".to_string()
    };
    let dev = match crate::device::create(state, &dev_id, &dev_name, role, &final_room, &token).await
    {
        Ok(d) => d,
        Err(_) => {
            // 设备创建失败：释放占位让码仍可核销（attempts 已计数，熔断不受影响）
            let _ = pairing_code::Entity::update_many()
                .col_expr(pairing_code::Column::UsedBy, Expr::value(None::<String>))
                .filter(pairing_code::Column::Code.eq(code))
                .filter(pairing_code::Column::UsedBy.eq(pending.clone()))
                .exec(&state.db)
                .await;
            return Err(PairErr::Invalid);
        }
    };
    // 占位转正为设备 id（WHERE 占位值，防释放窗口内被他人抢注）
    let _ = pairing_code::Entity::update_many()
        .col_expr(pairing_code::Column::UsedBy, Expr::value(dev_id.clone()))
        .filter(pairing_code::Column::Code.eq(code))
        .filter(pairing_code::Column::UsedBy.eq(pending))
        .exec(&state.db)
        .await;

    // 记录环境（展示名由管理台维护）
    let _ = crate::room::touch(state, &final_room).await;

    Ok((dev, token))
}

/// 易读配对码字母表（去 0/O/1/I 等易混字符）
const PAIR_ALPHABET: &[u8] = b"23456789ABCDEFGHJKMNPQRSTUVWXYZ";

fn new_pair_code() -> String {
    loop {
        let hex = rand_hex(4); // 8 hex chars → 8 个字母表字符
        let bytes = hex.as_bytes();
        let mut out = String::new();
        for (i, b) in bytes.iter().enumerate() {
            if i == 4 {
                out.push('-');
            }
            out.push(PAIR_ALPHABET[(*b as usize) % PAIR_ALPHABET.len()] as char);
        }
        return out;
    }
}
