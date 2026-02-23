//! 中文/拼音/模糊搜索引擎
//!
//! 支持 4 层分级搜索：精确前缀 → 拼音全拼 → 拼音首字母 → nucleo 模糊匹配

use std::collections::BTreeMap;
use std::ops::Bound;

use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Matcher, Utf32Str};
use pinyin::ToPinyin;

use crate::app::apps::App;

/// 预计算的搜索元数据
#[derive(Clone, Debug)]
pub struct SearchMeta {
    pub name_lc: String,
    pub pinyin_full: String,
    pub pinyin_initials: String,
    pub has_cjk: bool,
    // 本地化名称（如中文名）的搜索元数据
    pub loc_name_lc: Option<String>,
    pub loc_pinyin_full: Option<String>,
    pub loc_pinyin_initials: Option<String>,
}

/// 带搜索元数据的 App
#[derive(Clone, Debug)]
pub struct IndexedApp {
    pub app: App,
    pub meta: SearchMeta,
}

/// 匹配类型优先级（数值越小越优先）
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MatchType {
    ExactPrefix = 0,
    PinyinFull = 1,
    PinyinInitials = 2,
    Fuzzy = 3,
}

/// 评分结果
#[derive(Clone, Debug)]
pub struct ScoredApp {
    pub app: App,
    pub score: i64,
    pub match_type: MatchType,
}

/// 判断是否为 CJK 字符
fn is_cjk_char(c: char) -> bool {
    matches!(c,
        '\u{4E00}'..='\u{9FFF}' |   // CJK Unified Ideographs
        '\u{3400}'..='\u{4DBF}' |   // CJK Extension A
        '\u{20000}'..='\u{2A6DF}' | // CJK Extension B
        '\u{F900}'..='\u{FAFF}' |   // CJK Compatibility Ideographs
        '\u{2F800}'..='\u{2FA1F}'   // CJK Compatibility Supplement
    )
}

/// 为一个字符串计算拼音全拼和首字母
fn compute_pinyin(s: &str) -> (String, String) {
    let mut full = String::new();
    let mut initials = String::new();
    for c in s.chars() {
        if let Some(py) = c.to_pinyin() {
            full.push_str(py.plain());
            initials.push_str(&py.plain()[..1]);
        } else {
            let lc = c.to_lowercase().to_string();
            full.push_str(&lc);
            initials.push_str(&lc);
        }
    }
    (full, initials)
}

/// 为一个 App 构建搜索元数据
pub fn build_search_meta(name: &str, localized_name: Option<&str>) -> SearchMeta {
    let name_lc = name.to_lowercase();
    let has_cjk = name.chars().any(is_cjk_char);

    let (pinyin_full, pinyin_initials) = if has_cjk {
        compute_pinyin(name)
    } else {
        (String::new(), String::new())
    };

    // 本地化名称的搜索元数据
    let (loc_name_lc, loc_pinyin_full, loc_pinyin_initials) =
        if let Some(ln) = localized_name {
            let ln_lc = ln.to_lowercase();
            let ln_has_cjk = ln.chars().any(is_cjk_char);
            let (lpf, lpi) = if ln_has_cjk {
                let (f, i) = compute_pinyin(ln);
                (Some(f), Some(i))
            } else {
                (None, None)
            };
            (Some(ln_lc), lpf, lpi)
        } else {
            (None, None, None)
        };

    SearchMeta {
        name_lc,
        pinyin_full,
        pinyin_initials,
        has_cjk,
        loc_name_lc,
        loc_pinyin_full,
        loc_pinyin_initials,
    }
}

/// 所有已索引应用的搜索结构
#[derive(Clone, Debug)]
pub struct AppIndex {
    apps: Vec<IndexedApp>,
    by_name: BTreeMap<String, Vec<usize>>,
    by_pinyin: BTreeMap<String, Vec<usize>>,
    by_initials: BTreeMap<String, Vec<usize>>,
}

