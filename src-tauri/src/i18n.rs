//! 界面语言：运行时可在中文与英文之间切换，供所有面向用户的文案选择用词。
//! 前端通过 `set_language` 命令写入；默认中文。
//!
//! 用法：在需要显示文案的位置用 `crate::tr!("中文", "English")` 二选一，
//! 可带 `format!` 参数，例如 `crate::tr!("下载 {}", "Downloading {}", name)`。

use std::sync::atomic::{AtomicU8, Ordering};

static LANG: AtomicU8 = AtomicU8::new(0); // 0 = 中文，1 = English

/// 当前语言是否为英文。
pub fn is_en() -> bool {
    LANG.load(Ordering::Relaxed) == 1
}

/// 由前端语言标识（"zh" / "en"）设置当前语言；未知值回落中文。
pub fn set_lang(code: &str) {
    let v = if code.eq_ignore_ascii_case("en") { 1 } else { 0 };
    LANG.store(v, Ordering::Relaxed);
}

/// 按当前语言二选一文案：`tr!("中文", "English")`，可带 `format!` 参数。
#[macro_export]
macro_rules! tr {
    ($zh:literal, $en:literal $(, $arg:expr)* $(,)?) => {
        if $crate::i18n::is_en() {
            format!($en $(, $arg)*)
        } else {
            format!($zh $(, $arg)*)
        }
    };
}
