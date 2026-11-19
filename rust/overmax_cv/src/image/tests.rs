use super::*;

#[test]
fn test_match_character_perfect_match() {
    // 16x32 pattern: vertical bar
    let mut mask = [0u8; 16 * 32];
    for y in 0..32 {
        for x in 6..10 {
            mask[y * 16 + x] = 1;
        }
    }
    let template = CvTemplate {
        char_val: '1',
        width: 16,
        height: 32,
        mask: &mask,
    };

    let result = match_character(&mask, 16, 32, &[template]);
    assert!(result.is_some());
    let (ch, score) = result.unwrap();
    assert_eq!(ch, '1');
    assert!((score - 1.0).abs() < 1e-5);
}

#[test]
fn test_match_character_rescaled() {
    // 16x32 pattern scaled down to 8x16 input
    let mut mask = [0u8; 16 * 32];
    for y in 0..32 {
        for x in 6..10 {
            mask[y * 16 + x] = 1;
        }
    }
    let template = CvTemplate {
        char_val: '1',
        width: 16,
        height: 32,
        mask: &mask,
    };

    let mut input_8x16 = [0u8; 8 * 16];
    resize_binary_nearest_into(&mask, 16, 32, &mut input_8x16, 8, 16);
    let result = match_character(&input_8x16, 8, 16, &[template]);
    assert!(result.is_some());
    let (ch, score) = result.unwrap();
    assert_eq!(ch, '1');
    assert!(score >= 0.95);
}

#[test]
fn test_match_character_edge_cases() {
    let template = CvTemplate {
        char_val: 'A',
        width: 8,
        height: 8,
        mask: &[1u8; 64],
    };

    // Empty / zero dimension cases
    assert_eq!(match_character(&[], 0, 0, &[template]), None);
    assert_eq!(match_character(&[1], 1, 0, &[template]), None);
    assert_eq!(match_character(&[1], 0, 1, &[template]), None);
    assert_eq!(match_character(&[1; 64], 8, 8, &[]), None);

    // Insufficient buffer length
    assert_eq!(match_character(&[1; 10], 8, 8, &[template]), None);

    // Unmatched character returns None if score < 0.65
    let zero_input = [0u8; 64];
    assert_eq!(match_character(&zero_input, 8, 8, &[template]), None);
}

#[test]
fn test_resize_binary_nearest_into() {
    let src = [1, 0, 1, 0, 0, 1, 0, 1]; // 4x2
    let mut dst = [0u8; 8 * 4];
    resize_binary_nearest_into(&src, 4, 2, &mut dst, 8, 4);
    assert_eq!(&dst[0..8], &[1, 1, 0, 0, 1, 1, 0, 0]);
    assert_eq!(&dst[8..16], &[1, 1, 0, 0, 1, 1, 0, 0]);
    assert_eq!(&dst[16..24], &[0, 0, 1, 1, 0, 0, 1, 1]);
    assert_eq!(&dst[24..32], &[0, 0, 1, 1, 0, 0, 1, 1]);
}

#[test]
fn test_segment_characters_stack_and_large_inputs() {
    // Small input (<= 512px stack path)
    let mut binary_small = vec![0u8; 100 * 20];
    // Paint 2 characters of width 10 each
    for y in 0..20 {
        for x in 10..20 {
            binary_small[y * 100 + x] = 255;
        }
        for x in 40..50 {
            binary_small[y * 100 + x] = 255;
        }
    }
    let segs_small = segment_characters(&binary_small, 100, 20);
    assert_eq!(segs_small, vec![(10, 20), (40, 50)]);

    // Large input (> 512px heap fallback path)
    let mut binary_large = vec![0u8; 600 * 10];
    for y in 0..10 {
        for x in 550..570 {
            binary_large[y * 600 + x] = 255;
        }
    }
    let segs_large = segment_characters(&binary_large, 600, 10);
    assert_eq!(segs_large, vec![(550, 570)]);
}

