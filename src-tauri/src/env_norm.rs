//! 路径规范化与 PATH 增删的纯逻辑，无 IO 副作用；
//! `expand_env` 依赖进程环境变量，其余为纯字符串函数。
//! 设计依据：设计文档第 11 节「写入前必须先检测」与规范化规则表。

/// 展开 `%VAR%` 形式的变量引用；未定义的变量原样保留。
pub fn expand_env(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '%' {
            if let Some(off) = chars[i + 1..].iter().position(|c| *c == '%') {
                let name: String = chars[i + 1..i + 1 + off].iter().collect();
                if !name.is_empty() {
                    if let Ok(v) = std::env::var(&name) {
                        out.push_str(&v);
                        i += off + 2;
                        continue;
                    }
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// 纯字符串规范化：trim → 正斜杠转反斜杠 → 去尾随反斜杠 → 转小写。
pub fn normalize_pure(s: &str) -> String {
    let t = s.trim().replace('/', "\\");
    t.trim_end_matches('\\').to_lowercase()
}

/// 完整规范化：先展开变量，再走纯字符串规范化。
pub fn normalize_entry(s: &str) -> String {
    normalize_pure(&expand_env(s))
}

/// PATH 值是否已包含目标目录（规范化后精确比较）。
pub fn path_contains(path_value: &str, target: &str) -> bool {
    let t = normalize_entry(target);
    if t.is_empty() {
        return false;
    }
    path_value
        .split(';')
        .any(|e| !e.trim().is_empty() && normalize_entry(e) == t)
}

/// 把 target 追加到 PATH 值末尾；已存在则返回 None（调用方据此跳过写入）。
pub fn path_append_value(path_value: &str, target: &str) -> Option<String> {
    if path_contains(path_value, target) {
        return None;
    }
    let trimmed = path_value.trim().trim_end_matches(';');
    if trimmed.is_empty() {
        Some(target.to_string())
    } else {
        Some(format!("{};{}", trimmed, target))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_handles_case_slash_trailing_and_space() {
        assert_eq!(normalize_pure("  C:\\FOO\\ "), "c:\\foo");
        assert_eq!(normalize_pure("C:/foo/"), "c:\\foo");
        assert_eq!(normalize_pure("c:\\foo"), "c:\\foo");
        assert_eq!(normalize_pure("C:\\Foo"), "c:\\foo");
    }

    #[test]
    fn normalize_mixed_slashes() {
        assert_eq!(normalize_pure("C:/Program Files/Git/cmd"), "c:\\program files\\git\\cmd");
    }

    #[test]
    fn expand_env_replaces_known_var() {
        std::env::set_var("DEV_TEST_VAR", "hello");
        assert_eq!(expand_env("%DEV_TEST_VAR%\\x"), "hello\\x");
    }

    #[test]
    fn expand_env_keeps_unknown_var() {
        assert_eq!(expand_env("%NO_SUCH_VAR_XYZ%\\x"), "%NO_SUCH_VAR_XYZ%\\x");
    }

    #[test]
    fn expand_env_edge_inputs_are_preserved() {
        assert_eq!(expand_env("%%"), "%%");
        assert_eq!(expand_env("C:\\foo%"), "C:\\foo%");
        assert_eq!(expand_env("%A%B"), "%A%B");
    }

    #[test]
    fn path_contains_ignores_writing_style() {
        let pv = "C:\\Windows;C:\\FOO\\;D:/bar";
        assert!(path_contains(pv, "c:\\foo"));
        assert!(path_contains(pv, "d:\\bar\\"));
        assert!(!path_contains(pv, "c:\\baz"));
    }

    #[test]
    fn path_contains_expands_vars() {
        std::env::set_var("DEV_TEST_ROOT", "C:\\Root");
        assert!(path_contains("%DEV_TEST_ROOT%\\bin;D:\\x", "C:\\Root\\bin"));
        assert!(path_contains("C:\\Root\\bin", "%DEV_TEST_ROOT%\\bin"));
    }

    #[test]
    fn path_contains_empty_target_is_false() {
        assert!(!path_contains("C:\\A", ""));
        assert!(!path_contains("C:\\A", "   "));
    }

    #[test]
    fn path_append_skips_when_present() {
        assert_eq!(path_append_value("C:\\Foo\\", "c:\\foo"), None);
        assert_eq!(path_append_value("A:\\x;C:/Foo/", "C:\\foo"), None);
    }

    #[test]
    fn path_append_adds_when_absent() {
        assert_eq!(path_append_value("C:\\A", "C:\\B"), Some("C:\\A;C:\\B".to_string()));
        assert_eq!(path_append_value("C:\\A;", "C:\\B"), Some("C:\\A;C:\\B".to_string()));
        assert_eq!(path_append_value("", "C:\\B"), Some("C:\\B".to_string()));
        assert_eq!(path_append_value("  ", "C:\\B"), Some("C:\\B".to_string()));
    }
}
