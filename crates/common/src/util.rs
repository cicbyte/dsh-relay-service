//! 通用小工具：随机数 / 时间

/// 随机十六进制串（getrandom；失败兜底为时间熵，几乎不可达）
pub fn rand_hex(nbytes: usize) -> String {
    let mut buf = vec![0u8; nbytes];
    if getrandom::getrandom(&mut buf).is_err() {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(7);
        for (i, b) in buf.iter_mut().enumerate() {
            *b = t.wrapping_add(i as u32) as u8;
        }
    }
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

/// 当前 Unix 秒
pub fn now_secs() -> i64 {
    chrono::Utc::now().timestamp()
}