#[test]
fn test_match_character_vertical_normalization() {
    // 32px height font-like template: top 4 lines 0, body 24 lines 1, bottom 4 lines 0 (width 16)
    let mut template_mask = [0u8; 16 * 32];
    for y in 4..28 {
        for x in 4..12 {
            template_mask[y * 16 + x] = 1;
        }
    }
    let template = CvTemplate {
        char_val: 'I',
        width: 16,
        height: 32,
        mask: &template_mask,
    };

    // Case 1: Pure body without vertical padding (width 12, height 24, glyph x in 3..9 is 25%..75%)
    let mut input_tight = [0u8; 12 * 24];
    for y in 0..24 {
        for x in 3..9 {
            input_tight[y * 12 + x] = 1;
        }
    }
    let res_tight = match_character(&input_tight, 12, 24, &[template]);
    assert!(res_tight.is_some());
    let (ch1, score1) = res_tight.unwrap();
    assert_eq!(ch1, 'I');
    assert!(
        score1 >= 0.95,
        "Score for tight input should be >= 0.95, got {}",
        score1
    );

    // Case 2: Body with arbitrary top/bottom padding (width 12, height 26, body 18 lines)
    let mut input_padded = [0u8; 12 * 26];
    for y in 3..21 {
        for x in 3..9 {
            input_padded[y * 12 + x] = 1;
        }
    }
    let res_padded = match_character(&input_padded, 12, 26, &[template]);
    assert!(res_padded.is_some());
    let (ch2, score2) = res_padded.unwrap();
    assert_eq!(ch2, 'I');
    assert!(
        score2 >= 0.95,
        "Score for padded input should be >= 0.95, got {}",
        score2
    );
}

#[test]
fn test_match_character_soft_blurred_waist_8_vs_0() {
    // 16x32 크기의 템플릿 생성: '0' (외곽 루프만 있음) vs '8' (외곽 루프 + 허리선)
    let mut mask_0 = [0u8; 16 * 32];
    let mut mask_8 = [0u8; 16 * 32];

    // 공통 외곽 타원 루프 (상/하/좌/우)
    for y in 4..28 {
        for x in 2..14 {
            let is_border =
                x == 2 || x == 3 || x == 12 || x == 13 || y == 4 || y == 5 || y == 26 || y == 27;
            if is_border {
                mask_0[y * 16 + x] = 1;
                mask_8[y * 16 + x] = 1;
            }
        }
    }
    // '8'에만 허리선 (y=15..17) 추가
    for y in 15..17 {
        for x in 2..14 {
            mask_8[y * 16 + x] = 1;
        }
    }

    let tmpl_0 = CvTemplate {
        char_val: '0',
        width: 16,
        height: 32,
        mask: &mask_0,
    };
    let tmpl_8 = CvTemplate {
        char_val: '8',
        width: 16,
        height: 32,
        mask: &mask_8,
    };
    let templates = [tmpl_0, tmpl_8];

    // 1440p 다운샘플링 블러를 시뮬레이션한 '8' 입력 생성:
    // 외곽선은 밝기 240, 허리선은 블러로 희석되어 140 (대비 58% 수준), 배경은 20
    let mut blurred_8 = vec![20u8; 16 * 32];
    for y in 4..28 {
        for x in 2..14 {
            let is_border =
                x == 2 || x == 3 || x == 12 || x == 13 || y == 4 || y == 5 || y == 26 || y == 27;
            if is_border {
                blurred_8[y * 16 + x] = 240;
            }
        }
    }
    for y in 15..17 {
        for x in 2..14 {
            blurred_8[y * 16 + x] = 140; // 흐려진 허리선!
        }
    }

    // 1. 기존 하드 이진화(72% 대비): 허리선(140)이 임계값(20 + 220*0.72 ≈ 178) 아래로 떨어져 잘림 -> '0'으로 오인식됨!
    let mut hard_bin = vec![0u8; 16 * 32];
    for i in 0..16 * 32 {
        if blurred_8[i] >= 178 {
            hard_bin[i] = 255;
        }
    }
    let hard_res = match_character(&hard_bin, 16, 32, &templates);
    assert_eq!(
        hard_res.map(|m| m.0),
        Some('0'),
        "하드 이진화에서는 허리선이 잘려 '0'으로 오인식됨을 증명"
    );

    // 2. 소프트 템플릿 매칭: 허리선의 밝기(140)가 보존되어 '8'을 정확하게 판독!
    let soft_res = match_character_soft(&blurred_8, 16, 32, &templates);
    assert!(soft_res.is_some());
    let (ch, score) = soft_res.unwrap();
    assert_eq!(
        ch, '8',
        "소프트 템플릿 매칭은 블러로 옅어진 허리선을 감지하여 '8'을 올바르게 판정해야 함"
    );
    assert!(score >= 0.50);
}

