//! post-action-plugin — 投稿の **アクションバー** に足す手動ボタン(スロット `post.action`)のサンプル。
//!
//! | 項目 | 入口 |
//! |---|---|
//! | マニフェスト | `plugin.manifest`(`manifest.json` の `actions` にボタンとタグ = 希望スロット、`placement`(v0.14)を宣言) |
//! | 後で読む | `post.action {actionId:"later"}` → `storage.get later` → 先頭に足す(50 件 / URL で重複除去)→ `storage.set later` → `fow.open`(一覧) |
//! | 本文をコピー | `post.action {actionId:"copy"}` → `clipboard.set {text: body}` |
//! | 引用の下書き | `post.action {actionId:"quoteDraft"}` → `composer.open {token, text:"「本文 80 文字」\n<url>"}` |
//! | 一覧の行 | `later.open.<i>` → `url.open {url}` |
//! | 空にする | `later.clear` → `storage.set later []` + 一覧を surface patch で描き直す |
//! | FOW が閉じた | `panelClosed`(一覧は storage にあるので捨てるものは無い) |
//!
//! `plugin.manifest` の答えは同じディレクトリの `manifest.json` そのもの(`include_str!`。
//! `rasq check` の往復検査が常に一致する)。ボタン(`actions`)を足す / 直すときは `manifest.json` を直し、
//! `handle` の `post.action` にその `actionId` の枝を足す。
//!
//! 確認: `rasq check` / `rasq call post-action-plugin post.action --args '…' --reply '…'`
//! 契約: `../../CONTRACT.md` が指す rasq-sdk-sample `docs/slots.md` §2.9 / §2.1(投稿 payload)/ §4。

use rasq_plugin_kit::{err, fow_spec, host, i18n, rasq_plugin, rrl, serde_json::{json, Value}, unsupported, Surface};
use std::sync::Mutex;

mod text;
use text::TEXT;

/// 一覧に覚える本文 / 引用に入れる本文の上限(Unicode スカラー値)。
pub const BODY_CHARS: usize = 80;
/// 「後で読む」に覚える最大件数(新しい順)。
pub const LATER_MAX: usize = 50;

/// `storage.safe` のキー(保存した投稿の配列)。
const STORAGE_KEY: &str = "later";

static PANEL: Surface = Surface::new("later-list");

/// `plugin.manifest` の答え = `manifest.json`(ボタン / 名前 / 権限はあちらを直す)。
fn manifest() -> Value {
    rasq_plugin_kit::serde_json::from_str(include_str!("../manifest.json")).expect("manifest.json")
}

// ---------------------------------------------------------------------------
// 投稿 payload / 後で読むの 1 件
// ---------------------------------------------------------------------------

/// `post.action` の `post`(§2.1 の投稿 payload)から使うぶんだけ取り出したもの。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PostInput {
    pub url: String,
    pub author: String,
    pub handle: String,
    pub body: String,
    pub created_at: Option<String>,
}

/// `storage.safe` の `later` に並ぶ 1 件 `{url, author, body, createdAt?}`。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LaterEntry {
    pub url: String,
    pub author: String,
    /// 本文の先頭 80 文字。
    pub body: String,
    pub created_at: Option<String>,
}

impl LaterEntry {
    pub fn from_post(p: &PostInput) -> Self {
        let author = if p.author.is_empty() { p.handle.clone() } else { p.author.clone() };
        LaterEntry { url: p.url.clone(), author, body: truncate_chars(p.body.trim(), BODY_CHARS), created_at: p.created_at.clone() }
    }
    pub fn to_json(&self) -> Value {
        let mut v = json!({"url": self.url, "author": self.author, "body": self.body});
        if let Some(c) = &self.created_at {
            v["createdAt"] = json!(c);
        }
        v
    }
    pub fn from_json(v: &Value) -> Option<Self> {
        let url = v["url"].as_str()?.to_string();
        if url.is_empty() {
            return None;
        }
        Some(LaterEntry {
            url,
            author: str_of(v, "author"),
            body: str_of(v, "body"),
            created_at: v["createdAt"].as_str().map(str::to_string),
        })
    }
}

