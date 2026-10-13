//! 安装 / 配置完成后的自检：起新进程执行验证命令，全部退出码 0 视为通过。

use crate::manifest::Item;
use crate::proc;
use crate::state::Params;
use serde::Serialize;
use std::time::Duration;

#[derive(Debug, Clone, Serialize)]
pub struct VerifyOutcome {
    pub ok: bool,
    pub code: i32,
    pub output: String,
    pub command: String,
}

/// 把 `{占位符}` 应用到验证命令上。命令首元素是可执行文件（可能是绝对路径）。
pub fn commands_for(item: &Item, params: &Params) -> Vec<Vec<String>> {
    item.verify
        .iter()
        .map(|cmd| cmd.iter().map(|s| params.fill(s)).collect())
        .collect()
}

pub fn run_verify(item: &Item, params: &Params, extra_dirs: &[std::path::PathBuf]) -> VerifyOutcome {
    let cmds = commands_for(item, params);
    if cmds.is_empty() {
        return VerifyOutcome { ok: true, code: 0, output: "无验证命令".into(), command: String::new() };
    }
    let mut last = VerifyOutcome { ok: true, code: 0, output: String::new(), command: String::new() };
    for cmd in cmds {
        let (program, args) = match cmd.split_first() {
            Some(x) => x,
            None => continue,
        };
        let shown = cmd.join(" ");
        match proc::run_program(program, args, extra_dirs, Duration::from_secs(30)) {
            Ok(out) if out.code == 0 => {
                last = VerifyOutcome { ok: true, code: 0, output: out.combined(), command: shown };
            }
            Ok(out) => {
                return VerifyOutcome { ok: false, code: out.code, output: out.combined(), command: shown };
            }
            Err(e) => {
                return VerifyOutcome { ok: false, code: -1, output: e.to_string(), command: shown };
            }
        }
    }
    last
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::Manifest;

    #[test]
    fn placeholders_are_filled() {
        let m = Manifest::load().unwrap();
        let it = m.get("redis").unwrap();
        let params = Params { install_dir: Some("D:\\Tools\\redis".into()), ..Default::default() };
        let cmds = commands_for(it, &params);
        assert!(cmds[0][0].contains("D:\\Tools\\redis"));
        assert!(cmds[0][0].ends_with("redis-cli.exe"));
    }

    #[test]
    fn no_commands_counts_as_ok() {
        let m = Manifest::load().unwrap();
        let it = m.get("node").unwrap();
        let out = run_verify(it, &Params::default(), &[]);
        assert!(out.ok);
    }

    #[test]
    fn failing_command_is_reported() {
        let mut it = Manifest::load().unwrap().get("node").unwrap().clone();
        it.verify = vec![vec!["cmd".into(), "/C".into(), "exit 3".into()]];
        let out = run_verify(&it, &Params::default(), &[]);
        assert!(!out.ok);
        assert_eq!(out.code, 3);
    }
}
