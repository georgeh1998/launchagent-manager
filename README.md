# LaunchAgent Manager

`~/Library/LaunchAgents` にある定期実行ジョブ（.plist）を一覧・詳細表示・追加・編集・削除できる Mac 用デスクトップアプリです。
plist の手編集や `launchctl` の手打ちをせずに「何が登録されているか / いつ動くか / 最後にいつ動いて成功したか」を確認できます。

Tauri v2 + React + TypeScript（バックエンドは Rust）で作っています。

## 必要環境

- macOS
- Node.js 22 以降
- Rust（stable）

## セットアップと起動

```bash
npm install
npx tauri dev
```

`npx tauri dev` は開発用の起動方法です。開発サーバーが動いている間だけ使えます。普段使いのアプリとして使う場合は、次の「Mac アプリとしてビルド・インストール」の手順で `.app` を作ります。

## Mac アプリとしてビルド・インストール

### ビルド

```bash
npm run tauri build
```

次のファイルが作られます。

| ファイル | 場所 |
|---|---|
| アプリ本体 | `src-tauri/target/release/bundle/macos/LaunchAgent Manager.app` |
| インストーラ | `src-tauri/target/release/bundle/dmg/LaunchAgent Manager_<バージョン>_aarch64.dmg` |

### インストール

`LaunchAgent Manager.app` を Finder で `/Applications` にドラッグします。コマンドで行う場合は次のとおりです。

```bash
cp -R "src-tauri/target/release/bundle/macos/LaunchAgent Manager.app" /Applications/
```

インストール後は Launchpad や Spotlight から起動できます。開発サーバーは必要ありません。

### コード変更後に反映する

ビルドしただけでは `/Applications` のアプリは更新されません。次の手順で入れ替えます。

1. 起動中のアプリを終了する（⌘Q）
2. `npm run tauri build` でビルドし直す
3. 古いアプリを削除して、新しいアプリをコピーする

```bash
rm -rf "/Applications/LaunchAgent Manager.app"
cp -R "src-tauri/target/release/bundle/macos/LaunchAgent Manager.app" /Applications/
```

古いアプリを消さずに `cp -R` すると、古いファイルが残ることがあります。必ず削除してからコピーしてください。

### アイコンの変更

1024×1024 の PNG（背景透過）を用意して、次のコマンドで各サイズのアイコンを生成します。生成後にビルドし直してください。

```bash
npx tauri icon path/to/icon.png
```

`src-tauri/icons/` に `icon.icns` などが出力されます。Android・iOS 用のファイルも生成されますが、このアプリでは使いません。

### うまく動かないとき

- **「開発元を確認できない」と表示されて開けない**: コード署名をしていないためです。Finder でアプリを右クリックして「開く」を選ぶか、次のコマンドを実行します。

  ```bash
  xattr -cr "/Applications/LaunchAgent Manager.app"
  ```

- **アイコンが古いまま表示される**: Dock のアイコンキャッシュが残っています。`killall Dock` を実行してください。

## テスト

```bash
npm run build          # Rust のビルドに dist/ が必要なため先に実行する
npx vitest run         # フロントエンド
(cd src-tauri && cargo test)
```

実際の launchd を使うテストはデフォルトでは実行されません。手動で実行する場合は次のコマンドを使います（一時ジョブ `dev.georgeh.lam-test` を作成して削除します）。

```bash
(cd src-tauri && cargo test -- --ignored)
```

## 使い方

### 一覧と詳細

- 左側にジョブ一覧、右側に選択したジョブの詳細が表示されます。
- 絞り込み欄に `com.georgeh.` のように入力すると、部分一致で絞り込みます。「本アプリ管理のみ」で本アプリが作ったジョブだけを表示します。
- 詳細には、状態（登録済みか・無効化されているか・PID）、スケジュール、実行コマンド、ログのパス、最終実行が表示されます。
  - 本アプリで作ったジョブ: 最終実行の開始・終了日時、終了コード、成功/失敗を表示します。
  - それ以外のジョブ: `launchctl print` の実行回数と最終終了コード、ログの更新日時を「推定」として表示します。

### 追加・編集

「新規作成」から次の 3 項目を入力します。

| 項目 | 内容 |
|---|---|
| 名前 | Label。英数字と `. _ -` のみ使えます。ファイル名は `<名前>.plist` になります |
| 実行するコマンド | 1 行の bash コマンド |
| スケジュール | 毎日 / 毎週（曜日は複数選択可）/ 毎月（日付）/ 一定間隔（N 分・N 時間ごと） |

保存すると plist を書き込み、`launchctl bootstrap` で登録します。
編集・削除できるのは本アプリで作ったジョブだけです。それ以外のジョブは閲覧、有効/無効の切り替え、今すぐ実行だけができます。

### その他の操作

- **有効にする / 無効にする**: plist は残したまま、`launchctl enable` + `bootstrap` / `bootout` + `disable` で切り替えます。
- **今すぐ実行**: `launchctl kickstart` を実行します。
- **削除**: 確認ダイアログの後、登録を解除して plist を削除します。ログと実行記録は残ります。

## 生成されるファイル

| ファイル | 場所 |
|---|---|
| ジョブ定義 | `~/Library/LaunchAgents/<名前>.plist` |
| 標準出力 / 標準エラー | `~/Library/Logs/<名前>/stdout.log`, `stderr.log` |
| ラッパースクリプト | `~/Library/Application Support/launchagent-manager/bin/run-job.sh` |
| 実行記録 | `~/Library/Application Support/launchagent-manager/runs/<名前>.json` |

本アプリで作ったジョブは、ラッパースクリプト経由でコマンドを実行します（`ProgramArguments = [ラッパー, 名前, コマンド]`）。ラッパーが開始・終了時刻と終了コードを実行記録に書き込みます。

## 注意点

- launchd から起動されるジョブの PATH は最小限なので、作成時に `EnvironmentVariables` の `PATH` に `/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin` を自動で設定します。
- 毎月 29〜31 日を指定すると、その日がない月には実行されません。
- 60 分ごとのような 1 時間単位の間隔は、編集画面では「1 時間ごと」として表示されます。
- 実行中に強制終了されると、実行記録は「実行中（または中断）」のまま残ります。
- `launchctl` の操作に失敗した場合は、終了コードと stderr を表示し、可能な限り変更前の状態に戻します。
