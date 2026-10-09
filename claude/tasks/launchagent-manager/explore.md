# Explore: launchagent-manager

グリーンフィールド（既存コードなし）。実機環境調査の結果。

## A. 事実

### 1. ツールチェイン
- node v22.12.0 / npm 10.9.0 / cargo・rustc 1.99.0 / macOS 26.3 / uid=501 / `/bin/bash` 3.2.57
- Xcode あり。`tauri-cli` 未インストール（`npx @tauri-apps/cli` で使用可）。npm・crates.io 到達可
- バージョン: `@tauri-apps/cli` 2.12.1、`@tauri-apps/api` 2.12.2、plist crate 1.10.1、`@digital-go-jp/design-tokens` 2.0.1
- Tauri v2 手書き最小構成で `cargo check` 通過確認済み:
  - `src-tauri/Cargo.toml`（`[lib] name="app_lib", crate-type=["staticlib","cdylib","rlib"]`、build-deps `tauri-build="2"`、deps `tauri="2"`）
  - `build.rs` = `tauri_build::build()`
  - `tauri.conf.json`: `$schema`, productName, version, identifier, `build.frontendDist: "../dist"`, `app.windows`, `app.security.csp: null`, `bundle.active: false`（開発時は `devUrl` / `beforeDevCommand` も）
  - `capabilities/default.json`: `{"identifier":"default","windows":["main"],"permissions":["core:default"]}`
  - `src/lib.rs`, `src/main.rs`（`app_lib::run()`）
  - **`src-tauri/icons/icon.png` 必須**（`generate_context!` が開こうとする。RGBA 32x32 PNG 1枚で可）
  - `frontendDist` のディレクトリ（`dist/index.html`）が存在する必要あり

### 2. launchctl 出力サンプル（フィクスチャ用）
ロード済みジョブ（`launchctl print gui/501/com.openai.atlas.update-helper` 抜粋）:
```
gui/501/com.openai.atlas.update-helper = {
	active count = 0
	path = /Users/george/Library/LaunchAgents/com.openai.atlas.update-helper.plist
	type = LaunchAgent
	state = not running

	program = /Applications/ChatGPT Atlas.app/Contents/Library/AtlasUpdateHelper
	arguments = {
		/Applications/ChatGPT Atlas.app/Contents/Library/AtlasUpdateHelper
	}

	stdout path = /Users/george/Library/Logs/AtlasUpdateHelper.log
	stderr path = /Users/george/Library/Logs/AtlasUpdateHelper.log
	default environment = {
		PATH => /usr/bin:/bin:/usr/sbin:/sbin
	}

	domain = gui/501 [100002]
	asid = 100002
	minimum runtime = 10
	exit timeout = 5
	runs = 684
	last exit code = 0

	resource coalition = {
		ID = 1234
		type = resource
		state = active
	}
	spawn type = daemon (3)
	run interval = 3600 seconds
	job state = exited

	properties = runatload | inferred program | managed LWCR | has LWCR
}
```
実行中ジョブの抜粋:
```
	state = running
	runs = 1
	pid = 83124
	immediate reason = ipc (socket)
	last exit code = (never exited)
```
- `pid` 行は実行中のみ。`last exit code = (never exited)` の非数値あり
- ネストした `state = active` が出るので **トップレベル（タブ1つ）の行のみ** 採る
- 未登録: stderr `Bad request.\nCould not find service "com.nonexistent.zzz" in domain for user gui: 501`、終了コード **113**

`launchctl print-disabled gui/501`:
```

	disabled services = {
		"com.docker.helper" => enabled
		"com.apple.Siri.agent" => disabled
	}
```
記載なしは有効扱い。

`launchctl list`: `PID\tStatus\tLabel`（PID 未実行は `-`）。

### 3. 引数形式とエラー
```
bootstrap gui/<uid> <plist-path>
bootout   gui/<uid>/<label>
enable    gui/<uid>/<label>
disable   gui/<uid>/<label>
kickstart gui/<uid>/<label>
```
- 未登録 bootout: `Boot-out failed: 3: No such process`、終了コード 3 → 成功扱いにする
- bootstrap でファイル無し: `Bootstrap failed: 5: Input/output error`、終了コード 5
- kickstart 未登録: 終了コード 113
- enable/disable は未知 label でも 0（override DB に残る）