/// 先頭 `max` 文字(Unicode スカラー値)。切ったら末尾に `…`。
pub fn truncate_chars(s: &str, max: usize) -> String {
    let mut it = s.chars();
    let head: String = it.by_ref().take(max).collect();
    if it.next().is_some() {
        format!("{head}…")
    } else {
        head
    }
}

/// 引用の下書きの文面: `「<本文 80 文字まで>」\n<url>`。
pub fn quote_text(p: &PostInput) -> String {
    format!("「{}」\n{}", truncate_chars(p.body.trim(), BODY_CHARS), p.url)
}

/// 一覧の先頭に足す。同じ URL が既にあれば足さず `false`。`LATER_MAX` を超えたぶんは末尾(古い方)から落とす。
pub fn add_later(list: &mut Vec<LaterEntry>, entry: LaterEntry) -> bool {
    if list.iter().any(|e| e.url == entry.url) {
        return false;
    }
    list.insert(0, entry);
    list.truncate(LATER_MAX);
    true
}

fn str_of(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or("").to_string()
}

fn post_from_args(args: &Value) -> Option<PostInput> {
    let post = args.get("post")?.as_object()?;
    let post = Value::Object(post.clone());
    Some(PostInput {
        url: str_of(&post, "url"),
        author: str_of(&post, "author"),
        handle: str_of(&post, "handle"),
        body: str_of(&post, "body"),
        created_at: post["createdAt"].as_str().map(str::to_string),
    })
}

fn load_later() -> Vec<LaterEntry> {
    host::storage_get(STORAGE_KEY)
        .as_array()
        .map(|a| a.iter().filter_map(LaterEntry::from_json).collect())
        .unwrap_or_default()
}

fn save_later(list: &[LaterEntry]) {
    host::storage_set(STORAGE_KEY, Value::Array(list.iter().map(LaterEntry::to_json).collect()));
}

// ---------------------------------------------------------------------------
// 状態(一覧 FOW が開いているか。一覧そのものは storage にある)
// ---------------------------------------------------------------------------

static OPEN: Mutex<bool> = Mutex::new(false);

fn set_open(v: bool) {
    *OPEN.lock().unwrap_or_else(|p| p.into_inner()) = v;
}
fn is_open() -> bool {
    *OPEN.lock().unwrap_or_else(|p| p.into_inner())
}

// ---------------------------------------------------------------------------
// RRL(一覧 FOW の中身)
// ---------------------------------------------------------------------------

fn later_column(loc: &str, list: &[LaterEntry]) -> Value {
    let count = TEXT.f(loc, "later.count", &[("n", &list.len().to_string())]);
    let mut children = vec![rrl::styled_text("later-count", count, 13.0, true, Some("#888888"))];
    if list.is_empty() {
        children.push(rrl::styled_text("later-empty", TEXT.t(loc, "later.empty"), 12.0, false, Some("#888888")));
    }
    children.extend(list.iter().enumerate().map(|(i, e)| {
        let title = if e.author.is_empty() { e.url.as_str() } else { e.author.as_str() };
        rrl::list_tile(&format!("later.{i}"), title, Some(&e.body), Some(&format!("later.open.{i}")))
    }));
    children.push(rrl::button("later.clear", TEXT.t(loc, "later.clear"), "later.clear"));
    rrl::column("root", children)
}

fn later_root(loc: &str, list: &[LaterEntry]) -> Value {
    rrl::scroll("root-scroll", false, vec![later_column(loc, list)])
}

/// 本体ツリーは使わない(UI は一覧 FOW だけ)。
fn root() -> Value {
    let loc = i18n::current();
    rrl::column("root", vec![rrl::text("hello", TEXT.t(&loc, "root.hello"))])
}