#[test]
fn test_match_character_soft_blurred_left_8_vs_3() {
    // '3' (좌측 기둥이 없음) vs '8' (좌측 기둥이 있음)
    let mut mask_3 = [0u8; 16 * 32];
    let mut mask_8 = [0u8; 16 * 32];

    // 공통 우측 곡선 및 상/중/하 가로선
    for y in 4..28 {
        for x in 2..14 {
            let is_right = x == 12 || x == 13;
            let is_horizontal = y == 4 || y == 5 || y == 15 || y == 16 || y == 26 || y == 27;
            if is_right || is_horizontal {
                mask_3[y * 16 + x] = 1;
                mask_8[y * 16 + x] = 1;
            }
        }
    }
    // '8'에만 좌측 기둥 (x=2..4, y=6..14, y=17..25) 추가
    for y in 6..26 {
        if y == 15 || y == 16 {
            continue;
        }
        mask_8[y * 16 + 2] = 1;
        mask_8[y * 16 + 3] = 1;
    }

    let tmpl_3 = CvTemplate {
        char_val: '3',
        width: 16,
        height: 32,
        mask: &mask_3,
    };
    let tmpl_8 = CvTemplate {
        char_val: '8',
        width: 16,
        height: 32,
        mask: &mask_8,
    };
    let templates = [tmpl_3, tmpl_8];

    // 1440p 다운샘플링 블러로 좌측 기둥이 희석된(140) '8' 입력
    let mut blurred_8 = vec![20u8; 16 * 32];
    for y in 4..28 {
        for x in 2..14 {
            let is_right = x == 12 || x == 13;
            let is_horizontal = y == 4 || y == 5 || y == 15 || y == 16 || y == 26 || y == 27;
            if is_right || is_horizontal {
                blurred_8[y * 16 + x] = 240;
            }
        }
    }
    for y in 6..26 {
        if y == 15 || y == 16 {
            continue;
        }
        blurred_8[y * 16 + 2] = 140;
        blurred_8[y * 16 + 3] = 140;
    }

    // 1. 하드 이진화(72%): 좌측선(140)이 잘림 -> '3'으로 오인식됨!
    let mut hard_bin = vec![0u8; 16 * 32];
    for i in 0..16 * 32 {
        if blurred_8[i] >= 178 {
            hard_bin[i] = 255;
        }
    }
    let hard_res = match_character(&hard_bin, 16, 32, &templates);
    assert_eq!(
        hard_res.map(|m| m.0),
        Some('3'),
        "하드 이진화에서는 좌측 기둥이 잘려 '3'으로 오인식됨을 증명"
    );

    // 2. 소프트 템플릿 매칭: 좌측 기둥 신호(140)가 '3'에는 페널티로 작용하고 '8'에는 가산점으로 작용 -> '8'로 정확히 판독!
    let soft_res = match_character_soft(&blurred_8, 16, 32, &templates);
    assert!(soft_res.is_some());
    let (ch, score) = soft_res.unwrap();
    assert_eq!(
        ch, '8',
        "소프트 템플릿 매칭은 블러로 옅어진 좌측선을 감지하여 '8'을 올바르게 판정해야 함"
    );
    assert!(score >= 0.50);
}

#[test]
fn test_match_character_soft_vertical_normalization() {
    // 32px height font-like template: top 4 lines 0, body 24 lines 1, bottom 4 lines 0 (width 16)
    let mut template_mask = [0u8; 16 * 32];
    for y in 4..28 {
        for x in 4..12 {
            template_mask[y * 16 + x] = 1;
        }
    }
    let template = CvTemplate {
        char_val: 'I',
        width: 16,
        height: 32,
        mask: &template_mask,
    };

    // Case: Body with arbitrary top/bottom padding (width 12, height 26, body 18 lines)
    // Background 20, Body 240
    let mut input_padded = vec![20u8; 12 * 26];
    for y in 3..21 {
        for x in 3..9 {
            input_padded[y * 12 + x] = 240;
        }
    }
    let res_padded = match_character_soft(&input_padded, 12, 26, &[template]);
    assert!(res_padded.is_some());
    let (ch, score) = res_padded.unwrap();
    assert_eq!(ch, 'I');
    assert!(
        score >= 0.85,
        "Score for soft padded input should be >= 0.85, got {}",
        score
    );
}