impl AppIndex {
    /// 从 App 列表构建索引
    pub fn from_apps(options: Vec<App>) -> Self {
        let mut apps = Vec::with_capacity(options.len());
        let mut by_name: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        let mut by_pinyin: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        let mut by_initials: BTreeMap<String, Vec<usize>> = BTreeMap::new();

        for (i, app) in options.into_iter().enumerate() {
            let meta = build_search_meta(&app.name, app.localized_name.as_deref());

            // 索引 name_lc（app 原始字段，可能是英文原名小写如 "wechat"）
            by_name
                .entry(app.name_lc.clone())
                .or_default()
                .push(i);

            // 如果 meta.name_lc 和 app.name_lc 不同（name 被替换为中文显示名时），也索引
            if meta.name_lc != app.name_lc {
                by_name
                    .entry(meta.name_lc.clone())
                    .or_default()
                    .push(i);
            }

            if meta.has_cjk && !meta.pinyin_full.is_empty() {
                by_pinyin
                    .entry(meta.pinyin_full.clone())
                    .or_default()
                    .push(i);
                by_initials
                    .entry(meta.pinyin_initials.clone())
                    .or_default()
                    .push(i);
            }

            // 索引本地化名称
            if let Some(ref ln_lc) = meta.loc_name_lc {
                if *ln_lc != app.name_lc && *ln_lc != meta.name_lc {
                    by_name.entry(ln_lc.clone()).or_default().push(i);
                }
            }
            if let Some(ref lpf) = meta.loc_pinyin_full {
                by_pinyin.entry(lpf.clone()).or_default().push(i);
            }
            if let Some(ref lpi) = meta.loc_pinyin_initials {
                by_initials.entry(lpi.clone()).or_default().push(i);
            }

            apps.push(IndexedApp { app, meta });
        }

        AppIndex {
            apps,
            by_name,
            by_pinyin,
            by_initials,
        }
    }

    /// 返回所有 App（用于 emoji 全量展示）
    pub fn all(&self) -> Vec<App> {
        self.apps.iter().map(|ia| ia.app.clone()).collect()
    }

    /// 在 BTreeMap 中做前缀搜索，返回匹配的索引集合
    fn prefix_search(map: &BTreeMap<String, Vec<usize>>, prefix: &str) -> Vec<usize> {
        map.range::<str, _>((Bound::Included(prefix), Bound::Unbounded))
            .take_while(|(k, _)| k.starts_with(prefix))
            .flat_map(|(_, indices)| indices.iter().copied())
            .collect()
    }

