use crate::manifest::{Kind, Manifest, MANIFEST_TOML, Source};

#[test]
fn embedded_manifest_parses_and_validates() {
    let m = Manifest::load().expect("内嵌清单应能解析并通过校验");
    assert_eq!(m.items.len(), 56, "清单应有 56 项");
}

#[test]
fn ids_are_unique() {
    let m = Manifest::load().unwrap();
    let mut ids: Vec<&str> = m.items.iter().map(|i| i.id.as_str()).collect();
    ids.sort_unstable();
    let n = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), n);
}

#[test]
fn categories_cover_expected_set() {
    let m = Manifest::load().unwrap();
    for cat in ["runtime", "build", "database", "web", "vcredist", "ide", "config"] {
        assert!(m.items.iter().any(|i| i.category == cat), "缺少分类 {cat}");
    }
}

#[test]
fn portable_items_have_archive() {
    let m = Manifest::load().unwrap();
    for it in m.items.iter().filter(|i| i.kind == Kind::Portable) {
        assert!(it.archive.is_some(), "{} 是免安装项但无 archive", it.id);
    }
}

#[test]
fn config_items_have_actions() {
    let m = Manifest::load().unwrap();
    for it in m.items.iter().filter(|i| i.source == Source::Config) {
        assert!(!it.actions.is_empty(), "{} 是配置项但无 actions", it.id);
    }
}

#[test]
fn duplicate_id_is_rejected() {
    let bad = r#"
[[item]]
id = "a"
name = "A"
category = "runtime"
source = "online"
kind = "installer"
provider = { kind = "node_dist", prefer = "lts" }

[[item]]
id = "a"
name = "A2"
category = "runtime"
source = "online"
kind = "installer"
provider = { kind = "node_dist", prefer = "lts" }
"#;
    let m: Manifest = toml::from_str(bad).unwrap();
    let err = m.validate_for_test().unwrap_err();
    assert!(err.to_string().contains("重复 id"));
}

#[test]
fn online_item_without_provider_is_rejected() {
    let bad = r#"
[[item]]
id = "x"
name = "X"
category = "runtime"
source = "online"
kind = "installer"
"#;
    let m: Manifest = toml::from_str(bad).unwrap();
    let err = m.validate_for_test().unwrap_err();
    assert!(err.to_string().contains("缺少 provider"));
}

#[test]
fn raw_manifest_is_not_empty() {
    assert!(MANIFEST_TOML.contains("[[item]]"));
}

#[test]
fn mirror_based_config_items_have_non_empty_mirrors() {
    let m = Manifest::load().unwrap();
    // 这些换源项必须带镜像清单，且每条都有名称与实际地址
    for id in [
        "rust-mirror", "cargo-mirror", "npm-mirror", "pip-mirror",
        "go-mirror", "nuget-mirror", "gem-mirror", "composer-mirror",
        "conda-mirror", "bun-mirror", "pnpm-mirror", "yarn-mirror",
        "maven-mirror", "gradle-mirror", "cran-mirror",
    ] {
        let it = m.get(id).unwrap_or_else(|| panic!("缺少 {id}"));
        assert!(!it.mirrors.is_empty(), "{id} 缺少镜像清单");
        for mi in &it.mirrors {
            assert!(!mi.name.trim().is_empty(), "{id} 有镜像缺名称");
            assert!(mi.url.starts_with("https://"), "{id} 镜像地址非 https: {}", mi.url);
        }
    }
}

#[test]
fn npm_mirror_count_is_four() {
    // 各生态覆盖度不同，npm 最少（只有 4 家），防止误加不存在的镜像
    let m = Manifest::load().unwrap();
    assert_eq!(m.get("npm-mirror").unwrap().mirrors.len(), 4);
}
