//! Zero-cost compile-time i18n lookup system with single unified top-level t! macro.

use serde_json::Value;
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Locale {
    #[default]
    Ko,
    En,
    Ja,
}

static CURRENT_LOCALE: AtomicU8 = AtomicU8::new(0);

pub fn set_locale(locale: Locale) {
    let code = match locale {
        Locale::Ko => 0,
        Locale::En => 1,
        Locale::Ja => 2,
    };
    CURRENT_LOCALE.store(code, Ordering::Relaxed);
}

pub fn current_locale() -> Locale {
    match CURRENT_LOCALE.load(Ordering::Relaxed) {
        1 => Locale::En,
        2 => Locale::Ja,
        _ => Locale::Ko,
    }
}

/// Resolves a locale from an explicit language string, falling back to OS detection
/// if the language is `"auto"`, missing, empty, or unparseable.
pub fn resolve_locale(lang: Option<&str>) -> Locale {
    match lang {
        Some("ko") => Locale::Ko,
        Some("en") => Locale::En,
        Some("ja") => Locale::Ja,
        Some("auto") | None | Some("") => match crate::ui::platform::detect_os_language() {
            "ja" => Locale::Ja,
            "en" => Locale::En,
            _ => Locale::Ko,
        },
        _ => match crate::ui::platform::detect_os_language() {
            "ja" => Locale::Ja,
            "en" => Locale::En,
            _ => Locale::Ko,
        },
    }
}

/// Reads the top-level `"language"` key (`"auto"`/`"ko"`/`"en"`/`"ja"`) from merged settings JSON.
pub fn set_locale_from_settings(settings: &Value) {
    let lang = settings.get("language").and_then(Value::as_str);
    set_locale(resolve_locale(lang));
}

/// Zero-cost, zero-reallocation macro-based i18n dispatcher.
/// Selects translation based on current locale without string allocations.
#[macro_export]
macro_rules! t_select {
    // Fully translated arm (Ko / En / Ja all provided).
    (Ko => $ko:expr, En => $en:expr, Ja => $ja:expr) => {
        match $crate::ui::i18n::current_locale() {
            $crate::ui::i18n::Locale::Ko => $ko,
            $crate::ui::i18n::Locale::En => $en,
            $crate::ui::i18n::Locale::Ja => $ja,
        }
    };
    // Transitional arm: Japanese translation not yet authored; falls back to English.
    (Ko => $ko:expr, En => $en:expr) => {
        match $crate::ui::i18n::current_locale() {
            $crate::ui::i18n::Locale::Ko => $ko,
            $crate::ui::i18n::Locale::En | $crate::ui::i18n::Locale::Ja => $en,
        }
    };
}

