# post-action-plugin — 投稿ボタン(サンプル)

スロット **`post.action`**(`rasq-sdk-sample/docs/slots.md` §2.9)。`post.transform`(自動で本文を書き換える)と違い、
投稿の **アクションバー** に **ユーザーが押して動く** ボタンを足す。マニフェストの `actions` に 3 つ宣言する:

| id | ラベル | icon | tags(希望スロット) | 動き |
|---|---|---|---|---|
| `later` | 後で読む | `bookmark_add` | `Favorite` / `Bookmark` / `Star` | `storage.safe` の `later` に先頭追加(最大 50 件、URL で重複除去)→ 一覧 FOW を開く |
| `copy` | 本文をコピー | `content_copy` | `Copy` / `Share` | `clipboard.set {text: post.body}` |
| `quoteDraft` | 引用の下書き | `format_quote` | `Quote` / `Repost` | `composer.open {token, text: "「<本文 80 文字まで>」\n<url>"}` |

## このリポジトリの形(`rasq` ワークスペース)

`rasq-sdk-sample` の `docs/cli.md` §2 のワークスペース。`rasq init` / `rasq new post.action` が作る形に合わせてある。

```
rasq.toml                                # schema = "rasq.workspace/1"、[[plugins]] に name/slot/id
Cargo.toml                               # [workspace](rasq CLI が rasq.toml から作り直す)
kit/                                     # rasq-plugin-kit の写し(正典は SDK の rust/rasq-plugin-kit、D-244i)
plugins/post-action-plugin/
  manifest.json                          # plugin.manifest の答えそのもの(actions もここ)
  src/lib.rs                             # manifest.json を include_str! で返し、post.action ほかを実装
dist/                                    # rasq build --raw の出力(.rasqpkg と生の .wasm)
```

- 契約の正典は [`CONTRACT.md`](CONTRACT.md) が指す `rasq-sdk-sample/docs/slots.md` §2.9 / §2.1(投稿 payload)/ §4。
- **ボタンを足す / 直すときは `plugins/post-action-plugin/manifest.json` の `actions` を直し、`src/lib.rs` の
  `post.action` にその `actionId` の枝を足す**。`src/lib.rs` はマニフェストを `include_str!` でそのまま返すので、
  `rasq check` の往復検査(wasm の `plugin.manifest` == `manifest.json`)が常に一致する。宣言したボタンが
  `handle` に無い取りこぼしは `every_declared_action_is_handled` のテストが止める。

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
アカウントごとに変えられる。ボタンの id はホスト側で `slot:sample.post-action:<actionId>`(= ActionKey)になる。

## call / ホスト関数の流れ

```
ホスト → plugin.manifest                                   ← manifest.json そのもの
                                                             {id:"sample.post-action", slots:["post.action"],
                                                              permissions:["fow.widget","storage.safe"], actions:[…3 つ]}
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

## ビルドと確認

`rasq` CLI は `rasq-sdk-sample` から組む(`cd rust && cargo build --release -p rasq-cli`)。

```sh
rasq check                 # wasm32 ビルド → export 検査 → manifest 往復 → スキーマ / 登録簿 → cargo test
rasq test                  # cargo test だけ
rasq build --raw           # dist/sample.post-action-0.1.0.rasqpkg と dist/post_action_plugin.wasm
rasq inspect dist/sample.post-action-0.1.0.rasqpkg
```

native テストは `rasq_plugin_kit::set_host_hook` でホスト関数を差し替え、マニフェストの `actions`(3 つ + tags)、
`later` → `storage.set` + `fow.open`(行の title / subtitle / tap、重複除去、新しい順、50 件上限)、`later.open.<i>` → `url.open`、
`later.clear` → surface patch、`copy` → `clipboard.set`、`quoteDraft` → `composer.open`(80 文字で切る)、
不明な `actionId` / `post` 無し → `D01` を検証している。

`cargo test` は native で走るので、wasm でだけ起きる問題(時刻 / 乱数 / スレッド、`docs/cli.md` §5 の落とし穴)は
**ビルド済みの wasm をそのまま叩く** `rasq call` で確かめる。ホスト関数の答えは `--reply` で差し込む:

```sh
rasq call post-action-plugin plugin.manifest

rasq call post-action-plugin post.action \
  --args '{"token":"tk1","actionId":"later",
           "post":{"url":"https://example.social/@alice/123","author":"アリス","handle":"alice",
                   "body":"こんにちは","createdAt":"2026-09-12T00:00:00Z"},
           "account":{"sns":"mastodon","instance":"mine.example","handle":"me"}}' \
  --reply 'storage.get={"value":[]}' --reply 'storage.set={}' --reply 'fow.open={"opened":true}' \
  --then later.clear
# → {"result":{"done":true,"message":"後で読むに追加しました"}}
#   host storage.get / storage.set / fow.open(一覧の RRL)、later.clear で surface patch

rasq call post-action-plugin post.action \
  --args '{"token":"tk1","actionId":"copy","post":{"url":"https://x/1","body":"本文です"}}' \
  --reply 'clipboard.set={}'
```

## 端末で試す(rasq-client)

1. `rasq dev` が出す絶対パス(または `dist/post_action_plugin.wasm` の絶対パス)を控える。
2. サイドバー → **WASMプラグイン試験 (dev)** → `.wasm のパスからインポート` に貼って **読み込み**。
3. **スロットに登録**(権限 `fow.widget` / `storage.safe` を許可)。
4. 投稿のアクションバーで、お気に入り / リポスト などのバンドルを開くと最下層にプラグインのボタンが出る
   (届かなかったものは末尾の単体ボタン)。「アクションリストの編集」→「プラグインボタン」で有効 / 無効と閾値を変えられる。
5. 「後で読む」→ 一覧 FOW(行のタップでブラウザ、「後で読むを空にする」でクリア)、「本文をコピー」→ トースト、
   「引用の下書き」→ 選んだアカウントのコンポーザーに文面が入る。

## 掲載(審査)に出す

`rasq build` が出す `dist/sample.post-action-0.1.0.rasqpkg` がそのまま掲載できる形
(zstd tar + `bundle.json` `rasq.bundle/1` + `SHA512SUMS`)。`rasq publish --central <url> --dry-run` で
Central への 3 要求を書き出せる。開発者鍵の署名(`*.sig.json`)はサイドロード用で、リポジトリには入れない。
