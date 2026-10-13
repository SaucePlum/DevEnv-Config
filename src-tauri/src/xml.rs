//! 浅层 XML 合并：为 Maven settings.xml、NuGet.Config 这类「根下容器 + 重复子块」结构
//! 提供「按 id 定位、存在则更新、不存在则追加」的合并能力，保留其余内容与注释。
//! 不支持深层嵌套路径；只用于这两类小型配置文件，大文件与复杂 XML 不适用。

use crate::error::{AppError, AppResult};
use std::path::Path;

/// 元素节点。属性值已反转义存储；子项区分元素 / 文本 / 注释等原样内容。
#[derive(Debug, Clone)]
pub struct Node {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Child>,
}

#[derive(Debug, Clone)]
pub enum Child {
    El(Node),
    /// 元素内的文本（已反转义）
    Txt(String),
    /// 注释等按原文保留的内容（含定界符）
    Raw(String),
}

impl Node {
    fn new(name: &str, attrs: Vec<(String, String)>) -> Self {
        Self { name: name.to_string(), attrs, children: Vec::new() }
    }

    /// 某直接子元素的文本（首个非空）。
    fn child_text(&self, name: &str) -> Option<String> {
        self.children.iter().find_map(|c| match c {
            Child::El(n) if n.name == name => n.children.iter().find_map(|t| match t {
                Child::Txt(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
                _ => None,
            }),
            _ => None,
        })
    }
}

fn attrs_of(e: &quick_xml::events::BytesStart) -> Vec<(String, String)> {
    e.attributes()
        .flatten()
        .map(|a| {
            let raw = String::from_utf8_lossy(&a.value).to_string();
            let val = quick_xml::escape::unescape(&raw)
                .map(|v| v.to_string())
                .unwrap_or(raw);
            (String::from_utf8_lossy(a.key.as_ref()).to_string(), val)
        })
        .collect()
}

/// 解析 XML 为（声明原文, 根节点）。声明缺失时返回 None。
pub fn parse(text: &str) -> Result<(Option<String>, Node), String> {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_str(text);
    let mut decl: Option<String> = None;
    let mut stack: Vec<Node> = Vec::new();
    let mut root: Option<Node> = None;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let node = Node::new(&String::from_utf8_lossy(e.name().as_ref()), attrs_of(&e));
                if stack.is_empty() {
                    stack.push(node);
                } else {
                    // 占位入栈：End 时原位写回，保持子项顺序
                    let top = stack.last_mut().unwrap();
                    top.children.push(Child::El(node));
                    let taken = match top.children.last_mut() {
                        Some(Child::El(n)) => std::mem::replace(n, Node::new("", vec![])),
                        _ => unreachable!("刚压入的必为元素"),
                    };
                    stack.push(taken);
                }
            }
            Ok(Event::Empty(e)) => {
                let node = Node::new(&String::from_utf8_lossy(e.name().as_ref()), attrs_of(&e));
                match stack.last_mut() {
                    Some(top) => top.children.push(Child::El(node)),
                    None => root = Some(node),
                }
            }
            Ok(Event::End(_)) => {
                let done = stack.pop().ok_or_else(|| crate::tr!("多余的结束标签", "Unexpected closing tag"))?;
                match stack.last_mut() {
                    Some(top) => {
                        match top.children.last_mut() {
                            // 写回占位槽（Start 时留下的空壳）
                            Some(Child::El(n)) if n.name.is_empty() => *n = done,
                            _ => top.children.push(Child::El(done)),
                        }
                    }
                    None => root = Some(done),
                }
            }
            Ok(Event::Text(t)) => {
                let s = String::from_utf8_lossy(&t).to_string();
                if let Some(top) = stack.last_mut() {
                    top.children.push(Child::Txt(s));
                }
            }
            Ok(Event::Comment(c)) => {
                if let Some(top) = stack.last_mut() {
                    top.children.push(Child::Raw(format!("<!--{}-->", String::from_utf8_lossy(&c))));
                }
            }
            Ok(Event::Decl(d)) => {
                decl = Some(format!("<?{}?>", String::from_utf8_lossy(&d)));
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    match root {
        Some(r) => Ok((decl, r)),
        None => Err(crate::tr!("没有找到根元素", "No root element found")),
    }
}

fn escape(s: &str) -> String {
    quick_xml::escape::escape(s).to_string()
}

fn serialize(node: &Node, indent: usize, out: &mut String) {
    let pad = "  ".repeat(indent);
    let attrs = node
        .attrs
        .iter()
        .map(|(k, v)| format!(" {k}=\"{}\"", escape(v)))
        .collect::<String>();
    // 叶子纯文本节点输出为单行
    if node.children.len() == 1 {
        if let Child::Txt(t) = &node.children[0] {
            out.push_str(&format!(
                "{pad}<{}{attrs}>{}</{}>\n",
                node.name,
                escape(t.trim()),
                node.name
            ));
            return;
        }
    }
    // 空元素输出自闭合形态（保留 <proxies/> 风格）
    if node.children.iter().all(|c| matches!(c, Child::Txt(t) if t.trim().is_empty())) {
        out.push_str(&format!("{pad}<{}{attrs}/>\n", node.name));
        return;
    }
    out.push_str(&format!("{pad}<{}{attrs}>\n", node.name));
    for c in &node.children {
        match c {
            Child::El(n) => serialize(n, indent + 1, out),
            Child::Txt(t) if !t.trim().is_empty() => {
                out.push_str(&format!("{}{}\n", "  ".repeat(indent + 1), escape(t.trim())))
            }
            Child::Raw(r) => out.push_str(&format!("{}{}\n", "  ".repeat(indent + 1), r.trim())),
            _ => {}
        }
    }
    out.push_str(&format!("{pad}</{}>\n", node.name));
}

fn set_child_text(node: &mut Node, name: &str, value: &str) {
    for c in node.children.iter_mut() {
        if let Child::El(n) = c {
            if n.name == name {
                n.children = vec![Child::Txt(value.to_string())];
                return;
            }
        }
    }
    let mut n = Node::new(name, vec![]);
    n.children.push(Child::Txt(value.to_string()));
    node.children.push(Child::El(n));
}

/// 合并结果字符串：None 表示无需写回。
pub fn merge(
    text: Option<&str>,
    root_name: &str,
    container: &str,
    child: &str,
    id_tag: &str,
    id_value: &str,
    fields: &[(String, String)],
) -> Result<String, String> {
    let (decl, mut root) = match text {
        Some(t) if !t.trim().is_empty() => {
            let (d, r) = parse(t)?;
            if r.name != root_name {
                return Err(crate::tr!("根元素是 <{0}>，期望 <{1}>", "Root element is <{0}>, expected <{1}>", r.name, root_name));
            }
            (d, r)
        }
        _ => (
            Some("<?xml version=\"1.0\" encoding=\"UTF-8\"?>".into()),
            Node::new(root_name, vec![]),
        ),
    };

    // 找到（或创建）根下 container
    let idx = root.children.iter().position(|c| matches!(c, Child::El(n) if n.name == container));
    let cont = match idx {
        Some(i) => match &mut root.children[i] {
            Child::El(n) => n,
            _ => unreachable!(),
        },
        None => {
            root.children.push(Child::El(Node::new(container, vec![])));
            match root.children.last_mut().unwrap() {
                Child::El(n) => n,
                _ => unreachable!(),
            }
        }
    };

    // 定位同 id 子块：有则更新 fields，无则追加新块
    let mut found = false;
    for c in cont.children.iter_mut() {
        if let Child::El(n) = c {
            if n.name == child && n.child_text(id_tag).as_deref() == Some(id_value) {
                for (k, v) in fields {
                    set_child_text(n, k, v);
                }
                found = true;
                break;
            }
        }
    }
    if !found {
        let mut block = Node::new(child, vec![]);
        set_child_text(&mut block, id_tag, id_value);
        for (k, v) in fields {
            set_child_text(&mut block, k, v);
        }
        cont.children.push(Child::El(block));
    }

    let mut out = String::new();
    if let Some(d) = &decl {
        out.push_str(d);
        out.push('\n');
    }
    serialize(&root, 0, &mut out);
    Ok(out)
}

/// 对外入口：合并到文件（存在则备份为 `<文件名>.bak`，缺失则创建）。
/// 返回是否发生了写回（内容已一致时为 false）。
pub fn merge_into_file(
    path: &Path,
    root_name: &str,
    container: &str,
    child: &str,
    id_tag: &str,
    id_value: &str,
    fields: &[(String, String)],
) -> AppResult<bool> {
    let original = if path.exists() {
        let mut backup_name = path.as_os_str().to_owned();
        backup_name.push(".bak");
        std::fs::copy(path, Path::new(&backup_name))?;
        Some(std::fs::read_to_string(path)?)
    } else {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        None
    };
    let next = merge(original.as_deref(), root_name, container, child, id_tag, id_value, fields)
        .map_err(|e| AppError::Config(crate::tr!("{0} 解析失败: {1}", "Failed to parse {0}: {1}", path.display(), e)))?;
    let changed = original.as_deref() != Some(next.as_str());
    if changed {
        std::fs::write(path, next)?;
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SETTINGS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<settings>
  <!-- 阿里云镜像 -->
  <mirrors>
    <mirror>
      <id>aliyunmaven</id>
      <mirrorOf>*</mirrorOf>
      <url>https://maven.aliyun.com/repository/public</url>
    </mirror>
  </mirrors>
</settings>"#;

    fn fields() -> Vec<(String, String)> {
        vec![
            ("mirrorOf".into(), "central".into()),
            ("url".into(), "https://mirrors.tencent.com/nexus/repository/maven-public/".into()),
        ]
    }

    #[test]
    fn appends_mirror_and_keeps_existing() {
        let out =
            merge(Some(SETTINGS), "settings", "mirrors", "mirror", "id", "devenv-mirror", &fields()).unwrap();
        assert!(out.contains("aliyunmaven"), "既有 mirror 应保留");
        assert!(out.contains("devenv-mirror"));
        assert!(out.contains("<!-- 阿里云镜像 -->"), "注释应保留");
        assert!(out.contains("central"));
        // 二次合并不重复
        let out2 = merge(Some(&out), "settings", "mirrors", "mirror", "id", "devenv-mirror", &fields()).unwrap();
        assert_eq!(out2.matches("devenv-mirror").count(), 1);
    }

    #[test]
    fn updates_existing_mirror_with_same_id() {
        let mine = r#"<settings><mirrors>
          <mirror><id>devenv-mirror</id><mirrorOf>*</mirrorOf><url>https://old.example.com/maven</url></mirror>
        </mirrors></settings>"#;
        let out = merge(Some(mine), "settings", "mirrors", "mirror", "id", "devenv-mirror", &fields()).unwrap();
        assert!(!out.contains("old.example.com"), "同 id 的旧 url 应被更新");
        assert_eq!(out.matches("devenv-mirror").count(), 1);
    }

    #[test]
    fn creates_container_when_missing() {
        let out =
            merge(Some("<settings></settings>"), "settings", "mirrors", "mirror", "id", "devenv-mirror", &fields())
                .unwrap();
        assert!(out.contains("<mirrors>"));
        assert!(out.contains("devenv-mirror"));
    }

    #[test]
    fn creates_file_shape_from_empty() {
        let out = merge(None, "settings", "mirrors", "mirror", "id", "devenv-mirror", &fields()).unwrap();
        assert!(out.starts_with("<?xml"));
        assert!(out.contains("<settings>"));
        assert!(out.contains("devenv-mirror"));
    }

    #[test]
    fn self_closing_and_escaped_values_roundtrip() {
        let src = r#"<settings><proxies/></settings>"#;
        let out = merge(Some(src), "settings", "mirrors", "mirror", "id", "m", &fields()).unwrap();
        assert!(out.contains("<proxies/>"), "自闭合元素应保留：{out}");
        assert!(out.contains("<mirrors>"));
    }

    #[test]
    fn wrong_root_is_rejected() {
        let r = merge(Some("<project></project>"), "settings", "mirrors", "mirror", "id", "x", &fields());
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("根元素"));
    }

    #[test]
    fn malformed_xml_is_rejected() {
        assert!(merge(Some("<settings><mirrors>"), "settings", "mirrors", "mirror", "id", "x", &fields()).is_err());
    }

    #[test]
    fn merge_into_file_roundtrip() {
        let dir = std::env::temp_dir().join("devenv_xml_test");
        let _ = std::fs::create_dir_all(&dir);
        let f = dir.join("settings.xml");
        std::fs::write(&f, SETTINGS).unwrap();
        let changed =
            merge_into_file(&f, "settings", "mirrors", "mirror", "id", "devenv-mirror", &fields()).unwrap();
        assert!(changed);
        let text = std::fs::read_to_string(&f).unwrap();
        assert!(text.contains("devenv-mirror"));
        let mut backup_name = f.as_os_str().to_owned();
        backup_name.push(".bak");
        assert!(Path::new(&backup_name).exists(), "应留有备份");
        // 二次执行内容一致 → 不再写回
        let changed2 =
            merge_into_file(&f, "settings", "mirrors", "mirror", "id", "devenv-mirror", &fields()).unwrap();
        assert!(!changed2);
    }
}