/// Single top-level SSOT i18n macro for static keys, dynamic formatters, and domain metas.
#[macro_export]
macro_rules! t {
    // 1) Domain Meta Direct Matchers
    (gold = $meta:expr) => {
        match $meta {
            overmax_data::community::sheet_meta::GoldMeta::HalfRandom => $crate::t!("gold-half-random"),
            overmax_data::community::sheet_meta::GoldMeta::MaxRandom => $crate::t!("gold-max-random"),
            overmax_data::community::sheet_meta::GoldMeta::Random => $crate::t!("gold-random"),
            overmax_data::community::sheet_meta::GoldMeta::None => "",
        }
    };
    (assist = $meta:expr) => {
        match $meta {
            overmax_data::community::sheet_meta::AssistMeta::Used => $crate::t!("assist-used"),
            overmax_data::community::sheet_meta::AssistMeta::Caution => $crate::t!("assist-caution"),
            overmax_data::community::sheet_meta::AssistMeta::NotUsed => $crate::t!("assist-not-used"),
            overmax_data::community::sheet_meta::AssistMeta::None => "",
        }
    };

    // 2) Dynamic Format Keys (Arg-based formatting)
    ("candidate-count", n = $n:expr) => {
        $crate::t_select!(
            Ko => format!("후보 {}건", $n),
            En => format!("{} candidates", $n),
            Ja => format!("{}件の候補", $n)
        )
    };
    ("sys-place-achieved", mode = $mode:expr, rank = $rank:expr) => {
        $crate::t_select!(
            Ko => format!("{} TOP {}위 달성!", $mode, $rank),
            En => format!("Achieved TOP {} in {}!", $rank, $mode),
            Ja => format!("{} TOP {}位達成!", $mode, $rank)
        )
    };
    ("sys-upload-cache-error", error = $err:expr) => {
        $crate::t_select!(
            Ko => format!("업로드 OK, 캐시 갱신 오류: {}", $err),
            En => format!("Upload OK, cache error: {}", $err),
            Ja => format!("アップロードOK、キャッシュ更新エラー: {}", $err)
        )
    };
    ("sys-update-prompt-dialog", current = $current:expr, latest = $latest:expr) => {
        $crate::t_select!(
            Ko => format!("새 앱 업데이트가 있습니다.\n\n현재 버전: {}\n최신 버전: {}\n\n지금 업데이트를 진행할까요?", $current, $latest),
            En => format!("A new app update is available.\n\nCurrent version: {}\nLatest version: {}\n\nWould you like to update now?", $current, $latest),
            Ja => format!("新しいアプリのアップデートがあります。\n\n現在のバージョン: {}\n最新バージョン: {}\n\n今すぐ更新しますか?", $current, $latest)
        )
    };
    ("sys-update-error-dialog", error = $err:expr) => {
        $crate::t_select!(
            Ko => format!("자동 패치가 완료되지 않았습니다.\n\n사유: {}", $err),
            En => format!("The automatic update did not complete.\n\nReason: {}", $err),
            Ja => format!("自動パッチが完了しませんでした。\n\n理由: {}", $err)
        )
    };
    ("sys-api-ok-cache-failed", error = $err:expr) => {
        $crate::t_select!(
            Ko => format!("API 조회 OK, 캐시 병합 실패: {}", $err),
            En => format!("API fetch OK, cache merge failed: {}", $err),
            Ja => format!("API取得OK、キャッシュ統合に失敗: {}", $err)
        )
    };
    ("sys-api-and-fallback-failed", api_error = $api_err:expr, fallback_error = $fb_err:expr) => {
        $crate::t_select!(
            Ko => format!("API 실패 ({}), 폴백 캐시 갱신 실패: {}", $api_err, $fb_err),
            En => format!("API failed ({}), fallback cache update failed: {}", $api_err, $fb_err),
            Ja => format!("API失敗 ({}), フォールバックキャッシュ更新に失敗: {}", $api_err, $fb_err)
        )
    };
    ("sys-fallback-cache-failed", error = $err:expr) => {
        $crate::t_select!(
            Ko => format!("폴백 캐시 갱신 실패: {}", $err),
            En => format!("Fallback cache update failed: {}", $err),
            Ja => format!("フォールバックキャッシュ更新に失敗: {}", $err)
        )
    };
    ("settings-eg-hint", domain = $domain:expr) => {
        $crate::t_select!(
            Ko => format!("예: {}", $domain),
            En => format!("e.g. {}", $domain),
            Ja => format!("例: {}", $domain)
        )
    };
    ("sync-filter-btn", icon = $icon:expr) => {
        $crate::t_select!(
            Ko => format!("{} 🔍 필터", $icon),
            En => format!("{} 🔍 Filter", $icon),
            Ja => format!("{} 🔍 フィルター", $icon)
        )
    };
    ("sync-level-range", min = $min:expr, max = $max:expr) => {
        $crate::t_select!(
            Ko => format!("난이도 ({} ~ {})", $min, $max),
            En => format!("Level ({} ~ {})", $min, $max),
            Ja => format!("レベル ({} ~ {})", $min, $max)
        )
    };
    ("rec-patterns-count", has = $has:expr, total = $total:expr) => {
        $crate::t_select!(
            Ko => format!("{}/{}곡", $has, $total),
            En => format!("{}/{} patterns", $has, $total),
            Ja => format!("{}/{}譜面", $has, $total)
        )
    };
    ("overlay-gold-rec-meta", val = $val:expr) => {
        $crate::t_select!(
            Ko => format!("황배:{}", $val),
            En => format!("Recommendation:{}", $val),
            Ja => format!("推奨:{}", $val)
        )
    };
    ("overlay-assist-key-meta", val = $val:expr) => {
        $crate::t_select!(
            Ko => format!("보조:{}", $val),
            En => format!("Assist Key:{}", $val),
            Ja => format!("アシスト:{}", $val)
        )
    };
    ("status-varchive-failed-toast", error = $err:expr) => {
        $crate::t_select!(
            Ko => format!("V-Archive 동기화 실패: {}", $err),
            En => format!("V-Archive sync failed: {}", $err),
            Ja => format!("V-Archive同期に失敗: {}", $err)
        )
    };
    ("sys-shortcut-create-success", path = $path:expr) => {
        $crate::t_select!(
            Ko => format!("앱 메뉴 바로가기를 생성했습니다.\n\n{}", $path),
            En => format!("App menu shortcut created successfully.\n\n{}", $path),
            Ja => format!("アプリメニューショートカットを作成しました。\n\n{}", $path)
        )
    };
    ("sys-shortcut-create-failed", error = $err:expr) => {
        $crate::t_select!(
            Ko => format!("바로가기를 생성하지 못했습니다.\n\n{}", $err),
            En => format!("Failed to create shortcut.\n\n{}", $err),
            Ja => format!("ショートカットを作成できませんでした。\n\n{}", $err)
        )
    };
    ("sys-upload-msg-with-rank", message = $msg:expr, rank_msg = $rank:expr) => {
        $crate::t_select!(
            Ko => format!("{} ({})", $msg, $rank),
            En => format!("{} ({})", $msg, $rank),
            Ja => format!("{} ({})", $msg, $rank)
        )
    };
    ("overlay-recommended-target", target = $target:expr) => {
        $crate::t_select!(
            Ko => format!("🎯 권장 {}", $target),
            En => format!("🎯 Target {}", $target),
            Ja => format!("🎯 推奨 {}", $target)
        )
    };
    ("reco-reason-top50-defend", rank = $rank:expr) => {
        $crate::t_select!(
            Ko => format!("Top-50 수성 방어 타깃 (현재 {}위)", $rank),
            En => format!("Top-50 defense target (Rank #{})", $rank),
            Ja => format!("Top-50防衛ターゲット (現在{}位)", $rank)
        )
    };

    // 3) Static Keys Direct Matchers (Multi-line formatted per key for legibility & DX)
    ("overlay-avg-label") => {
        $crate::t_select!(
            Ko => "평균",
            En => "Avg",
            Ja => "平均"
        )
    };
    ("reco-reason-top50-attack") => {
        $crate::t_select!(
            Ko => "Top-50 컷라인 돌파 추천 타깃",
            En => "Top-50 cutoff breakthrough target",
            Ja => "Top-50カットライン突破ターゲット"
        )
    };
    ("reco-reason-climbing") => {
        $crate::t_select!(
            Ko => "세션 상승 모멘텀 상위 난이도 도전",
            En => "Session upward momentum challenge",
            Ja => "セッション上昇モメンタム挑戦"
        )
    };
    ("reco-reason-recovery") => {
        $crate::t_select!(
            Ko => "세션 회복/손풀기 적정 난이도",
            En => "Session warmup / recovery target",
            Ja => "セッションウォームアップ / リカバリーターゲット"
        )
    };
    ("reco-reason-retry") => {
        $crate::t_select!(
            Ko => "방치된 기록 경신 재도전 추천",
            En => "Stale record improvement retry",
            Ja => "放置記録改善リトライ"
        )
    };
    ("reco-reason-unplayed") => {
        $crate::t_select!(
            Ko => "미플레이 첫 클리어 도전 추천",
            En => "Unplayed first clear challenge",
            Ja => "未プレイ初クリアー挑戦"
        )
    };
    ("status-account-path-missing") => {
        $crate::t_select!(
            Ko => "account.txt 경로 설정 필요",
            En => "account.txt path required",
            Ja => "account.txtパスが必要です"
        )
    };
    ("status-account-parse-failed") => {
        $crate::t_select!(
            Ko => "account.txt 읽기 실패",
            En => "Failed to read account.txt",
            Ja => "account.txtを読み取れませんでした"
        )
    };

    ("gold-half-random") => {
        $crate::t_select!(
            Ko => "핲랜",
            En => "Half Random",
            Ja => "ハーフランダム"
        )
    };
    ("gold-max-random") => {
        $crate::t_select!(
            Ko => "맥랜",
            En => "Max Random",
            Ja => "マックスランダム"
        )
    };
    ("gold-random") => {
        $crate::t_select!(
            Ko => "랜덤",
            En => "Random",
            Ja => "ランダム"
        )
    };
    ("assist-used") => {
        $crate::t_select!(
            Ko => "사용",
            En => "Used",
            Ja => "使用"
        )
    };
    ("assist-caution") => {
        $crate::t_select!(
            Ko => "주의",
            En => "Caution",
            Ja => "注意"
        )
    };
    ("assist-not-used") => {
        $crate::t_select!(
            Ko => "미사용",
            En => "Not Used",
            Ja => "未使用"
        )
    };

    ("settings-title") => {
        $crate::t_select!(
            Ko => "설정",
            En => "Settings",
            Ja => "設定"
        )
    };
    ("settings-overlay-section") => {
        $crate::t_select!(
            Ko => "오버레이 설정",
            En => "Overlay Settings",
            Ja => "オーバーレイ設定"
        )
    };
    ("settings-size") => {
        $crate::t_select!(
            Ko => "크기",
            En => "Size",
            Ja => "サイズ"
        )
    };
    ("settings-opacity") => {
        $crate::t_select!(
            Ko => "투명도",
            En => "Opacity",
            Ja => "不透明度"
        )
    };
    ("settings-lite-mode") => {
        $crate::t_select!(
            Ko => "라이트모드",
            En => "Lite Mode",
            Ja => "ライトモード"
        )
    };
    ("settings-enable") => {
        $crate::t_select!(
            Ko => "활성화",
            En => "Enable",
            Ja => "有効化"
        )
    };
    ("settings-lite-mode-desc") => {
        $crate::t_select!(
            Ko => "추천 숨기기 및 레이아웃 축소",
            En => "Hide recommendations and shrink layout",
            Ja => "推奨を非表示にしてレイアウトを縮小"
        )
    };
    ("settings-snap-position") => {
        $crate::t_select!(
            Ko => "오버레이 고정 위치",
            En => "Overlay Snap Position",
            Ja => "オーバーレイスナップ位置"
        )
    };
    ("settings-top-left") => {
        $crate::t_select!(
            Ko => "좌상단",
            En => "Top-Left",
            Ja => "左上"
        )
    };
    ("settings-top-right") => {
        $crate::t_select!(
            Ko => "우상단",
            En => "Top-Right",
            Ja => "右上"
        )
    };
    ("settings-bottom-left") => {
        $crate::t_select!(
            Ko => "좌하단",
            En => "Bottom-Left",
            Ja => "左下"
        )
    };
    ("settings-bottom-right") => {
        $crate::t_select!(
            Ko => "우하단",
            En => "Bottom-Right",
            Ja => "右下"
        )
    };
    ("settings-manual") => {
        $crate::t_select!(
            Ko => "수동",
            En => "Manual",
            Ja => "手動"
        )
    };
    ("settings-varchive-account") => {
        $crate::t_select!(
            Ko => "V-Archive 계정",
            En => "V-Archive Account",
            Ja => "V-Archiveアカウント"
        )
    };
    ("settings-link-status") => {
        $crate::t_select!(
            Ko => "연동 상태",
            En => "Link Status",
            Ja => "連携状態"
        )
    };
    ("settings-data-sync") => {
        $crate::t_select!(
            Ko => "데이터 동기화",
            En => "Data Sync",
            Ja => "データ同期"
        )
    };
    ("settings-browse") => {
        $crate::t_select!(
            Ko => "찾기",
            En => "Browse",
            Ja => "参照"
        )
    };
    ("settings-storage-section") => {
        $crate::t_select!(
            Ko => "저장소 및 데이터",
            En => "Storage & Data",
            Ja => "ストレージとデータ"
        )
    };
    ("settings-storage-mode") => {
        $crate::t_select!(
            Ko => "실행 모드",
            En => "Runtime Mode",
            Ja => "実行モード"
        )
    };
    ("settings-storage-mode-hint") => {
        $crate::t_select!(
            Ko => "현재 데이터가 저장되는 방식입니다.",
            En => "Current data storage mode.",
            Ja => "現在のデータ保存方式です。"
        )
    };
    ("settings-storage-mode-portable") => {
        $crate::t_select!(
            Ko => "포터블 모드",
            En => "Portable Mode",
            Ja => "ポータブルモード"
        )
    };
    ("settings-storage-mode-installed") => {
        $crate::t_select!(
            Ko => "MSIX / 설치 모드",
            En => "MSIX / Installed Mode",
            Ja => "MSIX / インストールモード"
        )
    };
    ("settings-storage-folder") => {
        $crate::t_select!(
            Ko => "데이터 폴더",
            En => "Data Folder",
            Ja => "データフォルダー"
        )
    };
    ("settings-storage-folder-hint") => {
        $crate::t_select!(
            Ko => "설정(settings.user.json) 및 기록(record.db)이 저장되는 경로입니다.",
            En => "Path where settings (settings.user.json) and history (record.db) are stored.",
            Ja => "設定(settings.user.json)および記録(record.db)が保存される場所です。"
        )
    };
    ("settings-storage-open-folder") => {
        $crate::t_select!(
            Ko => "폴더 열기",
            En => "Open Folder",
            Ja => "フォルダーを開く"
        )
    };
    ("settings-update-section") => {
        $crate::t_select!(
            Ko => "업데이트 설정",
            En => "Update Settings",
            Ja => "アップデート設定"
        )
    };
    ("settings-auto-update") => {
        $crate::t_select!(
            Ko => "자동 업데이트",
            En => "Auto Update",
            Ja => "自動アップデート"
        )
    };
    ("settings-use") => {
        $crate::t_select!(
            Ko => "사용",
            En => "Enable",
            Ja => "有効"
        )
    };
    ("settings-version-info") => {
        $crate::t_select!(
            Ko => "버전 정보",
            En => "Version",
            Ja => "バージョン"
        )
    };
    ("settings-no-steam-account") => {
        $crate::t_select!(
            Ko => "발견된 Steam 계정이 없습니다.",
            En => "No Steam account found.",
            Ja => "Steamアカウントが見つかりません。"
        )
    };
    ("settings-varchive-connect") => {
        $crate::t_select!(
            Ko => "계정 연결",
            En => "Account Connection",
            Ja => "アカウント連携"
        )
    };
    ("settings-varchive-upload") => {
        $crate::t_select!(
            Ko => "업로드 연동",
            En => "Upload Integration",
            Ja => "アップロード連携"
        )
    };
    ("settings-varchive-upload-desc") => {
        $crate::t_select!(
            Ko => "게임 플레이 후 V-Archive로 기록을 전송하기 위해 필요한 연동 설정입니다.",
            En => "Settings required to upload play records to V-Archive.",
            Ja => "プレイ記録をV-Archiveへアップロードするための設定です。"
        )
    };
    ("sync-refresh") => {
        $crate::t_select!(
            Ko => "새로고침",
            En => "Refresh",
            Ja => "更新"
        )
    };
    ("settings-account-path-required") => {
        $crate::t_select!(
            Ko => "업로드를 사용하려면 account.txt 파일 경로를 설정해야 합니다.",
            En => "You must configure the account.txt file path to use upload.",
            Ja => "アップロードには account.txt ファイルパスの設定が必要です。"
        )
    };
    ("settings-tab-general") => {
        $crate::t_select!(
            Ko => "일반",
            En => "General",
            Ja => "一般"
        )
    };
    ("settings-tab-recommend") => {
        $crate::t_select!(
            Ko => "추천",
            En => "Recommendation",
            Ja => "おすすめ"
        )
    };
    ("settings-tab-varchive") => {
        $crate::t_select!(
            Ko => "V-Archive",
            En => "V-Archive",
            Ja => "V-Archive"
        )
    };
    ("settings-tab-advanced") => {
        $crate::t_select!(
            Ko => "고급",
            En => "Advanced",
            Ja => "詳細"
        )
    };
    ("settings-overlay-hint") => {
        $crate::t_select!(
            Ko => "오버레이 크기와 투명도, 위치는 변경 즉시 화면에 실시간 반영됩니다.",
            En => "Overlay size, opacity, and position changes are previewed immediately.",
            Ja => "オーバーレイのサイズ・不透明度・位置の変更はすぐにプレビューされます。"
        )
    };
    ("settings-smart-recommend-hint") => {
        $crate::t_select!(
            Ko => "Top-50 수성/돌파, 모멘텀 가중치 적용",
            En => "Applies Top-50 cutlines, retry gap & momentum",
            Ja => "Top-50カットライン、リトライ格差、モメンタムを反映"
        )
    };
    ("settings-reco-mode-smart") => {
        $crate::t_select!(
            Ko => "스마트",
            En => "Smart",
            Ja => "スマート"
        )
    };
    ("settings-reco-mode-classic") => {
        $crate::t_select!(
            Ko => "클래식",
            En => "Classic",
            Ja => "クラシック"
        )
    };
    ("settings-target-rate-hint") => {
        $crate::t_select!(
            Ko => "권장 레벨 산출 기준 정확도",
            En => "Target accuracy for recommended levels",
            Ja => "推奨レベルの目標精度"
        )
    };
    ("settings-recommend-provider-desc") => {
        $crate::t_select!(
            Ko => "외부 추천 알고리즘 HTTP 서버와 연동합니다.",
            En => "Integrate with an external recommendation HTTP server.",
            Ja => "外部のおすすめHTTPサーバーと連携します。"
        )
    };
    ("settings-debug-window-hint") => {
        $crate::t_select!(
            Ko => "실시간 탐지 수치 및 진단 로그 모니터링",
            En => "Monitor real-time detection metrics and diagnostic logs",
            Ja => "リアルタイム検出メトリクスと診断ログを表示"
        )
    };
    ("settings-ipc-section") => {
        $crate::t_select!(
            Ko => "방송 및 외부 앱 연동",
            En => "Broadcasting & External Apps",
            Ja => "配信・外部アプリ連携"
        )
    };
    ("settings-ipc-desc") => {
        $crate::t_select!(
            Ko => "OBS 방송 화면이나 커뮤니티 위젯에서 현재 곡명과 플레이 결과를 실시간으로 띄울 수 있도록 데이터를 제공합니다.",
            En => "Provides real-time song and result data for OBS overlays and community widgets.",
            Ja => "OBS配信画面や外部ウィジェットで現在の曲名やプレイ結果をリアルタイム表示できるようにデータを共有します。"
        )
    };
    ("settings-ipc-enable") => {
        $crate::t_select!(
            Ko => "실시간 데이터 공유 켜기",
            En => "Enable real-time data sharing",
            Ja => "リアルタイムデータ共有を有効化"
        )
    };
    ("settings-ipc-enable-hint") => {
        $crate::t_select!(
            Ko => "내 컴퓨터(127.0.0.1) 내부에서만 안전하게 동작하며 인터넷 외부로 유출되지 않습니다",
            En => "Runs securely on your local PC (127.0.0.1) only with zero external traffic",
            Ja => "お使いのPC内部(127.0.0.1)でのみ安全に動作し、外部へ送信されることはありません"
        )
    };
    ("settings-ipc-port") => {
        $crate::t_select!(
            Ko => "연결 포트",
            En => "Connection Port",
            Ja => "接続ポート"
        )
    };
    ("settings-ipc-port-hint") => {
        $crate::t_select!(
            Ko => "다른 프로그램과 충돌할 때만 변경하세요 (기본값: 30110)",
            En => "Change only if conflicting with other apps (Default: 30110)",
            Ja => "他のアプリと競合する場合のみ変更してください (デフォルト: 30110)"
        )
    };
    ("settings-ipc-status-running") => {
        $crate::t_select!(
            Ko => "연결 대기 중",
            En => "Ready for connection",
            Ja => "接続待機中"
        )
    };
    ("settings-ipc-status-stopped") => {
        $crate::t_select!(
            Ko => "꺼짐",
            En => "Disabled",
            Ja => "オフ"
        )
    };
    ("settings-capture-engine-hint") => {
        $crate::t_select!(
            Ko => "DXGI: 고성능 / GDI: 호환성",
            En => "DXGI: High Perf / GDI: Compatibility",
            Ja => "DXGI: 高性能 / GDI: 互換性"
        )
    };
    ("settings-protect-overlay-hint") => {
        $crate::t_select!(
            Ko => "화면 캡처 대상에서 오버레이 제외",
            En => "Exclude overlay from screen capture",
            Ja => "オーバーレイを画面キャプチャから除外"
        )
    };
    ("settings-auto-update-hint") => {
        $crate::t_select!(
            Ko => "기동 시 새 버전 감지 및 알림",
            En => "Check for new versions on startup",
            Ja => "起動時に新バージョンを確認"
        )
    };
    ("settings-general") => {
        $crate::t_select!(
            Ko => "일반",
            En => "General",
            Ja => "一般"
        )
    };
    ("settings-language") => {
        $crate::t_select!(
            Ko => "언어",
            En => "Language",
            Ja => "言語"
        )
    };
    ("settings-lang-auto") => {
        $crate::t_select!(
            Ko => "자동",
            En => "Auto",
            Ja => "自動"
        )
    };
    ("settings-language-hint") => {
        $crate::t_select!(
            Ko => "자동 선택 시 OS 언어에 맞춥니다",
            En => "Follows OS language when set to Auto",
            Ja => "自動設定時はOSの言語に合わせます"
        )
    };
    ("settings-recommend-provider") => {
        $crate::t_select!(
            Ko => "추천 Provider",
            En => "Recommend Provider",
            Ja => "おすすめプロバイダー"
        )
    };
    ("settings-recommend-section") => {
        $crate::t_select!(
            Ko => "추천 설정",
            En => "Recommendation",
            Ja => "おすすめ"
        )
    };
    ("settings-smart-recommend") => {
        $crate::t_select!(
            Ko => "스마트 추천",
            En => "Smart Recommendation",
            Ja => "スマートおすすめ"
        )
    };
    ("settings-smart-recommend-desc") => {
        $crate::t_select!(
            Ko => "Top-50 경계, 방치 재도전 및 세션 모멘텀을 반영해 가중 정렬하고 사유 뱃지를 표시합니다.\n비활성화 시 기존 단순 달성률 순서로 정렬됩니다.",
            En => "Weighted sorting by Top-50 boundaries, retry gap, and session momentum with reason badges.\nWhen disabled, sorts by classic achievement rate only.",
            Ja => "Top-50カットライン、リトライ格差、セッションモメンタムによる理由バッジ付きの重み付けソートです。\n無効にすると従来の達成率のみでソートします。"
        )
    };
    ("settings-target-rate") => {
        $crate::t_select!(
            Ko => "권장 기준 레이팅",
            En => "Target Rate",
            Ja => "目標精度"
        )
    };
    ("settings-target-rate-desc") => {
        $crate::t_select!(
            Ko => "오버레이 하단 권장 레벨을 산출할 때 기준이 되는 목표 달성률(정확도)을 설정합니다.\n높게 설정할수록 완벽한 판정을 요구하여 권장 난이도가 낮아지고, 낮게 설정할수록 상위 난이도에 도전하도록 유도합니다.",
            En => "Set target accuracy rate used to derive recommended level on the overlay footer.\nHigher values demand better precision for lower recommended levels; lower values encourage climbing higher difficulties.",
            Ja => "オーバーレイフッターの推奨レベル算出に使う目標精度を設定します。\n高い値ほど低い推奨レベルに高精度を要求し、低い値ほど高難易度への挑戦を促します。"
        )
    };
    ("settings-use-external-provider") => {
        $crate::t_select!(
            Ko => "외부 Provider 사용",
            En => "Use External Provider",
            Ja => "外部プロバイダーを使用"
        )
    };
    ("settings-display-name") => {
        $crate::t_select!(
            Ko => "표시 이름",
            En => "Display Name",
            Ja => "表示名"
        )
    };
    ("settings-overlay-display") => {
        $crate::t_select!(
            Ko => "오버레이 표시",
            En => "Overlay Display",
            Ja => "オーバーレイ表示"
        )
    };
    ("settings-always-show") => {
        $crate::t_select!(
            Ko => "항상 표시",
            En => "Always Show",
            Ja => "常に表示"
        )
    };
    ("settings-always-show-desc") => {
        $crate::t_select!(
            Ko => "게임 구동 중 씬 감지(Unknown) 결과와 상관없이 오버레이를 항상 표시합니다.",
            En => "Keeps the overlay visible at all times, regardless of scene detection (Unknown) results.",
            Ja => "シーン検出結果(Unknown)に関係なく、オーバーレイを常に表示し続けます。"
        )
    };
    ("settings-diagnostics") => {
        $crate::t_select!(
            Ko => "진단 및 디버그",
            En => "Diagnostics & Debug",
            Ja => "診断とデバッグ"
        )
    };
    ("settings-debug-window") => {
        $crate::t_select!(
            Ko => "디버그 창",
            En => "Debug Window",
            Ja => "デバッグウィンドウ"
        )
    };
    ("settings-show-debug-window") => {
        $crate::t_select!(
            Ko => "디버그 모니터링 창 표시",
            En => "Show Debug Monitoring Window",
            Ja => "デバッグ監視ウィンドウを表示"
        )
    };
    ("settings-debug-window-desc") => {
        $crate::t_select!(
            Ko => "실시간 탐지 수치 및 진단 로그를 표출하는 디버그 창을 엽니다.",
            En => "Opens a debug window showing real-time detection metrics and diagnostic logs.",
            Ja => "リアルタイムの検出メトリクスと診断ログを表示するデバッグウィンドウを開きます。"
        )
    };
    ("settings-screen-capture") => {
        $crate::t_select!(
            Ko => "화면 캡처 설정",
            En => "Screen Capture Settings",
            Ja => "画面キャプチャ設定"
        )
    };
    ("settings-protect-overlay") => {
        $crate::t_select!(
            Ko => "캡처 시 오버레이 보호",
            En => "Protect Overlay from Capture",
            Ja => "オーバーレイキャプチャ保護"
        )
    };
    ("settings-prevent-screen-capture") => {
        $crate::t_select!(
            Ko => "화면 캡처 방지",
            En => "Prevent Screen Capture",
            Ja => "画面キャプチャ防止"
        )
    };
    ("settings-protect-overlay-desc") => {
        $crate::t_select!(
            Ko => "해제 시 화면 캡쳐에 잡히는 대신, 특정 영역에 오버레이가 위치하면 곡 인식이 제대로 동작하지 않게 됩니다.",
            En => "Disabling this exposes the overlay to screen capture, but recognition may fail if the overlay covers key areas.",
            Ja => "無効にするとオーバーレイが画面キャプチャに露出しますが、主要領域を覆うと認識が失敗する場合があります。"
        )
    };
    ("settings-find-sync-candidates-btn") => {
        $crate::t_select!(
            Ko => "🔍 동기화 후보 찾기",
            En => "🔍 Find Sync Candidates",
            Ja => "🔍 同期候補を検索"
        )
    };
    ("settings-capture-method-win") => {
        $crate::t_select!(
            Ko => "화면 캡처 설정 (Windows)",
            En => "Screen Capture Method (Windows)",
            Ja => "画面キャプチャ方式 (Windows)"
        )
    };
    ("settings-capture-mode-auto") => {
        $crate::t_select!(
            Ko => "자동 (추천)",
            En => "Auto (Recommended)",
            Ja => "自動 (推奨)"
        )
    };
    ("settings-capture-mode-dxgi") => {
        $crate::t_select!(
            Ko => "DXGI (고성능)",
            En => "DXGI (High Perf)",
            Ja => "DXGI (高性能)"
        )
    };
    ("settings-capture-mode-gdi") => {
        $crate::t_select!(
            Ko => "GDI (호환성)",
            En => "GDI (Compatibility)",
            Ja => "GDI (互換性)"
        )
    };

    ("sync-title") => {
        $crate::t_select!(
            Ko => "동기화",
            En => "Sync",
            Ja => "同期"
        )
    };
    ("sync-desc") => {
        $crate::t_select!(
            Ko => "Steam 계정 기준으로 업로드 후보를 확인합니다.",
            En => "Checks upload candidates for the current Steam account.",
            Ja => "現在のSteamアカウントのアップロード候補を確認します。"
        )
    };
    ("sync-scan") => {
        $crate::t_select!(
            Ko => "스캔",
            En => "Scan",
            Ja => "スキャン"
        )
    };
    ("sync-upload-candidates") => {
        $crate::t_select!(
            Ko => "업로드 후보",
            En => "Upload Candidates",
            Ja => "アップロード候補"
        )
    };
    ("sync-sort-by-change") => {
        $crate::t_select!(
            Ko => "변경순",
            En => "By Change",
            Ja => "変更順"
        )
    };
    ("sync-sort-by-title") => {
        $crate::t_select!(
            Ko => "제목순",
            En => "By Title",
            Ja => "タイトル順"
        )
    };
    ("sync-varchive-sync") => {
        $crate::t_select!(
            Ko => "V-Archive 동기화",
            En => "V-Archive Sync",
            Ja => "V-Archive同期"
        )
    };
    ("sync-register") => {
        $crate::t_select!(
            Ko => "등록",
            En => "Register",
            Ja => "登録"
        )
    };
    ("sync-delete") => {
        $crate::t_select!(
            Ko => "삭제",
            En => "Delete",
            Ja => "削除"
        )
    };
    ("sync-mode") => {
        $crate::t_select!(
            Ko => "모드",
            En => "Mode",
            Ja => "モード"
        )
    };
    ("sync-difficulty") => {
        $crate::t_select!(
            Ko => "난이도",
            En => "Difficulty",
            Ja => "難易度"
        )
    };
    ("sync-max-combo-only") => {
        $crate::t_select!(
            Ko => "맥스콤보 달성만",
            En => "Max Combo achieved only",
            Ja => "Max Combo達成のみ"
        )
    };
    ("sync-exclude-unuploaded") => {
        $crate::t_select!(
            Ko => "미업로드 제외",
            En => "Exclude not uploaded",
            Ja => "未アップロードを除外"
        )
    };
    ("sync-reason-not-registered") => {
        $crate::t_select!(
            Ko => "미등록",
            En => "Not Registered",
            Ja => "未登録"
        )
    };
    ("sync-reset-btn") => {
        $crate::t_select!(
            Ko => "초기화 ↺",
            En => "Reset ↺",
            Ja => "リセット ↺"
        )
    };

    ("app-settings-window") => {
        $crate::t_select!(
            Ko => "Overmax 설정",
            En => "Overmax Settings",
            Ja => "Overmax設定"
        )
    };
    ("app-close") => {
        $crate::t_select!(
            Ko => "닫기",
            En => "Close",
            Ja => "閉じる"
        )
    };
    ("app-save") => {
        $crate::t_select!(
            Ko => "저장",
            En => "Save",
            Ja => "保存"
        )
    };
    ("Linux 앱 실행") => {
        $crate::t_select!(
            Ko => "Linux 앱 실행",
            En => "Linux 앱 실행",
            Ja => "Linuxアプリを実行"
        )
    };
    ("앱 메뉴") => {
        $crate::t_select!(
            Ko => "앱 메뉴",
            En => "앱 메뉴",
            Ja => "アプリメニュー"
        )
    };
    ("바로가기 생성") => {
        $crate::t_select!(
            Ko => "바로가기 생성",
            En => "바로가기 생성",
            Ja => "ショートカット作成"
        )
    };

    ("tray-exit") => {
        $crate::t_select!(
            Ko => "종료",
            En => "Exit",
            Ja => "終了"
        )
    };

    ("overlay-varchive-upload-needed") => {
        $crate::t_select!(
            Ko => "V-Archive 업로드 필요 (클릭하여 즉시 업로드)",
            En => "V-Archive upload needed (click to upload now)",
            Ja => "V-Archiveアップロードが必要です (クリックで今すぐアップロード)"
        )
    };
    ("overlay-varchive-link-needed") => {
        $crate::t_select!(
            Ko => "V-Archive 계정 연동 필요 (설정에서 account.txt 경로를 지정해주세요)",
            En => "V-Archive account link needed (set account.txt path in Settings)",
            Ja => "V-Archiveアカウント連携が必要です (設定で account.txt パスを指定)"
        )
    };
    ("overlay-similar-avg") => {
        $crate::t_select!(
            Ko => "유사 구간 평균",
            En => "Similar Section Average",
            Ja => "類似区間平均"
        )
    };
    ("overlay-keypart-focused") => {
        $crate::t_select!(
            Ko => "키파트 위주 패턴",
            En => "Key-part Focused Pattern",
            Ja => "キーパターン集中型譜面"
        )
    };
    ("overlay-gold-rec") => {
        $crate::t_select!(
            Ko => "황배",
            En => "Recommendation",
            Ja => "推奨"
        )
    };
    ("overlay-assist-key") => {
        $crate::t_select!(
            Ko => "보조",
            En => "Assist Key",
            Ja => "アシストキー"
        )
    };
    ("overlay-no-record") => {
        $crate::t_select!(
            Ko => "기록 없음",
            En => "No Record",
            Ja => "記録なし"
        )
    };

    ("rec-detecting-pattern") => {
        $crate::t_select!(
            Ko => "패턴을 감지하는 중...",
            En => "Detecting pattern...",
            Ja => "パターン解析中..."
        )
    };
    ("rec-no-recommendations") => {
        $crate::t_select!(
            Ko => "추천 결과 없음",
            En => "No recommendations",
            Ja => "おすすめなし"
        )
    };
    ("rec-patterns-suffix") => {
        $crate::t_select!(
            Ko => "개 패턴",
            En => " patterns",
            Ja => " 譜面"
        )
    };
    ("rec-select-song") => {
        $crate::t_select!(
            Ko => "곡을 선택하세요",
            En => "Please select a song",
            Ja => "曲を選択してください"
        )
    };

    ("status-scanning") => {
        $crate::t_select!(
            Ko => "스캔 중…",
            En => "Scanning…",
            Ja => "スキャン中…"
        )
    };
    ("status-updated") => {
        $crate::t_select!(
            Ko => "갱신 완료",
            En => "Updated",
            Ja => "更新済み"
        )
    };
    ("status-registered") => {
        $crate::t_select!(
            Ko => "등록 완료",
            En => "Registered",
            Ja => "登録済み"
        )
    };

    ("sys-already-running") => {
        $crate::t_select!(
            Ko => "이미 Overmax가 실행 중입니다. 기존 인스턴스를 종료한 뒤 다시 실행하세요.",
            En => "Overmax is already running. Please close the existing instance and try again.",
            Ja => "Overmaxはすでに実行中です。既存のインスタンスを終了してから再実行してください。"
        )
    };
    ("sys-reason") => {
        $crate::t_select!(
            Ko => "사유",
            En => "Reason",
            Ja => "理由"
        )
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use overmax_data::community::sheet_meta::{AssistMeta, GoldMeta};

    static TEST_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn test_zero_cost_i18n_translation_and_locale_switch() {
        let _guard = TEST_MUTEX.lock().unwrap();
        set_locale(Locale::Ko);
        assert_eq!(t!("settings-title"), "설정");
        assert_eq!(t!("candidate-count", n = 3), "후보 3건");
        assert_eq!(
            t!("sys-place-achieved", mode = "4B", rank = 5),
            "4B TOP 5위 달성!"
        );
        assert_eq!(
            t!("sys-upload-cache-error", error = "timeout"),
            "업로드 OK, 캐시 갱신 오류: timeout"
        );
        assert_eq!(t!("rec-patterns-count", has = 3, total = 5), "3/5곡");
        assert_eq!(
            t!("sys-shortcut-create-success", path = "/usr/bin/app"),
            "앱 메뉴 바로가기를 생성했습니다.\n\n/usr/bin/app"
        );
        assert_eq!(
            t!(
                "sys-upload-msg-with-rank",
                message = "성공",
                rank_msg = "4B TOP 1위 달성!"
            ),
            "성공 (4B TOP 1위 달성!)"
        );
        assert_eq!(t!(gold = GoldMeta::HalfRandom), "핲랜");
        assert_eq!(t!(assist = AssistMeta::Caution), "주의");

        set_locale(Locale::En);
        assert_eq!(t!("settings-title"), "Settings");
        assert_eq!(t!("candidate-count", n = 3), "3 candidates");
        assert_eq!(
            t!("sys-place-achieved", mode = "4B", rank = 5),
            "Achieved TOP 5 in 4B!"
        );
        assert_eq!(
            t!("sys-upload-cache-error", error = "timeout"),
            "Upload OK, cache error: timeout"
        );
        assert_eq!(t!("rec-patterns-count", has = 3, total = 5), "3/5 patterns");
        assert_eq!(
            t!("sys-shortcut-create-success", path = "/usr/bin/app"),
            "App menu shortcut created successfully.\n\n/usr/bin/app"
        );
        assert_eq!(t!(gold = GoldMeta::HalfRandom), "Half Random");
        assert_eq!(t!(assist = AssistMeta::Caution), "Caution");

        // Japanese locale: authored keys return the Japanese translation.
        set_locale(Locale::Ja);
        assert_eq!(t!("settings-title"), "設定");
        assert_eq!(t!("settings-language"), "言語");
        assert_eq!(t!("candidate-count", n = 3), "3件の候補");

        set_locale(Locale::Ko);
    }

    #[test]
    fn test_resolve_locale_explicit_and_auto() {
        let _guard = TEST_MUTEX.lock().unwrap();
        assert_eq!(resolve_locale(Some("ko")), Locale::Ko);
        assert_eq!(resolve_locale(Some("en")), Locale::En);
        assert_eq!(resolve_locale(Some("ja")), Locale::Ja);

        // "auto", None, and empty should fallback to OS detection (never panic)
        let auto_loc = resolve_locale(Some("auto"));
        let none_loc = resolve_locale(None);
        let empty_loc = resolve_locale(Some(""));
        assert_eq!(auto_loc, none_loc);
        assert_eq!(auto_loc, empty_loc);
        assert!(matches!(auto_loc, Locale::Ko | Locale::En | Locale::Ja));

        // Translation verification for new keys
        set_locale(Locale::Ko);
        assert_eq!(t!("settings-lang-auto"), "자동");
        set_locale(Locale::En);
        assert_eq!(t!("settings-lang-auto"), "Auto");
        set_locale(Locale::Ja);
        assert_eq!(t!("settings-lang-auto"), "自動");

        // Settings JSON integration
        let s_ko = serde_json::json!({ "language": "ko" });
        set_locale_from_settings(&s_ko);
        assert_eq!(current_locale(), Locale::Ko);

        let s_en = serde_json::json!({ "language": "en" });
        set_locale_from_settings(&s_en);
        assert_eq!(current_locale(), Locale::En);

        let s_auto = serde_json::json!({ "language": "auto" });
        set_locale_from_settings(&s_auto);
        assert_eq!(current_locale(), auto_loc);

        // Reset to Ko
        set_locale(Locale::Ko);
    }
}