### 4. plist crate 1.10.1
- `Value::from_file` / `from_reader` で XML・バイナリ自動判別。非 plist は Err（panic しない）
- `Dictionary` は挿入順保持（indexmap）。未知キー保持可
- 書き出し: `Value::to_file_xml` / `to_writer_xml`

### 5. DADS
- `@digital-go-jp/design-tokens` 2.0.1: `dist/tokens.css`（`:root` CSS 変数）
- 主要: `--color-primitive-blue-900 #0017c1`、blue-50 `#e8f1fe`、blue-1000 `#00118f`、gray-50 `#f2f2f2`、gray-200 `#ccc`、gray-536 `#767676`、gray-900 `#1a1a1a`、success green-800 `#197a4b`、error red-900 `#ce0000`、warning orange-2 `#c74700`
- `--font-family-sans: 'Noto Sans JP', -apple-system, ...`、角丸 `--border-radius-*`、影 `--elevation-*`
- spacing トークン・コンポーネント CSS は無し → 自作（8px 系）。フォーカスリング黄色 `#ffd43d`、ボタン角丸 8px

### 6. 実行環境
- launchd の既定 PATH は `/usr/bin:/bin:/usr/sbin:/sbin` のみ
- Tauri v2 は既定でサンドボックス無し。Rust から `std::process::Command` と `~/Library` 読み書きは追加権限不要。自作コマンドは `core:default` で invoke 可

## B. 推奨方針

- **状態モデル**: loaded（`launchctl list` 一括 / 詳細で `print`）、disabled（`print-disabled` 1回）、PID。UI は「有効 = loaded かつ not disabled」「無効 = それ以外」。無効化 = `bootout` + `disable`、有効化 = `enable` → `bootstrap`
- **print パース**: `^\t(state|runs|pid|last exit code) = (.*)$` のトップレベル限定。取れない項目は None（「不明」）。113 + "Could not find service" → 未登録、その他失敗 → 取得失敗として stderr 表示
- **失敗時方針**: 追加は bootstrap 失敗で plist 削除。編集は旧内容をメモリ保持し失敗時に書き戻し+再 bootstrap（ベストエフォート）。未登録 bootout（3）は成功扱い。エラーには終了コードと stderr
- **ラッパー**: `~/Library/Application Support/<app>/bin/run-job.sh`（起動時に書き出し、755）。`#!/bin/bash`、`ProgramArguments=[wrapper, label, command]`、`/bin/bash -c "$cmd"; rc=$?` → 記録 → `exit $rc`。記録は `runs/<Label>.json`（開始時に `finished_at:null` で書き、終了時に更新、tmp→mv）。`date -u +%Y-%m-%dT%H:%M:%SZ`。bash 3.2 互換
- **PATH**: `zsh -lc` は非推奨。PATH は `EnvironmentVariables`（標準キー）で `/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin` を自動設定（既存値があれば触らない）
- **書き込み**: 同ディレクトリの tmp → `plutil -lint`（CommandRunner 経由）→ rename。Label は `[A-Za-z0-9._-]`、`.`/`..`/先頭 `.` も拒否、書き込み先が LaunchAgents 直下か確認
- **trait**: `CommandRunner::run(program, args) -> Output{status, stdout, stderr}`（モックは呼び出し記録）、`FileSystem`（read/write_atomic/remove/list/mtime/create_dir_all/exists）、`Clock`
- **構成**: Rust モジュール plist_model / schedule / validate / launchctl / service / wrapper / fs / cmd / commands。フロントは Vite + React + TS + Vitest + RTL（`vi.mock('@tauri-apps/api/core')`）
- 実 launchd テストは `#[ignore]`

## 未確認
- `tauri dev` / `tauri build` のフルビルドは未実施
- 既登録 label への bootstrap のエラー文言、disabled ジョブへの bootstrap 挙動
- StartCalendarInterval ジョブの print 出力サンプル