fn open_list(loc: &str, list: &[LaterEntry]) {
    let opened = PANEL.open(fow_spec(TEXT.t(loc, "later.title"), 0.55), later_root(loc, list), Some("panelClosed"), json!({}));
    set_open(opened);
}

// ---------------------------------------------------------------------------
// call
// ---------------------------------------------------------------------------

fn handle(fn_name: &str, args: Value) -> Result<Value, Value> {
    let loc = i18n::locale_of(&args).to_string();
    let loc = loc.as_str();
    match fn_name {
        "plugin.manifest" => Ok(manifest()),

        // --- ボタンを押した ---------------------------------------------------
        "post.action" => {
            let token = str_of(&args, "token");
            let action_id = str_of(&args, "actionId");
            let post = post_from_args(&args).ok_or_else(|| err("D01", "post required"))?;
            match action_id.as_str() {
                "later" => {
                    if post.url.is_empty() {
                        return Err(err("D01", "post.url required"));
                    }
                    let mut list = load_later();
                    let added = add_later(&mut list, LaterEntry::from_post(&post));
                    if added {
                        save_later(&list);
                    }
                    open_list(loc, &list);
                    let message = TEXT.t(loc, if added { "later.added" } else { "later.already" });
                    Ok(json!({"result": {"done": true, "message": message}}))
                }
                "copy" => {
                    host::clipboard_set(&post.body);
                    Ok(json!({"result": {"done": true, "message": TEXT.t(loc, "copy.done")}}))
                }
                "quoteDraft" => {
                    host::composer_open(&token, &quote_text(&post), None, None, None);
                    Ok(json!({"result": {"done": true}}))
                }
                other => Err(err("D01", format!("unknown actionId: {other}"))),
            }
        }

        // --- 一覧 FOW ---------------------------------------------------------
        f if f.starts_with("later.open.") => {
            let idx: usize = f["later.open.".len()..].parse().map_err(|_| unsupported(f))?;
            if let Some(e) = load_later().get(idx) {
                host::url_open(&e.url);
            }
            Ok(json!({}))
        }
        "later.clear" => {
            save_later(&[]);
            if is_open() {
                Ok(PANEL.patch(vec![rrl::replace_subtree("root", later_column(loc, &[]))]))
            } else {
                Ok(json!({}))
            }
        }
        "panelClosed" => {
            set_open(false);
            Ok(json!({}))
        }
        "lifecycle" => Ok(json!({})),

        other => Err(unsupported(other)),
    }
}

rasq_plugin! { scope: "sample.post-action:main", root: root, call: handle }

