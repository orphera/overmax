use super::*;
use overmax_data::community::sheet_meta::{AssistMeta, GoldMeta};

#[test]
fn test_zero_cost_i18n_translation_and_locale_switch() {
    let _guard = LOCALE_TEST_LOCK.lock().unwrap();
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
    let _guard = LOCALE_TEST_LOCK.lock().unwrap();
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
