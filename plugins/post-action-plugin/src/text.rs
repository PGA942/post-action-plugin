//! 利用者に見せる文の表(PROTOCOL.md §4.6)。ホストの `args.locale`(無ければ `ja`)で引く。
//!
//! 基準は `ja`。`en` / `zh-Hans` / `zh-Hant` / `ko` は同じキーを全部持つ(テストで確かめる)。

rasq_plugin_kit::i18n_catalog! {
    pub(crate) static TEXT = {
        "ja": {
            "root.hello" => "投稿ボタン(UI は後で読むの FOW)",
            "later.title" => "後で読む",
            "later.count" => "後で読む: {n} 件",
            "later.empty" => "まだありません",
            "later.clear" => "後で読むを空にする",
            "later.added" => "後で読むに追加しました",
            "later.already" => "すでに追加されています",
            "copy.done" => "本文をコピーしました",
        },
        "en": {
            "root.hello" => "Post buttons (the UI is the Read later panel)",
            "later.title" => "Read later",
            "later.count" => "Read later: {n}",
            "later.empty" => "Nothing here yet",
            "later.clear" => "Clear Read later",
            "later.added" => "Added to Read later",
            "later.already" => "Already added",
            "copy.done" => "Text copied",
        },
        "zh-Hans": {
            "root.hello" => "帖子按钮（界面为“稍后阅读”面板）",
            "later.title" => "稍后阅读",
            "later.count" => "稍后阅读：{n} 条",
            "later.empty" => "暂无内容",
            "later.clear" => "清空稍后阅读",
            "later.added" => "已添加到稍后阅读",
            "later.already" => "已经添加过了",
            "copy.done" => "已复制正文",
        },
        "zh-Hant": {
            "root.hello" => "貼文按鈕（介面為「稍後閱讀」面板）",
            "later.title" => "稍後閱讀",
            "later.count" => "稍後閱讀：{n} 則",
            "later.empty" => "尚無內容",
            "later.clear" => "清空稍後閱讀",
            "later.added" => "已加入稍後閱讀",
            "later.already" => "已經加入過了",
            "copy.done" => "已複製內文",
        },
        "ko": {
            "root.hello" => "게시물 버튼(UI는 '나중에 읽기' 패널)",
            "later.title" => "나중에 읽기",
            "later.count" => "나중에 읽기: {n}개",
            "later.empty" => "아직 없습니다",
            "later.clear" => "나중에 읽기 비우기",
            "later.added" => "나중에 읽기에 추가되었습니다",
            "later.already" => "이미 추가되어 있습니다",
            "copy.done" => "본문이 복사되었습니다",
        },
    };
}

#[cfg(test)]
mod tests {
    use super::TEXT;
    use std::collections::BTreeSet;

    /// この表の言語ごとのキー(`Catalog::check` は「基準に無いキー」と差し込みの食い違いしか見ないので、訳の抜けはここで見る)。
    fn table_keys() -> Vec<(String, BTreeSet<String>)> {
        let mut out: Vec<(String, BTreeSet<String>)> = Vec::new();
        for line in include_str!("text.rs").lines().map(str::trim) {
            if let Some(tag) = line.strip_prefix('"').and_then(|l| l.strip_suffix("\": {")) {
                out.push((tag.to_string(), BTreeSet::new()));
            } else if let Some((key, _)) = line.strip_prefix('"').and_then(|l| l.split_once("\" =>")) {
                out.last_mut().expect("表の外のキー").1.insert(key.to_string());
            }
        }
        out
    }

    #[test]
    fn every_locale_has_every_key() {
        assert!(TEXT.check().is_empty(), "{:?}", TEXT.check());
        let tables = table_keys();
        let tags: Vec<&str> = tables.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(tags, TEXT.locales());
        assert_eq!(tags, ["ja", "en", "zh-Hans", "zh-Hant", "ko"]);
        for (tag, keys) in &tables[1..] {
            let missing: Vec<_> = tables[0].1.difference(keys).collect();
            assert!(missing.is_empty(), "{tag} に訳が無いキー: {missing:?}");
        }
    }
}
