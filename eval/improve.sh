#!/usr/bin/env bash
# eval/improve.sh — ゆきねの行動改善ループ
#
# 使い方:
#   cd /home/mizushima/repo/familiar-gui
#   bash eval/improve.sh           # 1サイクル実行
#   bash eval/improve.sh --loop    # 無限ループ
#   bash eval/improve.sh --dry-run # claude -p を呼ばず trace だけ確認
#
# 前提:
#   - claude CLI がパスに存在すること（MAX プランで追加コスト¥0）
#   - cargo build --bin dump_prompt が通ること

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"
TRACES_FILE="/tmp/familiar_eval_traces.log"
LOOP=false
DRY_RUN=false

for arg in "$@"; do
  case $arg in
    --loop)    LOOP=true ;;
    --dry-run) DRY_RUN=true ;;
  esac
done

run_cycle() {
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo "🔄  改善サイクル開始: $(date '+%Y-%m-%d %H:%M:%S')"
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

  # 1. 現在のシステムプロンプトを取得
  echo "📋 システムプロンプトを取得中..."
  cd "$REPO_DIR/src-tauri"
  SYSTEM_PROMPT=$(cargo run --bin dump_prompt 2>/dev/null)
  cd "$REPO_DIR"

  # 2. 各シナリオでエージェントの行動を記録
  echo "" > "$TRACES_FILE"
  for scenario in "$SCRIPT_DIR"/scenarios/*.txt; do
    name=$(basename "$scenario" .txt)
    echo "🎬 シナリオ: $name"
    echo "════════════════════════════════" >> "$TRACES_FILE"
    echo "## $name" >> "$TRACES_FILE"
    echo "### User Input:" >> "$TRACES_FILE"
    cat "$scenario" >> "$TRACES_FILE"
    echo "" >> "$TRACES_FILE"
    echo "### Agent Behavior Trace:" >> "$TRACES_FILE"

    if [ "$DRY_RUN" = false ]; then
      # claude -p をエージェントとして実行
      # エージェントは実際のツールを持たないので「何を呼ぶか」をシミュレートさせる
      claude -p \
        --model claude-haiku-4-5-20251001 \
        --system "$SYSTEM_PROMPT

[EVAL MODE] 実際のツール実行環境はありません。
各ステップで何のツールを呼ぶか、どんな引数で、なぜそう判断したかを以下の形式で出力してください：

THINK: <内部推論>
CALL: <tool_name>(<args>)
OBSERVE: <想定されるツールの返り値>
...
DONE: <最終判断>" \
        "$(cat "$scenario")" >> "$TRACES_FILE" 2>&1 || true
    else
      echo "(dry-run: claude -p skipped)" >> "$TRACES_FILE"
    fi

    echo "" >> "$TRACES_FILE"
  done

  echo ""
  echo "📊 トレース収集完了 → $TRACES_FILE"

  if [ "$DRY_RUN" = true ]; then
    echo "(dry-run モード: 評価ステップをスキップ)"
    cat "$TRACES_FILE"
    return
  fi

  # 3. ここねが評価してagent.rsを改善
  echo ""
  echo "🧠 ここねが評価・改善中..."
  echo ""

  cat "$SCRIPT_DIR/SUPERVISOR_PROMPT.md" "$TRACES_FILE" | \
    claude -p \
      --model claude-sonnet-4-6

  # 4. テストが通るか確認
  echo ""
  echo "🧪 テスト実行中..."
  cd "$REPO_DIR/src-tauri"
  if cargo test 2>&1 | tail -3; then
    echo "✅ テストパス"
    cd "$REPO_DIR"

    # 5. 変更があればコミット
    if git diff --quiet src-tauri/src/agent.rs; then
      echo "ℹ️  agent.rs に変更なし（スキップ）"
    else
      git add src-tauri/src/agent.rs
      git commit -m "auto(eval): improve agent behavior via supervisor loop"
      echo "✅ コミット完了"
    fi
  else
    echo "❌ テスト失敗 — 変更を取り消し"
    git checkout src-tauri/src/agent.rs
    cd "$REPO_DIR"
  fi

  echo ""
  echo "✨ サイクル完了"
}

if [ "$LOOP" = true ]; then
  while true; do
    run_cycle
    echo "⏳ 次のサイクルまで30秒待機..."
    sleep 30
  done
else
  run_cycle
fi