// ---------------------------------------------------------------------------
// テスト(native): ホスト関数はフックで差し替える
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rasq_plugin_kit::{set_host_hook, take_host_log, take_patch_message};
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::{Mutex as StdMutex, MutexGuard};

    /// テストはグローバル状態を共有するので直列化する。
    static SERIAL: StdMutex<()> = StdMutex::new(());

    const URL1: &str = "https://example.social/@alice/123";
    const URL2: &str = "https://example.social/@bob/456";

    /// ホストのふり: fow.open は受理、storage.safe の `later` は `store` に持つ。
    fn setup(stored: Value) -> (MutexGuard<'static, ()>, Rc<RefCell<Value>>) {
        let g = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
        let store = Rc::new(RefCell::new(stored));
        let st = store.clone();
        set_host_hook(move |f, a| match f {
            "fow.open" => json!({"opened": true}),
            "storage.get" if a["key"] == STORAGE_KEY => json!({"value": st.borrow().clone()}),
            "storage.set" if a["key"] == STORAGE_KEY => {
                *st.borrow_mut() = a["value"].clone();
                json!({})
            }
            _ => json!({}),
        });
        set_open(false);
        take_host_log();
        take_patch_message();
        (g, store)
    }

    fn action(token: &str, action_id: &str, url: &str, author: &str, body: &str) -> Result<Value, Value> {
        call_native(
            "post.action",
            json!({
                "token": token,
                "actionId": action_id,
                "post": {
                    "url": url, "sns": "mastodon", "instance": "example.social", "postId": "123",
                    "author": author, "handle": "alice", "body": body, "media": [], "createdAt": "2026-09-12T00:00:00Z"
                },
                "account": {"sns": "mastodon", "instance": "mine.example", "handle": "me"}
            }),
        )
    }

    fn find_node<'a>(node: &'a Value, key: &str) -> Option<&'a Value> {
        if node["k"].as_str() == Some(key) {
            return Some(node);
        }
        node["c"].as_array()?.iter().find_map(|c| find_node(c, key))
    }

    /// `plugin.manifest` は `manifest.json` そのもの(`rasq check` の往復検査と同じ突き合わせ)。
    #[test]
    fn manifest_matches_manifest_json() {
        let m = call_native("plugin.manifest", json!({})).unwrap();
        let file: Value = rasq_plugin_kit::serde_json::from_str(include_str!("../manifest.json")).unwrap();
        assert_eq!(m, file);
    }

    /// placement(v0.14): `later` / `copy` は宣言だけで自動配置(`auto`)、`quoteDraft` は利用者が (+) で足すまで出ない(`manual`)。
    /// `auto`(temp)は書かない = post.action の既定 `false`(押すまで動かない。`auto_hook` 権限も持たない)。
    #[test]
    fn manifest_declares_actions_with_tags() {
        let m = call_native("plugin.manifest", json!({})).unwrap();
        assert_eq!(m["id"], "sample.post-action");
        assert_eq!(m["name"], "投稿ボタン(サンプル)");
        assert_eq!(m["kind"], "action");
        assert_eq!(m["slots"], json!(["post.action"]));
        assert_eq!(m["permissions"], json!(["fow.widget", "storage.safe"]));
        assert_eq!(
            m["actions"],
            json!([
                {"id": "later", "label": "後で読む", "icon": "bookmark_add", "tags": ["Favorite", "Bookmark", "Star"], "placement": "auto"},
                {"id": "copy", "label": "本文をコピー", "icon": "content_copy", "tags": ["Copy", "Share"], "placement": "auto"},
                {"id": "quoteDraft", "label": "引用の下書き", "icon": "format_quote", "tags": ["Quote", "Repost"], "placement": "manual"}
            ])
        );
        for a in m["actions"].as_array().unwrap() {
            assert!(a.get("auto").is_none(), "auto は書かない(auto_hook を持たない): {a}");
        }
    }

    /// マニフェストが宣言したボタンは全部 `post.action` で受けられる(取りこぼしを防ぐ)。
    #[test]
    fn every_declared_action_is_handled() {
        let (_g, _store) = setup(Value::Null);
        let m = call_native("plugin.manifest", json!({})).unwrap();
        for a in m["actions"].as_array().unwrap() {
            let id = a["id"].as_str().unwrap();
            let r = action("tk", id, URL1, "アリス", "本文");
            assert!(r.is_ok(), "actionId {id} が handle にない: {r:?}");
        }
    }

    #[test]
    fn text_helpers() {
        assert_eq!(truncate_chars("abc", 3), "abc");
        assert_eq!(truncate_chars("abcd", 3), "abc…");
        let p = PostInput { url: URL1.into(), body: "  こんにちは \n".into(), ..Default::default() };
        assert_eq!(quote_text(&p), format!("「こんにちは」\n{URL1}"));
        let long = "あ".repeat(100);
        let p = PostInput { url: URL1.into(), body: long, ..Default::default() };
        assert_eq!(quote_text(&p), format!("「{}…」\n{URL1}", "あ".repeat(BODY_CHARS)));
        // 一覧の 1 件: 作者が無ければハンドル、本文は 80 文字
        let p = PostInput { url: URL1.into(), handle: "alice".into(), body: "x".repeat(90), created_at: Some("t".into()), ..Default::default() };
        let e = LaterEntry::from_post(&p);
        assert_eq!(e.author, "alice");
        assert_eq!(e.body, format!("{}…", "x".repeat(80)));
        assert_eq!(e.to_json(), json!({"url": URL1, "author": "alice", "body": e.body, "createdAt": "t"}));
        assert_eq!(LaterEntry::from_json(&e.to_json()), Some(e));
        assert_eq!(LaterEntry::from_json(&json!({"author": "no url"})), None);
    }

    #[test]
    fn add_later_dedupes_and_caps() {
        let mut list = vec![];
        for i in 0..60 {
            assert!(add_later(&mut list, LaterEntry { url: format!("https://x/{i}"), ..Default::default() }));
        }
        assert_eq!(list.len(), LATER_MAX);
        assert_eq!(list[0].url, "https://x/59"); // 新しい順
        assert_eq!(list[LATER_MAX - 1].url, "https://x/10"); // 古いものが落ちる
        assert!(!add_later(&mut list, LaterEntry { url: "https://x/59".into(), ..Default::default() }));
        assert_eq!(list.len(), LATER_MAX);
    }

    #[test]
    fn later_stores_and_opens_fow() {
        let (_g, store) = setup(Value::Null);
        let r = action("tk1", "later", URL1, "アリス", "こんにちは").unwrap();
        assert_eq!(r, json!({"result": {"done": true, "message": "後で読むに追加しました"}}));
        let log = take_host_log();
        assert!(log.iter().any(|(f, a)| f == "storage.get" && a["key"] == STORAGE_KEY));
        let set = log.iter().find(|(f, _)| f == "storage.set").expect("storage.set");
        assert_eq!(set.1["ns"], "safe");
        assert_eq!(set.1["value"], json!([{"url": URL1, "author": "アリス", "body": "こんにちは", "createdAt": "2026-09-12T00:00:00Z"}]));
        assert_eq!(store.borrow().as_array().unwrap().len(), 1);
        let open = log.iter().find(|(f, _)| f == "fow.open").expect("fow.open");
        assert_eq!(open.1["id"], "later-list");
        assert_eq!(open.1["spec"]["title"], "後で読む");
        assert_eq!(open.1["onClosed"], "panelClosed");
        let root = &open.1["root"];
        assert_eq!(find_node(root, "later-count").unwrap()["p"]["text"], "後で読む: 1 件");
        let tile = find_node(root, "later.0").expect("later.0");
        assert_eq!(tile["t"], "ListTile");
        assert_eq!(tile["p"]["title"], "アリス");
        assert_eq!(tile["p"]["subtitle"], "こんにちは");
        assert_eq!(tile["on"]["tap"], "later.open.0");
        let clear = find_node(root, "later.clear").expect("later.clear");
        assert_eq!(clear["t"], "Button");
        assert_eq!(clear["p"]["label"], "後で読むを空にする");
        assert_eq!(clear["on"]["tap"], "later.clear");
        assert!(find_node(root, "later-empty").is_none());

        // 同じ URL をもう一度: 足さない(storage.set 無し)が FOW は開く
        let r = action("tk2", "later", URL1, "アリス", "こんにちは").unwrap();
        assert_eq!(r["result"]["message"], "すでに追加されています");
        let log = take_host_log();
        assert!(!log.iter().any(|(f, _)| f == "storage.set"));
        assert!(log.iter().any(|(f, _)| f == "fow.open"));
        assert_eq!(store.borrow().as_array().unwrap().len(), 1);

        // 別の投稿は先頭に入る(新しい順)
        action("tk3", "later", URL2, "ボブ", "second").unwrap();
        let open = take_host_log().into_iter().find(|(f, _)| f == "fow.open").unwrap();
        assert_eq!(find_node(&open.1["root"], "later.0").unwrap()["p"]["title"], "ボブ");
        assert_eq!(find_node(&open.1["root"], "later.1").unwrap()["p"]["title"], "アリス");
        assert_eq!(store.borrow()[0]["url"], URL2);
        assert_eq!(store.borrow()[1]["url"], URL1);
    }

    #[test]
    fn later_open_and_clear() {
        let (_g, store) = setup(json!([
            {"url": URL2, "author": "ボブ", "body": "second"},
            {"url": URL1, "author": "アリス", "body": "first"}
        ]));
        action("tk1", "later", URL1, "アリス", "first").unwrap(); // すでにある → FOW を開くだけ
        take_host_log();

        // 行をタップ → url.open
        assert_eq!(call_native("later.open.1", json!({"nodeId": "sample.post-action:main:later.1", "kind": "tap"})).unwrap(), json!({}));
        assert_eq!(take_host_log().iter().find(|(f, _)| f == "url.open").unwrap().1["url"], URL1);
        // 範囲外は何もしない
        call_native("later.open.9", json!({})).unwrap();
        assert!(!take_host_log().iter().any(|(f, _)| f == "url.open"));
        assert!(call_native("later.open.x", json!({})).is_err());

        // 空にする → storage.set [] + 一覧を patch で描き直す
        let r = call_native("later.clear", json!({"nodeId": "sample.post-action:main:later.clear", "kind": "tap"})).unwrap();
        assert_eq!(r, json!({"patch": true}));
        assert_eq!(*store.borrow(), json!([]));
        let m = take_patch_message();
        assert_eq!(m["surface"], "later-list");
        let op = &m["ops"][0];
        assert_eq!(op["op"], "replaceSubtree");
        assert_eq!(op["target"], "sample.post-action:main:root");
        assert!(find_node(&op["node"], "later-empty").is_some());
        assert!(find_node(&op["node"], "later.0").is_none());
        assert!(find_node(&op["node"], "later.clear").is_some());

        // FOW が閉じた後の clear は storage だけ(patch 無し)
        call_native("panelClosed", json!({"id": "later-list", "reason": "cancel"})).unwrap();
        assert_eq!(call_native("later.clear", json!({})).unwrap(), json!({}));
    }

    #[test]
    fn copy_sets_clipboard() {
        let (_g, _store) = setup(Value::Null);
        let r = action("tk1", "copy", URL1, "アリス", "本文です").unwrap();
        assert_eq!(r, json!({"result": {"done": true, "message": "本文をコピーしました"}}));
        let log = take_host_log();
        assert_eq!(log.iter().find(|(f, _)| f == "clipboard.set").unwrap().1, json!({"text": "本文です"}));
        assert!(!log.iter().any(|(f, _)| f == "fow.open"));
    }

    #[test]
    fn quote_draft_opens_composer() {
        let (_g, _store) = setup(Value::Null);
        let long = "い".repeat(100);
        let r = action("tk1", "quoteDraft", URL1, "アリス", &long).unwrap();
        assert_eq!(r, json!({"result": {"done": true}}));
        let log = take_host_log();
        let open = log.iter().find(|(f, _)| f == "composer.open").expect("composer.open");
        assert_eq!(open.1, json!({"token": "tk1", "text": format!("「{}…」\n{URL1}", "い".repeat(BODY_CHARS))}));
    }

    /// `args.locale` の言語で文を引く(PROTOCOL.md §4.6)。表に無い言語・無い locale は ja。
    #[test]
    fn texts_follow_locale() {
        let (_g, _store) = setup(Value::Null);
        let post = json!({"url": URL1, "author": "アリス", "handle": "alice", "body": "hi"});
        let r = call_native("post.action", json!({"token": "t", "actionId": "later", "post": post, "locale": "en"})).unwrap();
        assert_eq!(r["result"]["message"], "Added to Read later");
        let open = take_host_log().into_iter().find(|(f, _)| f == "fow.open").unwrap();
        assert_eq!(open.1["spec"]["title"], "Read later");
        assert_eq!(find_node(&open.1["root"], "later-count").unwrap()["p"]["text"], "Read later: 1");
        assert_eq!(find_node(&open.1["root"], "later.clear").unwrap()["p"]["label"], "Clear Read later");
        let r = call_native("post.action", json!({"token": "t", "actionId": "later", "post": post, "locale": "ko-KR"})).unwrap();
        assert_eq!(r["result"]["message"], "이미 추가되어 있습니다");
        let r = call_native("post.action", json!({"token": "t", "actionId": "copy", "post": post, "locale": "zh-Hant-TW"})).unwrap();
        assert_eq!(r["result"]["message"], "已複製內文");
        let r = call_native("post.action", json!({"token": "t", "actionId": "copy", "post": post, "locale": "zh-Hans"})).unwrap();
        assert_eq!(r["result"]["message"], "已复制正文");
        let r = call_native("post.action", json!({"token": "t", "actionId": "copy", "post": post, "locale": "fr"})).unwrap();
        assert_eq!(r["result"]["message"], "本文をコピーしました");
        // 空にした後の描き直しも同じ言語
        let r = call_native("later.clear", json!({"locale": "en"})).unwrap();
        assert_eq!(r, json!({"patch": true}));
        let m = take_patch_message();
        assert_eq!(find_node(&m["ops"][0]["node"], "later-empty").unwrap()["p"]["text"], "Nothing here yet");
        // 本体ツリー(args の無い root)は最新の call の言語
        call_native("lifecycle", json!({"kind": "mount", "locale": "ko"})).unwrap();
        assert_eq!(find_node(&root(), "hello").unwrap()["p"]["text"], "게시물 버튼(UI는 '나중에 읽기' 패널)");
        call_native("lifecycle", json!({"kind": "mount"})).unwrap();
        assert_eq!(find_node(&root(), "hello").unwrap()["p"]["text"], "投稿ボタン(UI は後で読むの FOW)");
        assert!(i18n::take_misses().is_empty());
    }

    /// マニフェストの `locales`: 4 言語とも名前・説明・ボタンの表示文を全部持つ。
    #[test]
    fn manifest_locales_cover_every_action() {
        let m = call_native("plugin.manifest", json!({})).unwrap();
        let locales = m["locales"].as_object().unwrap();
        assert_eq!(locales.keys().map(String::as_str).collect::<Vec<_>>(), ["en", "ko", "zh-Hans", "zh-Hant"]);
        for (tag, l) in locales {
            assert!(l["name"].as_str().is_some_and(|s| !s.is_empty()), "{tag}");
            assert!(l["description"].as_str().is_some_and(|s| !s.is_empty()), "{tag}");
            for a in m["actions"].as_array().unwrap() {
                let id = a["id"].as_str().unwrap();
                assert!(l["actions"][id]["label"].as_str().is_some_and(|s| !s.is_empty()), "{tag}: {id}");
            }
        }
        assert_eq!(m["locales"]["en"]["actions"]["later"]["label"], "Read later");
    }

    #[test]
    fn bad_input_is_d01() {
        let (_g, _store) = setup(Value::Null);
        let e = action("tk1", "nope", URL1, "アリス", "x").unwrap_err();
        assert_eq!(e["code"], "D01");
        let e = call_native("post.action", json!({"token": "t", "actionId": "copy"})).unwrap_err();
        assert_eq!(e["code"], "D01");
        let e = call_native("post.action", json!({"token": "t", "actionId": "copy", "post": 5})).unwrap_err();
        assert_eq!(e["code"], "D01");
        assert!(call_native("post.action", json!(null)).is_err());
        let e = action("tk1", "later", "", "アリス", "x").unwrap_err();
        assert_eq!(e["code"], "D01");
        assert!(take_host_log().iter().all(|(f, _)| f != "fow.open" && f != "clipboard.set" && f != "composer.open"));
        assert!(call_native("lifecycle", json!({"kind": "mount"})).is_ok());
        assert_eq!(call_native("nope", json!({})).unwrap_err()["code"], "C01");
    }
}
