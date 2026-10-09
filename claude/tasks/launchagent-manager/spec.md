# launchagent-manager

## やりたいこと
`~/Library/LaunchAgents` にある定期実行ジョブ（.plist）を、一覧・詳細表示・追加・編集・削除できる Mac 用デスクトップアプリを作る。
plist の手編集や `launchctl` の手打ちをせずに「何が登録されているか / いつ動くか / 最後にいつ動いて成功したか」を1画面で確認できるようにする。

## 前提・技術方針
- **技術スタック**: Tauri v2 + React + TypeScript（バックエンドは Rust）
- **対象**: `~/Library/LaunchAgents`（ユーザー領域）のみ。`/Library/LaunchAgents`・`/Library/LaunchDaemons`・`/System/...` は対象外
- **launchd ドメイン**: `gui/<uid>`
- **UI**: シンプルな2ペイン構成。デザインは デジタル庁デザインシステム（DADS, https://design.digital.go.jp/dads/）の色・タイポグラフィ・スペーシング・コンポーネント（ボタン、テキスト入力、セレクト、チェックボックス、ダイアログ、ステータスラベル）に寄せる
- **テストしやすさ（必須）**: 外部への副作用はすべて抽象化（Rust の trait）の裏に置き、テストではモック/フェイクに差し替えられるようにする
  - `FileSystem`: plist の読み込み・書き込み・削除・一覧、ログの mtime/末尾取得
  - `CommandRunner`: `launchctl` 等の外部コマンド実行（引数と戻り値を記録・差し替え可能にする）
  - `Clock`: 現在時刻（必要なら）
  - パス（LaunchAgents ディレクトリ、ラッパー/実行記録の保存先、uid）は設定として注入し、テストでは一時ディレクトリを使う
- 動作確認は単体テストと結合テスト（モック利用）の結果で判断する。実際の launchd に触れるテストはデフォルトでは実行しない（必要なら `#[ignore]` 付きで手動実行）

## 機能要件

### 1. 閲覧機能
- **画面構成**: 左ペイン = ジョブ一覧、右ペイン = 選択中ジョブの詳細
- **一覧**
  - `~/Library/LaunchAgents/*.plist` を全件表示（表示名は Label。Label が無い/壊れている場合はファイル名＋エラー表示）
  - 各行に有効/無効のステータスラベルを表示
  - **絞り込み**: テキストでの部分一致検索（例: `com.georgeh.`）。初期値は空（全件表示）
  - 「本アプリ管理のみ」の切り替え（任意）
  - 再読み込みボタン
- **詳細**
  - Label / plist のファイルパス
  - **状態**: launchd に登録済みか（loaded）、無効化されているか（disabled）、実行中か（PID）
  - **スケジュール**: 統一フォーマットの日本語で表示
    - `毎日 09:00` / `毎週 月・水・金 09:00` / `毎月 1日 09:00` / `30分ごと` / `毎時 15分` / `ログイン時に実行`（RunAtLoad）
    - `StartCalendarInterval` が配列で複数ある場合は全件を列挙
    - 省略されたキー（ワイルドカード）や表現しきれない組み合わせは、「月=*, 日=1, 時=9, 分=0」のような生の値でフォールバック表示
  - **実行コマンド**: `Program` / `ProgramArguments` をつなげて表示（本アプリ管理のジョブはユーザーが入力したコマンドをそのまま表示）
  - **ログのパス**: `StandardOutPath` / `StandardErrorPath`（未設定ならその旨を表示）
  - **最終実行**
    - 本アプリ管理のジョブ: ラッパーが記録した開始日時・終了日時・終了コード・成功/失敗
    - それ以外のジョブ: `launchctl print` の `last exit code` と `runs`、ログファイルの更新日時を「推定」と明記して表示

### 2. 追加・編集・削除
- **入力項目は3つだけ**
  - 名前（Label。ファイル名は `<Label>.plist`）
  - 実行するコマンド（1行の bash コマンド文字列）
  - スケジュール: 毎日 / 毎週（曜日は複数選択可）/ 毎月（日付指定）/ N分・N時間ごと ＋ 時刻