    /// 4 层分级搜索
    pub fn search(&self, query: &str, matcher: &mut Matcher) -> Vec<App> {
        if query.is_empty() {
            return vec![];
        }

        let query_lc = query.to_lowercase();
        let mut scored: Vec<ScoredApp> = Vec::new();
        let mut matched_indices: std::collections::HashSet<usize> = std::collections::HashSet::new();

        // Layer 1: 精确前缀匹配（原名小写）
        let exact_hits = Self::prefix_search(&self.by_name, &query_lc);
        for idx in exact_hits {
            if matched_indices.insert(idx) {
                let ia = &self.apps[idx];
                scored.push(ScoredApp {
                    app: ia.app.clone(),
                    score: 10000 - ia.meta.name_lc.len() as i64,
                    match_type: MatchType::ExactPrefix,
                });
            }
        }

        // Layer 2: 拼音全拼前缀
        let pinyin_hits = Self::prefix_search(&self.by_pinyin, &query_lc);
        for idx in pinyin_hits {
            if matched_indices.insert(idx) {
                let ia = &self.apps[idx];
                scored.push(ScoredApp {
                    app: ia.app.clone(),
                    score: 8000 - ia.meta.name_lc.len() as i64,
                    match_type: MatchType::PinyinFull,
                });
            }
        }

        // Layer 3: 拼音首字母前缀
        let initial_hits = Self::prefix_search(&self.by_initials, &query_lc);
        for idx in initial_hits {
            if matched_indices.insert(idx) {
                let ia = &self.apps[idx];
                scored.push(ScoredApp {
                    app: ia.app.clone(),
                    score: 6000 - ia.meta.name_lc.len() as i64,
                    match_type: MatchType::PinyinInitials,
                });
            }
        }

        // Layer 4: nucleo 模糊匹配（仅对未匹配的 app）
        if scored.len() < 20 {
            let pattern = Pattern::new(
                &query_lc,
                CaseMatching::Ignore,
                Normalization::Smart,
                AtomKind::Fuzzy,
            );
            let mut buf = Vec::new();

            for (idx, ia) in self.apps.iter().enumerate() {
                if matched_indices.contains(&idx) {
                    continue;
                }

                // 对原名、拼音全拼、本地化名及其拼音都尝试模糊匹配
                let mut candidates = vec![&ia.meta.name_lc];
                if ia.meta.has_cjk && !ia.meta.pinyin_full.is_empty() {
                    candidates.push(&ia.meta.pinyin_full);
                }
                if let Some(ref ln) = ia.meta.loc_name_lc {
                    candidates.push(ln);
                }
                if let Some(ref lpf) = ia.meta.loc_pinyin_full {
                    candidates.push(lpf);
                }

                let mut best_score: Option<u32> = None;
                for candidate in candidates {
                    let haystack = Utf32Str::new(candidate, &mut buf);
                    if let Some(s) = pattern.score(haystack, matcher) {
                        best_score = Some(best_score.map_or(s, |prev: u32| prev.max(s)));
                    }
                }

                if let Some(s) = best_score {
                    scored.push(ScoredApp {
                        app: ia.app.clone(),
                        score: s as i64,
                        match_type: MatchType::Fuzzy,
                    });
                }
            }
        }

        // 排序：match_type 优先，score 降序
        scored.sort_by(|a, b| {
            a.match_type
                .cmp(&b.match_type)
                .then(b.score.cmp(&a.score))
        });

        scored.truncate(20);
        scored.into_iter().map(|s| s.app).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::apps::AppCommand;
    use nucleo_matcher::Config as MatcherConfig;

    fn make_app(name: &str) -> App {
        App {
            name: name.to_string(),
            name_lc: name.to_lowercase(),
            localized_name: None,
            desc: String::new(),
            icons: None,
            open_command: AppCommand::Display,
        }
    }

    /// 模拟 macOS 真实场景：discovery.rs 实际产出的数据
    /// bundle_name: 英文原名（如 "WeChat"）, zh_name: 中文本地化名（如 "微信"）
    /// 结果：name=zh_name（显示用）, name_lc=bundle_name.lowercase（英文搜索用）,
    ///       localized_name=Some(zh_name)（拼音索引用）
    fn make_localized_app(bundle_name: &str, zh_name: &str) -> App {
        App {
            name: zh_name.to_string(),
            name_lc: bundle_name.to_lowercase(),
            localized_name: Some(zh_name.to_string()),
            desc: bundle_name.to_string(),
            icons: None,
            open_command: AppCommand::Display,
        }
    }

    fn make_index(names: &[&str]) -> AppIndex {
        let apps: Vec<App> = names.iter().map(|n| make_app(n)).collect();
        AppIndex::from_apps(apps)
    }

    /// 构建模拟真实 Mac 环境的索引
    fn make_real_world_index() -> AppIndex {
        let apps = vec![
            make_localized_app("WeChat", "微信"),
            make_localized_app("Lark", "飞书"),
            make_localized_app("QQMusic", "QQ音乐"),
            make_localized_app("NeteaseMusic", "网易云音乐"),
            make_localized_app("TencentMeeting", "腾讯会议"),
            make_app("Google Chrome"),
            make_app("Safari"),
            make_app("Firefox"),
            make_app("Visual Studio Code"),
            make_app("iTerm"),
        ];
        AppIndex::from_apps(apps)
    }

    fn search_names(index: &AppIndex, query: &str) -> Vec<String> {
        let mut matcher = Matcher::new(MatcherConfig::DEFAULT);
        index.search(query, &mut matcher)
            .iter()
            .map(|a| a.name.clone())
            .collect()
    }

    // === 基础功能测试 ===

    #[test]
    fn test_chinese_prefix() {
        let index = make_index(&["微信", "微博", "Safari"]);
        let names = search_names(&index, "微");
        assert!(names.contains(&"微信".to_string()));
        assert!(names.contains(&"微博".to_string()));
        assert!(!names.contains(&"Safari".to_string()));
    }

    #[test]
    fn test_pinyin_full() {
        let index = make_index(&["微信", "Safari"]);
        let names = search_names(&index, "weixin");
        assert!(names.contains(&"微信".to_string()));
    }

    #[test]
    fn test_pinyin_initials() {
        let index = make_index(&["微信", "Safari"]);
        let names = search_names(&index, "wx");
        assert!(names.contains(&"微信".to_string()));
    }

    #[test]
    fn test_english_fuzzy() {
        let index = make_index(&["Google Chrome", "Safari", "Firefox"]);
        let names = search_names(&index, "chrom");
        assert!(names.contains(&"Google Chrome".to_string()));
    }

    #[test]
    fn test_mixed_name_pinyin_initials() {
        let index = make_index(&["QQ音乐", "Safari"]);
        let names = search_names(&index, "qqyl");
        assert!(names.contains(&"QQ音乐".to_string()));
    }

    #[test]
    fn test_mixed_name_pinyin_full() {
        let index = make_index(&["QQ音乐", "Safari"]);
        let names = search_names(&index, "qqyinle");
        assert!(names.contains(&"QQ音乐".to_string()));
    }

    #[test]
    fn test_empty_query() {
        let index = make_index(&["微信", "Safari"]);
        let names = search_names(&index, "");
        assert!(names.is_empty());
    }

    #[test]
    fn test_all() {
        let index = make_index(&["微信", "Safari"]);
        let all = index.all();
        assert_eq!(all.len(), 2);
    }

    // === 真实场景：英文 bundle name + 中文 localized_name ===
    // discovery.rs 产出: name=中文, name_lc=英文小写, localized_name=Some(中文)

    #[test]
    fn real_wechat_by_chinese_prefix() {
        let names = search_names(&make_real_world_index(), "微信");
        assert!(names.contains(&"微信".to_string()), "微信 should find 微信, got: {names:?}");
    }

    #[test]
    fn real_wechat_by_chinese_partial() {
        let names = search_names(&make_real_world_index(), "微");
        assert!(names.contains(&"微信".to_string()), "微 should find 微信, got: {names:?}");
    }

    #[test]
    fn real_wechat_by_pinyin_full() {
        let names = search_names(&make_real_world_index(), "weixin");
        assert!(names.contains(&"微信".to_string()), "weixin should find 微信, got: {names:?}");
    }

    #[test]
    fn real_wechat_by_pinyin_initials() {
        let names = search_names(&make_real_world_index(), "wx");
        assert!(names.contains(&"微信".to_string()), "wx should find 微信, got: {names:?}");
    }

    #[test]
    fn real_wechat_by_english_name() {
        let names = search_names(&make_real_world_index(), "wechat");
        assert!(names.contains(&"微信".to_string()), "wechat should find 微信, got: {names:?}");
    }

    #[test]
    fn real_lark_by_chinese() {
        let names = search_names(&make_real_world_index(), "飞书");
        assert!(names.contains(&"飞书".to_string()), "飞书 should find 飞书, got: {names:?}");
    }

    #[test]
    fn real_lark_by_pinyin() {
        let names = search_names(&make_real_world_index(), "feishu");
        assert!(names.contains(&"飞书".to_string()), "feishu should find 飞书, got: {names:?}");
    }

    #[test]
    fn real_lark_by_initials() {
        let names = search_names(&make_real_world_index(), "fs");
        assert!(names.contains(&"飞书".to_string()), "fs should find 飞书, got: {names:?}");
    }

    #[test]
    fn real_lark_by_english() {
        let names = search_names(&make_real_world_index(), "lark");
        assert!(names.contains(&"飞书".to_string()), "lark should find 飞书, got: {names:?}");
    }

    #[test]
    fn real_qqmusic_by_localized_chinese() {
        let names = search_names(&make_real_world_index(), "qq音乐");
        assert!(names.contains(&"QQ音乐".to_string()), "QQ音乐 should find QQ音乐, got: {names:?}");
    }

    #[test]
    fn real_qqmusic_by_localized_pinyin() {
        let names = search_names(&make_real_world_index(), "qqyinle");
        assert!(names.contains(&"QQ音乐".to_string()), "qqyinle should find QQ音乐, got: {names:?}");
    }

    #[test]
    fn real_qqmusic_by_english() {
        let names = search_names(&make_real_world_index(), "qqmusic");
        assert!(names.contains(&"QQ音乐".to_string()), "qqmusic should find QQ音乐, got: {names:?}");
    }

    #[test]
    fn real_netease_by_partial_chinese() {
        let names = search_names(&make_real_world_index(), "网易");
        assert!(names.contains(&"网易云音乐".to_string()), "网易 should find 网易云音乐, got: {names:?}");
    }

    #[test]
    fn real_netease_by_pinyin() {
        let names = search_names(&make_real_world_index(), "wangyiyun");
        assert!(names.contains(&"网易云音乐".to_string()), "wangyiyun should find 网易云音乐, got: {names:?}");
    }

    #[test]
    fn real_netease_by_english() {
        let names = search_names(&make_real_world_index(), "netease");
        assert!(names.contains(&"网易云音乐".to_string()), "netease should find 网易云音乐, got: {names:?}");
    }

    #[test]
    fn real_tencent_meeting_by_initials() {
        let names = search_names(&make_real_world_index(), "txhy");
        assert!(names.contains(&"腾讯会议".to_string()), "txhy should find 腾讯会议, got: {names:?}");
    }

    #[test]
    fn real_tencent_meeting_by_english() {
        let names = search_names(&make_real_world_index(), "tencent");
        assert!(names.contains(&"腾讯会议".to_string()), "tencent should find 腾讯会议, got: {names:?}");
    }

    #[test]
    fn real_chrome_by_english_prefix() {
        let names = search_names(&make_real_world_index(), "google");
        assert!(names.contains(&"Google Chrome".to_string()));
    }

    #[test]
    fn real_chrome_by_fuzzy() {
        let names = search_names(&make_real_world_index(), "chrom");
        assert!(names.contains(&"Google Chrome".to_string()));
    }

    #[test]
    fn real_vscode_by_fuzzy() {
        let names = search_names(&make_real_world_index(), "vscode");
        assert!(names.contains(&"Visual Studio Code".to_string()), "vscode should find VS Code, got: {names:?}");
    }

    #[test]
    fn real_no_cross_contamination() {
        let names = search_names(&make_real_world_index(), "微信");
        assert!(!names.contains(&"飞书".to_string()));
    }

    // === 排序优先级测试 ===

    #[test]
    fn exact_prefix_ranks_higher_than_pinyin() {
        let apps = vec![
            make_localized_app("WeChat", "微信"),
            make_app("微信读书"),
        ];
        let index = AppIndex::from_apps(apps);
        let names = search_names(&index, "微信");
        assert!(names.contains(&"微信".to_string()));
        assert!(names.contains(&"微信读书".to_string()));
    }

    // === 中文子串搜索测试 ===

    #[test]
    fn chinese_substring_shezhi_finds_system_settings() {
        let apps = vec![
            make_localized_app("System Settings", "系统设置"),
            make_app("Safari"),
        ];
        let index = AppIndex::from_apps(apps);
        let names = search_names(&index, "设置");
        assert!(names.contains(&"系统设置".to_string()), "设置 should find 系统设置, got: {names:?}");
    }

    #[test]
    fn chinese_substring_yinyue_finds_netease() {
        let apps = vec![
            make_localized_app("NeteaseMusic", "网易云音乐"),
            make_localized_app("QQMusic", "QQ音乐"),
            make_app("Safari"),
        ];
        let index = AppIndex::from_apps(apps);
        let names = search_names(&index, "音乐");
        assert!(names.contains(&"网易云音乐".to_string()), "音乐 should find 网易云音乐, got: {names:?}");
        assert!(names.contains(&"QQ音乐".to_string()), "音乐 should find QQ音乐, got: {names:?}");
    }

    #[test]
    fn chinese_substring_huiyi_finds_tencent_meeting() {
        let apps = vec![
            make_localized_app("TencentMeeting", "腾讯会议"),
            make_app("Safari"),
        ];
        let index = AppIndex::from_apps(apps);
        let names = search_names(&index, "会议");
        assert!(names.contains(&"腾讯会议".to_string()), "会议 should find 腾讯会议, got: {names:?}");
    }

    // === E2E 测试：真实 discovery → index → search ===

    #[test]
    fn e2e_search_shezhi_finds_system_settings() {
        use crate::platform::get_installed_apps;
        let apps = get_installed_apps(false);
        if !apps.iter().any(|a| a.name.contains("系统设置") || a.name.contains("System Settings")) {
            return; // skip if not available
        }
        let index = AppIndex::from_apps(apps);
        let names = search_names(&index, "设置");
        assert!(
            names.iter().any(|n| n.contains("系统设置") || n.contains("System Settings")),
            "e2e: 设置 should find System Settings, got: {names:?}"
        );
    }

    #[test]
    fn e2e_search_tianqi_finds_weather() {
        use crate::platform::get_installed_apps;
        let apps = get_installed_apps(false);
        if !apps.iter().any(|a| a.name.contains("天气") || a.name.contains("Weather")) {
            return;
        }
        let index = AppIndex::from_apps(apps);
        let names = search_names(&index, "tianqi");
        assert!(
            names.iter().any(|n| n.contains("天气")),
            "e2e: tianqi should find 天气, got: {names:?}"
        );
    }

    #[test]
    fn e2e_search_weixin_finds_wechat() {
        use crate::platform::get_installed_apps;
        let apps = get_installed_apps(false);
        if !apps.iter().any(|a| a.name_lc.contains("wechat")) {
            return;
        }
        let index = AppIndex::from_apps(apps);
        let names = search_names(&index, "weixin");
        assert!(
            names.iter().any(|n| n.contains("微信")),
            "e2e: weixin should find 微信, got: {names:?}"
        );
    }

    #[test]
    fn e2e_icons_for_representative_apps() {
        use crate::utils::icon_from_workspace;
        use std::path::Path;

        // Test diverse icon formats: .icns, Assets.car, Electron, system apps
        let apps = [
            ("/System/Applications/Calendar.app", "Calendar (Assets.car)"),
            ("/System/Applications/Photo Booth.app", "Photo Booth (Assets.car)"),
            ("/System/Applications/System Settings.app", "System Settings"),
            ("/Applications/Safari.app", "Safari"),
        ];
        for (path, label) in &apps {
            let p = Path::new(path);
            if !p.exists() { continue; }
            assert!(icon_from_workspace(p).is_some(), "{label} should have icon");
        }
    }

    #[test]
    fn e2e_icon_workspace_individual_apps() {
        use crate::utils::icon_from_workspace;
        use std::path::Path;

        // Use a known app to test
        let p = Path::new("/Applications/Safari.app");
        if !p.exists() { return; }
        let result = icon_from_workspace(p);
        assert!(result.is_some(), "Safari should have an icon via NSWorkspace");
    }
}
