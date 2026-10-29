//! 更新相关纯逻辑：版本比较与 Release 资产挑选（无 IO，可单测）。

use serde::{Deserialize, Serialize};

/// 更新包资产（轻量：仅 exe+dll）与完整版资产
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAsset {
    pub name: String,
    pub size: u64,
    pub url: String,
}

/// "v1.2" / "1.2.0" → [1,2,0]（前缀 v 忽略，非数字尾巴忽略）
pub fn version_key(v: &str) -> Vec<u64> {
    v.trim()
        .trim_start_matches(['v', 'V'])
        .split(|c: char| c == '.' || !c.is_ascii_digit())
        .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))
        .filter_map(|s| s.parse::<u64>().ok())
        .collect()
}

/// 语义化版本比较（缺省段补 0）："1.1" 与 "1.1.0" 相等
pub fn version_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let mut ka = version_key(a);
    let mut kb = version_key(b);
    let n = ka.len().max(kb.len());
    ka.resize(n, 0);
    kb.resize(n, 0);
    ka.cmp(&kb)
}

/// 从 GitHub Release assets 里挑资产：
/// 更新包 = zip 且名字含 update/更新包；完整版 = zip 且名字含 完整版。
pub fn pick_assets(assets: &[serde_json::Value]) -> (Option<UpdateAsset>, Option<UpdateAsset>) {
    let mut update_asset = None;
    let mut full_asset = None;
    for a in assets {
        let name = a["name"].as_str().unwrap_or("").to_string();
        let lower = name.to_lowercase();
        if !lower.ends_with(".zip") {
            continue;
        }
        let asset = UpdateAsset {
            name: name.clone(),
            size: a["size"].as_u64().unwrap_or(0),
            url: a["browser_download_url"].as_str().unwrap_or("").to_string(),
        };
        if asset.url.is_empty() {
            continue;
        }
        if (lower.contains("update") || name.contains("更新包")) && update_asset.is_none() {
            update_asset = Some(asset);
        } else if name.contains("完整版") && full_asset.is_none() {
            full_asset = Some(asset);
        }
    }
    (update_asset, full_asset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_version_cmp() {
        use std::cmp::Ordering::*;
        assert_eq!(version_cmp("1.1", "1.1.0"), Equal);
        assert_eq!(version_cmp("v1.2", "1.1.0"), Greater);
        assert_eq!(version_cmp("1.0.0", "1.1.0"), Less);
        assert_eq!(version_cmp("1.10", "1.9"), Greater); // 数值比较而非字典序
        assert_eq!(version_cmp("V2.0", "1.9.9"), Greater);
        assert_eq!(version_cmp("1.1-beta", "1.1.0"), Equal); // 非数字尾巴忽略
        assert_eq!(version_cmp("", "1.0"), Less);
    }

    #[test]
    fn test_pick_assets() {
        let assets = vec![
            json!({"name": "小丸工具箱V2现代版1.1完整版.zip", "size": 100, "browser_download_url": "https://x/full.zip"}),
            json!({"name": "MarukoV2-update.zip", "size": 7, "browser_download_url": "https://x/update.zip"}),
            json!({"name": "checksums.txt", "size": 1, "browser_download_url": "https://x/txt"}),
            json!({"name": "broken.zip", "size": 1, "browser_download_url": ""}),
        ];
        let (upd, full) = pick_assets(&assets);
        assert_eq!(upd.unwrap().name, "MarukoV2-update.zip");
        assert!(full.unwrap().name.contains("完整版"));
        // 反向命名也能挑中
        let assets2 = vec![json!({"name": "工具更新包.zip", "size": 7, "browser_download_url": "https://x/u.zip"})];
        let (upd2, full2) = pick_assets(&assets2);
        assert!(upd2.is_some() && full2.is_none());
    }
}