- **自動で設定する項目**（ユーザーは指定しない）
  - ラッパー経由で実行する `ProgramArguments`
  - ログ・実行記録はジョブごとに別ファイル（ディレクトリは自動作成）
    - 標準出力: `~/Library/Logs/<Label>/stdout.log`（`StandardOutPath`）
    - 標準エラー: `~/Library/Logs/<Label>/stderr.log`（`StandardErrorPath`）
    - 実行記録: `~/Library/Application Support/<app>/runs/<Label>.json`（ラッパーが書く）
- **本アプリ管理ジョブの判定**: `ProgramArguments[0]` が本アプリのラッパースクリプトの絶対パスと一致するものを本アプリ管理とみなす
  - `ProgramArguments` = `[<ラッパーパス>, <Label>, <ユーザー入力コマンド>]`
  - 編集時のコマンドは `ProgramArguments[2]` から復元する
  - 独自キーは plist に追加しない。管理対象一覧の別ファイルも持たない（plist と内容がずれるため）
- **追加**: plist 生成 → バリデーション → 書き込み → `launchctl bootstrap` で登録
- **編集**: 既存の値をフォームに読み込み、同じ3項目だけを編集可能にする
  - 保存時は `bootout` → 書き換え → `bootstrap`
  - 名前（Label）を変えた場合は旧 plist の bootout・削除と新 plist の作成・bootstrap を行う
  - 3項目以外の既存キーは消さずに残す
  - 編集できるのは本アプリ管理のジョブのみ。それ以外のジョブは閲覧専用
- **削除**: 確認ダイアログ → `bootout` → plist 削除（ログファイルと実行記録は残す）。本アプリ管理外のジョブは削除不可
- **有効/無効の切替**: plist は残したまま `bootstrap` / `bootout`（必要に応じて `enable` / `disable`）で切り替える
- **今すぐ実行**: `launchctl kickstart gui/<uid>/<Label>`
- **バリデーション**
  - Label: 空不可、使える文字は `[A-Za-z0-9._-]` のみ、既存ジョブとの重複不可（編集中の自分自身は除く）
  - コマンド: 空不可
  - 時刻: 0–23 時 / 0–59 分、曜日は1つ以上、日付は 1–31（29〜31日はその日がない月に実行されない旨を注意表示）、間隔は1分以上
- **エラー処理**: `launchctl` が失敗したら終了コードと stderr を UI に表示し、半端な状態を残さない（例: bootstrap 失敗時に plist を残すかどうかの方針を決めて統一する）

## テスト観点

### A. plist の解析（純粋関数・Rust）
- XML 形式とバイナリ形式の両方を読める
- `Program` のみ / `ProgramArguments` のみ / 両方 のジョブ
- `StartCalendarInterval` が辞書の場合 / 配列の場合、`StartInterval`、`RunAtLoad` のみ、スケジュール指定なし
- Label 欠落、壊れた XML、plist 以外のファイル（`.DS_Store` 等）が混ざっていても一覧全体は落ちない

### B. スケジュール ⇔ 表示文字列 / plist の変換（純粋関数）
- 毎日・毎週（単一/複数曜日、Weekday 0 と 7 はどちらも日曜）・毎月・N分/時間ごと・毎時 XX分 の表示
- ワイルドカード（Hour 省略など）や想定外の組み合わせのフォールバック表示
- フォーム入力 → plist 辞書 → フォーム入力 の往復で値が変わらない（round-trip）
- 境界値: 0:00 / 23:59 / 31日 / 1分間隔

### C. バリデーション
- Label の文字種・空・重複（編集時の自分自身は重複扱いしない）
- 時刻・日付・曜日・間隔の範囲外
- コマンドに空白・クォート・`$`・改行・日本語が含まれる場合に正しくエスケープされて plist に入る（往復で一致する）

### D. launchctl 出力の解析
- `launchctl print gui/<uid>/<label>` の実際の出力をフィクスチャ化し、state / pid / runs / last exit code を抽出できる
- 未登録時のエラー出力（"Could not find service" 等）を「未登録」と判定できる
- `launchctl print-disabled gui/<uid>` の解析

