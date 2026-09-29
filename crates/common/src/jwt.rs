use chrono::Utc;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use crate::AppError;

pub const AUD_ADMIN: &str = "admin";
pub const TYP_ACCESS: &str = "access";
pub const TYP_REFRESH: &str = "refresh";

/// JWT 载荷（对齐 byte-admin）：sub=管理员ID、sid=会话ID、aud=受众、
/// typ=access/refresh、nonce=刷新令牌轮转随机数
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Claims {
    pub sub: i64,
    pub sid: String,
    pub aud: String,
    pub typ: String,
    pub nonce: String,
    pub exp: i64,
    pub iat: i64,
}

#[derive(Clone)]
pub struct JwtManager {
    encoding: EncodingKey,
    decoding: DecodingKey,
    pub access_ttl_secs: i64,
    pub refresh_ttl_secs: i64,
}

impl JwtManager {
    pub fn new(secret: &str, access_ttl_secs: i64, refresh_ttl_secs: i64) -> Self {
        Self {
            encoding: EncodingKey::from_secret(secret.as_bytes()),
            decoding: DecodingKey::from_secret(secret.as_bytes()),
            access_ttl_secs,
            refresh_ttl_secs,
        }
    }

    fn sign(
        &self,
        sub: i64,
        sid: &str,
        aud: &str,
        typ: &str,
        nonce: &str,
        ttl: i64,
    ) -> Result<String, AppError> {
        let now = Utc::now().timestamp();
        let claims = Claims {
            sub,
            sid: sid.to_string(),
            aud: aud.to_string(),
            typ: typ.to_string(),
            nonce: nonce.to_string(),
            exp: now + ttl,
            iat: now,
        };
        encode(&Header::default(), &claims, &self.encoding)
            .map_err(|e| AppError::internal(format!("令牌签发失败: {e}")))
    }

    pub fn sign_access(&self, sub: i64, sid: &str, aud: &str) -> Result<String, AppError> {
        self.sign(sub, sid, aud, TYP_ACCESS, "", self.access_ttl_secs)
    }

    pub fn sign_refresh(
        &self,
        sub: i64,
        sid: &str,
        aud: &str,
        nonce: &str,
    ) -> Result<String, AppError> {
        self.sign(sub, sid, aud, TYP_REFRESH, nonce, self.refresh_ttl_secs)
    }

    pub fn verify(&self, token: &str, aud: &str, typ: &str) -> Result<Claims, AppError> {
        let mut validation = Validation::new(jsonwebtoken::Algorithm::HS256);
        validation.set_audience(&[aud]);
        validation.leeway = 30;
        let data = decode::<Claims>(token, &self.decoding, &validation)
            .map_err(|_| AppError::unauthorized("登录已过期"))?;
        if data.claims.typ != typ {
            return Err(AppError::unauthorized("令牌类型错误"));
        }
        Ok(data.claims)
    }
}

/// 提取 `Authorization: Bearer <token>` 中的凭证
pub fn extract_token(headers: &axum::http::HeaderMap) -> Option<String> {
    let value = headers.get("authorization")?.to_str().ok()?.trim();
    value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))
        .map(|t| t.trim().to_string())
}
