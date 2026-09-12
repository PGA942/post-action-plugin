# post-action-plugin — 投稿ボタン(サンプル)

スロット **`post.action`**(CONTRACT.md §2.9)。`post.transform`(自動で本文を書き換える)と違い、
投稿の **アクションバー** に **ユーザーが押して動く** ボタンを足す。マニフェストの `actions` に 3 つ宣言する:

| id | ラベル | icon | tags(希望スロット) | 動き |
|---|---|---|---|---|
| `later` | 後で読む | `bookmark_add` | `Favorite` / `Bookmark` / `Star` | `storage.safe` の `later` に先頭追加(最大 50 件、URL で重複除去)→ 一覧 FOW を開く |
| `copy` | 本文をコピー | `content_copy` | `Copy` / `Share` | `clipboard.set {text: post.body}` |
| `quoteDraft` | 引用の下書き | `format_quote` | `Quote` / `Repost` | `composer.open {token, text: "「<本文 80 文字まで>」\n<url>"}` |

## タグとバンドル(ボタンの置き場所はホストが決める)

プラグインは **置き場所を指定しない**。代わりに `tags` で「こういう既存ボタンの仲間にしてほしい」と希望を出し、
ホストがアカウント(SNS)ごとに既存アクションボタンのタグ(`genreTag`: `Reaction` / `Repost` / `Reply` / `Quote` / `Favorite` …と別名)と
比べて置く:

- 類似度 = タグ同士の最大値(大文字小文字を無視した完全一致 = 1.0、それ以外は文字 2-gram の Dice 係数)
- 類似度が **閾値(既定 0.85)以上** の既存ボタンがあれば、その **バンドルの最下層** に足す
  (既存ボタンがアクションリストにあればその末尾、無ければ 既存ボタン + プラグインボタン の新しいバンドル)
- どれにも届かなければ **単体ボタン** として末尾に並ぶ

例: `later` の `Favorite` は Mastodon の「お気に入り」(`Favorite`)に一致 → お気に入りのバンドルに入る。Misskey なら
`Reaction` には届かないが、`Bookmark` / `Star` の別名がホストにあればそこへ。`quoteDraft` の `Quote` / `Repost` は
引用 / リポスト(ブースト)のバンドルへ。閾値やボタンごとの有効 / 無効は「アクションリストの編集」→「プラグインボタン」節で
アカウントごとに変えられる。ボタンの id はホスト側で `slot:sample.post-action:<actionId>` になる。

## call / ホスト関数の流れ

```
ホスト → plugin.manifest                                   ← {id:"sample.post-action", slots:["post.action"], permissions:["fow.widget","storage.safe"], actions:[…]}
ホスト → post.action {token, actionId:"later", post, account}
            プラグイン → storage.get later
            プラグイン → storage.set later [{url, author, body, createdAt?}, …](新しければ)
            プラグイン → fow.open {id:"later-list", spec:{title:"後で読む", initialHeightFactor:0.55}, onClosed:"panelClosed", root}
                                                           ← {result:{done:true, message:"後で読むに追加しました" | "すでに追加されています"}}
ホスト → post.action {actionId:"copy", …}                  → clipboard.set {text: body}      ← {result:{done:true, message:"本文をコピーしました"}}
ホスト → post.action {actionId:"quoteDraft", …}            → composer.open {token, text}     ← {result:{done:true}}
ホスト → post.action {actionId:"(不明)", …} / post 無し     ← D01
FOW の ListTile → later.open.<i>                           → url.open {url}                  ← {}
FOW ボタン → later.clear                                   → storage.set later [] → {patch:true}(root を replaceSubtree)
ホスト → panelClosed / lifecycle                           ← {}(一覧は storage にあるので捨てるものは無い)
```

| ノード key | 種類 | fn |
|---|---|---|
| `later-count` | `Text`(件数) | — |
| `later-empty` | `Text`(0 件のとき) | — |
| `later.<i>` | `ListTile`(title = 作者、subtitle = 本文 80 文字) | `later.open.<i>` |
| `later.clear` | `Button`「後で読むを空にする」 | `later.clear` |

`message` はホストがトーストに出す。`later` の一覧は `storage.safe` の `later`(配列、新しい順)にだけあるので、
FOW を閉じても消えない。

## ビルド / テスト

```sh
cargo test -p post-action-plugin
cargo build --release --target wasm32-unknown-unknown -p post-action-plugin
#  → target/wasm32-unknown-unknown/release/post_action_plugin.wasm
```

native テストは `rasq_plugin_kit::set_host_hook` でホスト関数を差し替え、マニフェストの `actions`(3 つ + tags)、
`later` → `storage.set` + `fow.open`(行の title / subtitle / tap、重複除去、新しい順、50 件上限)、`later.open.<i>` → `url.open`、
`later.clear` → surface patch、`copy` → `clipboard.set`、`quoteDraft` → `composer.open`(80 文字で切る)、
不明な `actionId` / `post` 無し → `D01` を検証している。

## 端末で試す(rasq-client)

1. サイドバー → **WASMプラグイン試験 (dev)** → `.wasm のパスからインポート` に `post_action_plugin.wasm` の絶対パス → **読み込み**
2. **スロットに登録**(権限 `fow.widget` / `storage.safe` を許可)
3. 投稿のアクションバーで、お気に入り / リポスト などのバンドルを開くと最下層にプラグインのボタンが出る
   (届かなかったものは末尾の単体ボタン)。「アクションリストの編集」→「プラグインボタン」で有効 / 無効と閾値を変えられる
4. 「後で読む」→ 一覧 FOW(行のタップでブラウザ、「後で読むを空にする」でクリア)、「本文をコピー」→ トースト、
   「引用の下書き」→ 選んだアカウントのコンポーザーに文面が入る

---

## このリポジトリの形

- クレートはルート(`src/`)、共通部品 [`rasq-plugin-kit`](kit) は `kit/` に同梱(開発用モノレポから写している)。
- 契約は [`CONTRACT.md`](CONTRACT.md)(スロットプラグイン契約 v0.1)。
- ビルド済み `.wasm` は [`dist/`](dist)。`cargo build --release --target wasm32-unknown-unknown` で更新。
