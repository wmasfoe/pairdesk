//! 会话码与密码生成。
//!
//! 行为来源：`app/src/pages/HostPage.tsx` 的 `genSid()` / `genPwd()`。
//!
//! 迁移到 GPUI 后这两段逻辑不再属于任何 UI 组件，但生成规则必须**逐字节保持
//! 一致**——用户会照着念、照着打字，格式变了就是体验倒退。因此这里用单测把
//! 原实现的约束全部钉死。

use rand::Rng;

/// 会话码前缀（协议无硬性要求，但用户已习惯这个形状）。
pub const SID_PREFIX: &str = "pd-";

/// 会话码随机段的字符个数（原实现 `slice(2, 6)` 取 4 个字符）。
pub const SID_RANDOM_LEN: usize = 4;

/// 密码取值下界（含）。
pub const PASSWORD_MIN: u32 = 100_000;

/// 密码取值上界（不含）。
pub const PASSWORD_MAX: u32 = 1_000_000;

/// 生成 6 位数字密码。
///
/// 原实现：`Math.floor(100000 + Math.random() * 900000).toString()`
///
/// 即取值域 `[100000, 999999]`，永远是 6 位、永远不含前导零（前导零会让用户
/// 分不清是 "012345" 还是 "12345"）。
pub fn gen_password() -> String {
    let mut rng = rand::thread_rng();
    let n: u32 = rng.gen_range(PASSWORD_MIN..PASSWORD_MAX);
    n.to_string()
}

/// 生成会话码，形如 `pd-AB12C34`。
///
/// 原实现：
/// ```js
/// 'pd-' + Math.random().toString(36).slice(2, 6).toUpperCase() + Math.floor(100 + Math.random() * 900)
/// ```
///
/// 结构 = 固定前缀 `pd-` + 4 位大写 base36 + 3 位数字。
pub fn gen_sid() -> String {
    let mut rng = rand::thread_rng();
    let random: String = (0..SID_RANDOM_LEN)
        .map(|_| {
            const ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
            ALPHABET[rng.gen_range(0..ALPHABET.len())] as char
        })
        .collect();
    let digits: u32 = rng.gen_range(100..1000);
    format!("{SID_PREFIX}{random}{digits}")
}

/// 一对可直接发给对方的凭据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialPair {
    /// 会话码，如 `pd-AB12C34`
    pub sid: String,
    /// 连接密码，6 位数字
    pub password: String,
}

impl CredentialPair {
    /// 随机生成一对新凭据。
    pub fn generate() -> Self {
        Self {
            sid: gen_sid(),
            password: gen_password(),
        }
    }

    /// 渲染成「复制会话信息」按钮写入剪贴板的文本。
    ///
    /// 原实现见 `HostPage.tsx::copyShare`，格式必须保持一致：
    /// 两行、全角冒号、无尾随换行。
    pub fn share_text(&self) -> String {
        format!("会话码：{}\n密码：{}", self.sid, self.password)
    }

    /// 是否具备发起被控端的必要条件（原 `HostPage` 的按钮 `disabled` 条件）。
    pub fn is_complete(&self) -> bool {
        !self.sid.is_empty() && !self.password.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn 密码恒为六位数字() {
        for _ in 0..500 {
            let p = gen_password();
            assert_eq!(p.len(), 6, "密码必须是 6 位，实际得到 {p:?}");
            assert!(
                p.chars().all(|c| c.is_ascii_digit()),
                "密码必须全是 ASCII 数字，实际得到 {p:?}"
            );
            assert!(
                !p.starts_with('0'),
                "密码不能以 0 开头（否则用户会念错位数），实际得到 {p:?}"
            );
        }
    }

    #[test]
    fn 密码落在原实现的值域内() {
        for _ in 0..500 {
            let n: u32 = gen_password().parse().expect("必须可解析为整数");
            assert!(
                (PASSWORD_MIN..PASSWORD_MAX).contains(&n),
                "密码 {n} 越界，应落在 [{PASSWORD_MIN}, {PASSWORD_MAX})"
            );
        }
    }

    #[test]
    fn 会话码形状与原实现一致() {
        for _ in 0..500 {
            let sid = gen_sid();
            assert!(sid.starts_with("pd-"), "会话码必须带 pd- 前缀，实际 {sid:?}");
            // "pd-" (3) + 4 位随机 + 3 位数字 = 10 个字符
            assert_eq!(sid.len(), 10, "会话码长度必须是 10，实际 {sid:?}");

            let rest = &sid[3..];
            let (alpha, digits) = rest.split_at(SID_RANDOM_LEN);
            assert_eq!(alpha.len(), 4);
            assert_eq!(digits.len(), 3);
            assert!(
                alpha.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()),
                "随机段必须是 base36 大写，实际 {alpha:?}"
            );
            assert!(
                digits.chars().all(|c| c.is_ascii_digit()),
                "尾段必须是 3 位数字，实际 {digits:?}"
            );
            // 尾段是 100..999，故首位不可能是 '0'
            assert!(!digits.starts_with('0'), "尾段不应以 0 开头，实际 {digits:?}");
        }
    }

    #[test]
    fn 生成的凭据具有足够随机性() {
        // 500 次生成中重复率必须极低（若随机源退化会立刻暴露）
        let sids: HashSet<String> = (0..500).map(|_| gen_sid()).collect();
        assert!(
            sids.len() > 480,
            "会话码重复过多：500 次只得到 {} 个不同值",
            sids.len()
        );

        let pwds: HashSet<String> = (0..500).map(|_| gen_password()).collect();
        assert!(
            pwds.len() > 480,
            "密码重复过多：500 次只得到 {} 个不同值",
            pwds.len()
        );
    }

    #[test]
    fn 复制文本格式与原实现逐字一致() {
        let pair = CredentialPair {
            sid: "pd-AB12C34".into(),
            password: "123456".into(),
        };
        // 全角冒号、两行、无尾随换行 —— 与 HostPage.tsx::copyShare 完全一致
        assert_eq!(pair.share_text(), "会话码：pd-AB12C34\n密码：123456");
        assert!(!pair.share_text().ends_with('\n'), "不应有尾随换行");
        assert_eq!(pair.share_text().lines().count(), 2);
    }

    #[test]
    fn 完整性判定与按钮禁用条件一致() {
        let ok = CredentialPair {
            sid: "pd-AB12C34".into(),
            password: "123456".into(),
        };
        assert!(ok.is_complete());

        let no_sid = CredentialPair {
            sid: String::new(),
            password: "123456".into(),
        };
        assert!(!no_sid.is_complete());

        let no_pwd = CredentialPair {
            sid: "pd-AB12C34".into(),
            password: String::new(),
        };
        assert!(!no_pwd.is_complete());

        // 随机生成的凭据必然完整（否则「开始接收协助」按钮永远点不了）
        assert!(CredentialPair::generate().is_complete());
    }
}
