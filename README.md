# Den Editor

Rust で実装された TUI テキストエディタです。  
編集、ファイルツリー、統合ターミナル、デバッガを1画面で扱えます。

## 機能

- 検索 / 置換、Undo、保存
- サイドバーでファイル操作（新規ファイル・フォルダ作成）
- 言語別シンタックスハイライト（TOML 設定）
- 統合ターミナル（PTY）
- デバッグ（debugpy / CodeLLDB / dlv の設定例付き）

## インストール

ワンライナー（latest 固定・チェックサム検証あり）:

```bash
# Linux / macOS
curl -fsSL https://raw.githubusercontent.com/K16858/redit/main/install.sh | bash

# Windows (PowerShell)
irm https://raw.githubusercontent.com/K16858/redit/main/install.ps1 | iex
```

初回リリースは `v1.0.0` を想定しています。  
ローカルビルドしたい場合:

```bash
cargo build --release
```

## 起動

```text
den [path]
```

- 引数なし: カレントディレクトリで起動
- ディレクトリ指定: そのディレクトリをワークスペースとして起動
- ファイル指定: そのファイルを開いて起動

### CLI オプション

```text
den --version
den -V
```

- バージョンを表示して終了

## 設定ファイル

- Linux / macOS: `~/.config/den/`
- Windows: `%APPDATA%\den\`
- 例: `docs/examples/config.toml`

## キーバインド（主要）

| 操作                       | キー                                     |
| -------------------------- | ---------------------------------------- |
| 終了（未保存時は確認あり） | Ctrl+Q                                   |
| 保存                       | Ctrl+S                                   |
| 検索 / 置換                | Ctrl+F / Ctrl+H                          |
| サイドバー切替             | Ctrl+B                                   |
| フォーカス切替             | Ctrl+1（エディタ）/ Ctrl+2（ターミナル） |
| ターミナル表示切替         | Ctrl+@                                   |
| 新規ファイル / フォルダ    | Ctrl+N / Ctrl+Shift+N                    |
| 取消                       | Esc                                      |
| デバッグ開始 / 停止        | F5 / Shift+F5                            |
| ブレークポイント           | F9                                       |
| ステップ                   | F10（Over）/ F11（In）/ Shift+F11（Out） |
| 一時停止 / 再起動          | F6 / Shift+F6                            |
| 続行                       | Ctrl+R                                   |