### E. ジョブ操作サービス（FileSystem / CommandRunner をモック）
- 追加: 正しいパスに正しい内容の plist を書き、`bootstrap gui/<uid> <path>` を正しい引数で1回呼ぶ
- 編集（Label 変更なし）: `bootout` → 書き込み → `bootstrap` の順番で呼び、3項目以外のキーを保持している
- 編集（Label 変更あり）: 旧 bootout → 旧ファイル削除 → 新ファイル書き込み → 新 bootstrap
- 削除: `bootout` → ファイル削除。ログファイルには触れない
- 有効/無効・今すぐ実行: 期待どおりの launchctl 引数になる
- 失敗系: `bootstrap` / `bootout` の失敗、書き込み失敗、未登録ジョブの bootout（エラーにしない）などでの最終状態とエラー通知
- 本アプリ管理外のジョブに編集・削除を要求すると拒否される
- 本アプリ管理の判定: `ProgramArguments[0]` がラッパーパスと一致→管理対象、別パス・`Program` のみ・`ProgramArguments` が空→管理外
- 追加時に `~/Library/Logs/<Label>/` が作られ、stdout/stderr のパスがジョブごとに異なる
- 対象ディレクトリの外のパス（`../` を含む Label 等）には書き込まない

### F. ラッパー（実行記録）
- 成功時/失敗時に開始・終了時刻と終了コードが記録される
- コマンドの終了コードがそのまま launchd に返る
- 記録ファイルが無い（まだ一度も実行されていない）場合の表示

### G. フロントエンド（Vitest + React Testing Library。Tauri の `invoke` はモック）
- 一覧表示、絞り込み（初期値は空で全件表示）、選択で詳細が切り替わる
- 詳細の各項目の表示（未設定・推定値・エラー表示を含む）
- フォームのバリデーションメッセージ、保存/削除時に呼ぶコマンドと引数
- 本アプリ管理外のジョブでは編集・削除ボタンが出ない（または無効）
- 削除確認ダイアログでキャンセルすると何も呼ばない

## 調べてほしいこと
- **有効/無効の判定方法**: 「loaded（bootstrap 済み）」と「disabled（override DB）」の違いと、UI 上でどう1つの状態にまとめるか。`launchctl print` / `print-disabled` / `list` のどれを使うか
- **`launchctl print` の出力の安定性**: 機械向けの出力ではないため、macOS のバージョン差やパース失敗時の扱い
- **ラッパーの方式**
  - ラッパースクリプトの置き場所（例: `~/Library/Application Support/<app>/bin/run-job.sh`）と、実行記録の形式・保存先
  - ラッパーにコマンド文字列を1引数で渡し、`bash -c` 等で実行する場合のエスケープ・終了コードの受け渡し
- **実行時のシェル環境**: launchd から起動されると PATH 等が最小限になる。`/bin/zsh -lc` のようにログインシェル経由にするか、PATH を `EnvironmentVariables` で渡すか
- **Tauri v2 の権限**: Rust 側から `std::process::Command` で launchctl を呼ぶ構成で問題ないか、`~/Library` 配下を読み書きするときのサンドボックス・署名・権限の要否
- **Rust の plist 処理**: `plist` crate でのバイナリ/XML 読み込みと、XML 書き出し時に未知キーを保持できるか
- **DADS の取り込み方**: デザイントークン（npm パッケージや CSS 変数の有無）、使う色・フォント（Noto Sans JP）・コンポーネント仕様
- **plist の書き込みの安全性**: 一時ファイルへの書き込み → rename による原子的な更新、`plutil -lint` による事前検証

## スコープ外
- `/Library/LaunchAgents`、`/Library/LaunchDaemons`、システム領域のジョブ
- 本アプリ管理外のジョブの編集・削除（閲覧・有効/無効・今すぐ実行のみ）
- 3項目以外の plist キー（`EnvironmentVariables`、`WorkingDirectory`、`KeepAlive`、`WatchPaths` など）の編集 UI
- ログの末尾表示・ログビューア（パスの表示のみ）
- 実行履歴の一覧（最終実行1件のみ表示）
- 通知（失敗時のプッシュ通知など）
- 配布用の署名・公証・自動アップデート
- 実際の launchd を使う E2E テストの自動化
