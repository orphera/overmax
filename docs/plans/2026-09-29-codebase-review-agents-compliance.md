# 코드 전반 리뷰 결과 및 수정 추적 (AGENTS.md 규약 준수 점검)

> 최초 리뷰: 2026-09-29 / 최종 갱신: 2026-10-02
> 범위: `rust/overmax_core`, `rust/overmax_cv`, `rust/overmax_engine`, `rust/overmax_data`, `rust/overmax_app` (133 파일 / 46,404 LOC)
> 방식: 5개 도메인 병렬 정적 리뷰 + 실제 빌드/테스트 실행, 각 finding은 파일:라인 인용 기준
> 작업 브랜치: `chore/codebase-review-fixes`
>
> 이 문서는 **진단 결과와 수정 진행 상황을 함께 추적**한다. 파일:라인 인용은 별도 표기가 없는 한 **2026-09-29 리뷰 기준 코드**의 위치이며, 이미 수정된 파일(`schema.rs`, `sync.rs`, `dxgi.rs`, `native_app*.rs`, `varchive.rs`, `recommend_provider.rs`)에서는 라인이 밀렸을 수 있다.

---

## 0. 진행 현황 요약

| ID | 등급 | 항목 | 상태 | 커밋 |
|----|------|------|------|------|
| §2.1 | CRITICAL | 레거시 `records` 마이그레이션이 기록 전량 삭제 | ✅ 완료 | `d2954f9` |
| §2.2 | CRITICAL | UI 렌더 경로 매 프레임 DB 조회 | ✅ 완료 (업로드 플래그만) | `ac1063b` |
| §3.1 | HIGH | `hdr_replay_test` 실패 | ✅ 원인 규명, `#[ignore]` | `2ed4543` |
| §3.2 | HIGH | DXGI 출력 교체 실패를 삼킴 | ✅ 완료 (부수 효과 주의) | `a58a502` |
| §3.3 | HIGH | V-Archive 전체 조회 빈 응답 시 캐시 소실 | ✅ 완료 (빈 응답 시 캐시 보존 방향 유지 결정) | `ef56960`, `44f2d5f` |
| §3.4 | HIGH | Provider가 요청 대상 호스트 결정 + 비원자 쓰기 | ⚠️ 부분 완료 ((a) 완료, (b) 되돌림·§4.3 대기) | `070cfa8`, `24c8d4a` ((b) `63ae6d4` → revert `bd421d7`) |
| §3.5 | HIGH | Linux 오버레이가 IPC 표시 명령 무시 | ✅ 완료 | `9a556ca` |
| §4.1 | — | `with_retry`가 op을 4번째 실행 | ❌ 오진 (루프 밖 코드 도달 불가, 계약 테스트 추가) | `e93176d` |
| §4.2 | MEDIUM | `get_merged()` 매 프레임 deep clone | ⬇️ 실측 5.2µs/호출, 수정 안 함 | |
| §4.3 | MEDIUM | `write_atomic` 비원자성 | ⛔ 수정 시도 후 되돌림, 해법 미정 | `30125d0` (문서) |
| §4.4 | MEDIUM | DXGI 오류 1회에 GDI 강등 | ⬇️ 의도된 fail-safe, 발생 사례 없음, 수정 안 함 | |
| §4.5 | MEDIUM | 아틀라스 staging 미초기화 | ⬇️ 제안 수정 불가 + 발동 조건 좁음, 수정 안 함 | |
| §4.6 | MEDIUM | DXGI 타임아웃 동일 프레임 `Ok` 재전달 | ❌ 오진 (카운터는 시간 게이트, 제안 수정은 기능 파손) | |
| §4.7 | MEDIUM | 매 프레임 `is_fullscreen` syscall + dead 필드 | ✅ 완료 (Linux 빌드는 CI 확인 대기) | `2218b89`, `505e691` |
| §4.8 | — | GDI HBITMAP 누수 | ❌ 오진 (실측 반증) | |
| §4.9 | MEDIUM | `image_index.db` 갱신 미반영 | ✅ 완료 (실기 검증 대기) | `63b4a54`, `6baaf48` |
| §4.10 | MEDIUM | 서버 JSON 무검증 영속화 | ✅ 완료 | `e527501`, `58ead08` |
| §4.11 | MEDIUM | V-Archive URL 보간 | ✅ 완료 | `74094d8`, `8185d53` |
| §4.12 | MEDIUM | `AccountInfo` Debug로 토큰 노출 | ✅ 완료 (에러 메시지 URL 노출은 잔여) | `b7147a8` |
| §4.13 | LOW | `image_index` 로드마다 DDL | ⬇️ 결함 없음, 수정 안 함 | |
| §4.14 | — | `user_version` 미사용 | ⏸️ 근거 부족, 사용자 판단 대기 | |
| §4.15 | MEDIUM | 마이그레이션 실패를 삼키고 `is_ready=true` | ✅ 완료 | `d6c2546`, `19457c1` |
| §4.16 | MEDIUM | `upsert` 트랜잭션 부재 | ⬇️ 프로덕션 쓰기 스레드 1개, 수정 안 함 | |
| §4.17 | MEDIUM | OCR 잔존 설정/문서 | ✅ 완료 (필드는 호환성 위해 유지) | `5a0c831`, `41b3dab` |
| §4.18 | MEDIUM | 이진화 대비율 문서 72% → 65% | ✅ 완료 | `98c2a9f` |
| §4.19 | MEDIUM | `detect_rect_edges` margin unscaled | ⏳ 미착수 (측정 선행) | |
| §4.20 | MEDIUM | IPC 인증/스레드 제한 부재 | ⏸️ 설계 의도 확인 대기 | |
| §4.21 | LOW | 벤치 바이너리 릴리스 포함 | ⬇️ 배포물에 미포함(컴파일만), 수정 안 함 | |
| §4.22 | MEDIUM | CV 중복 작업 | ⏳ 미착수 (계측 선행) | |
| §4.23 | MEDIUM | Linux 정규화 부재 | ⏸️ 측정 전 보류 | |
| §4.24 | MEDIUM | Linux 풀 프레임 2회 순회 | ⏳ 미착수 | |
| §4.25 | MEDIUM | 문서-코드 드리프트 | ⚠️ 부분 완료 (슬롯 수만) | `4bcfbf2` |
| §7.2 | — | 2026-10-02 후속 리뷰 지적 사항 | ✅ 완료 | |

---

## 1. 품질 게이트 실행 결과

| 항목 | 명령 | 결과 (2026-09-29) |
|------|------|------|
| 클리피 | `cargo clippy --all-targets --workspace` | **경고 0건** 통과 (`cognitive_complexity = deny` 포함) |
| 테스트 | `cargo test --workspace` | **전부 통과** (`hdr_replay_test` 1건은 §3.1 사유로 `#[ignore]` 처리) |

`cargo test --workspace` 세부:
- `overmax_app` lib 23/23 통과, `ipc_server_integration` 1/1 통과
- `overmax_core` 3/3, `game_state_fixture` 1/1
- `overmax_data` 75/75 통과
- `overmax_cv` 10/10 통과
- `overmax_engine` lib 65/66 (1 ignored) + `hdr_replay_test` 14 passed / 1 ignored
- doc-test 전 크레이트 0 tests

리뷰 최초 실행 시점에 `hdr_replay_test::test_analyze_all_hdr_snapshots` 1건이 실패했으나, §3.1에서 **테스트 에셋 문제로 확정**되어 `#[ignore]` 처리했다. 코드 로직 변경은 없다. (이후 수정 커밋들이 테스트를 추가했으므로 위 개수는 리뷰 시점 기준이다.)

### 규약 준수 확인 항목 (문제 없음)

- **절대경로 하드코딩 0건.** `git grep -E 'D:\\dev|D:/dev|C:\\Users|C:/Users|/home/[a-z]'` 결과 5건은 전부 규약 문서 안의 "금지 예시" 텍스트(`AGENTS.md:84`, `ENGINEERING_TASTE.md:69,72`, `.antigravity/hooks/pre-save.ps1:13`). 소스/설정/CI/스크립트의 실제 사용처 0건.
- **메모리 접근 및 프로세스 인젝션 0건.** `ReadProcessMemory`, `OpenProcess`, `WriteProcessMemory`, `VirtualAlloc`, `/proc/`, `ptrace`, `inject` 전부 0건. 캡처는 `capture_bgra_inplace` 경로만 사용.
- **다중 패스 OCR 0건.** OCR 모듈 자체가 2026-07-28에 완전 삭제되었으며(`docs/decisions/detection_pipeline.md:52,53`), `run_ocr` 심볼 0건. 템플릿 매칭은 전부 단일 `for` 루프(`cv/image.rs:427-439`, `:618-649`, `templates/matching.rs:251-259`).
- **추천 시스템 floor 기반 구조 유지.** `Classic` 전략(`service/recommend/sorting/strategy.rs:20-34`)의 `is_played()` → `rate` → `floor` 정렬 기준 그대로. `Smart`(`:59-109`)는 가산 방식이며 곱셈 가중치 변경 없음. AGENTS.md 「기존 정렬 기준을 깨지 않도록 보완 방식」 준수.
- **커서 위치 오염 없음.** `derive_recommended_level`(scoring.rs:613-711) 시그니처에 `ref_floor`/`use_official` 파라미터 없음. `tests.rs:1262-1523`이 NM/HD/MX 커서 전환 시 footer 레벨 불변을 고정 검증.
- **프로덕션 경로 `unwrap`/`panic!` 0건.** 발견된 `panic!`/`unwrap()`/`expect()`는 전부 `#[cfg(test)] mod tests` 내부 또는 도달 불가 분기(`dxgi.rs:568`의 normalizer `unwrap`은 `ensure_normalizer()?` 하단에 위치).
- **0 나눗셈 없음.** `scoring.rs`의 `rate_to_rate_ratio`(상수 분모), `rating_to_effective_floor`(`ratio <= 0.0` 가드), `calculate_performance_rating`(`floor <= 0.0` 가드), `avg_rate`(`has_record_count == 0` 가드) 모두 안전.
- **설정 미지의 키 보존.** `merge_maps`(config/settings.rs:76-87)는 `settings.user.json`의 미지 섹션/키를 그대로 보존. 실측: `{"totally_new_section":{...}}` 역직렬화 성공, `overlay.scale == 1.25` 유지.
- **커밋되어선 안 될 파일 추적 0건.** `cache/`, `scratch/`, `target/`, `settings.user.json`, `*.log`, `*.db` 경로 일치 항목 없음. 추적 바이너리는 패키징 정적 자산(`assets/overmax.png` 1.52MB 등)뿐.
- **TODO/FIXME/HACK 0건** (`rust/*`, `settings.json`, `*.toml` 대상). 단, 이후 §3.1 조치로 `hdr_replay_test.rs`에 `TODO(hdr)` 1건이 의도적으로 추가되었다.
- **SQL 인젝션 없음.** V-Archive 동기화는 `song_id`를 SQL 바인딩 파라미터로만 사용(`record_db/sync.rs:63`).
- **IPC 페이로드 무제한 방어 존재.** `MAX_RPC_BODY = 64 * 1024`(transport/loopback.rs:19), `content_length` 재확인(`:429`), `get_recent_plays` limit `.clamp(1, 100)`(ipc_server.rs:301).
- **i18n 키 누락은 구조적으로 불가.** `t!` 매크로의 정적 룩업이라 미등록 키는 컴파일 에러.

---

## 2. CRITICAL

### 2.1 레거시 `records` 테이블 마이그레이션이 사용자 기록을 조용히 전량 삭제 — ✅ 완료 (`d2954f9`)

- **파일**: `rust/overmax_data/src/store/record_db/schema.rs:115-121`
- **코드 (수정 전)**:
```rust
fn ensure_schema(&self, conn: &mut Connection) {
    if let Ok(has_col) = self.table_has_column(conn, "records", "is_max_combo") {
        if !has_col {
            let _ = conn.execute("DROP TABLE records", []);
            let _ = self.create_records_table(conn);
        }
    }
```
- **문제**: `is_max_combo` 컬럼이 없는 레거시 DB를 여는 순간 `DROP TABLE`이 실행되고, `let _ =`로 오류도 감춰진다. **재현 확인**: 50행 삽입된 레거시 DB에 `initialize()` 호출 → `initialize_returned=true, is_ready=true, rows_after=Ok(0)`, `get(1) == None`. 사용자는 플레이 기록 전부를 잃고 성공 응답만 받는다.
- **AGENTS.md 근거**: Key Constraints 「기존 호환성 파괴 금지」, 「땜질식(대증요법) 코드 지양」
- **git blame 게이트**: `f9776f1` (2026-08-26). 버그가 실제로 재현되었으므로 수정 근거 충족.
- **조치**: `DROP TABLE` + recreate를 `ALTER TABLE records ADD COLUMN is_max_combo INTEGER NOT NULL DEFAULT 0`으로 교체. 레거시 스키마가 `is_max_combo`만 빠진 채 나머지는 현행과 동일함을 `95048ec8^:data/record_db.py` 히스토리에서 확인했다. 회귀 테스트 2건(`legacy_records_migration_preserves_existing_rows`, `legacy_migrated_db_accepts_upsert_with_max_combo`) 추가, 수정 전 코드에서 실패함을 확인.
- **잔여**: `ALTER TABLE` 실패도 여전히 `let _ =`로 삼켜지고 `is_ready = true`가 된다 → §4.15에서 처리 완료(`d6c2546`).

### 2.2 UI 렌더 경로가 매 프레임 SQLite Connection과 파일 stat을 수행 — ✅ 완료 (`ac1063b`, 업로드 플래그만)

- **파일**: `rust/overmax_app/src/ui/native_app_viewports.rs:887-888` (Windows), `:601-602` (Linux)
- **호출 체인**:
  - `current_pattern_needs_upload()` → `native_app.rs:874` → `record_manager.get_local_record(song_id, mode, diff)` → `record_db/mod.rs:253` `open_conn()` — **매 호출마다 새 Connection + PRAGMA 3개 실행**
  - `is_varchive_account_configured()` → `native_app.rs:840` → `self.settings.get_merged()` (JSON deep clone + 역직렬화) + `Path::exists()` 파일 stat
  - 두 호출 모두 `eframe::App::ui()` → `render_overlay_panel` 안에서 **매 프레임** 실행
- **AGENTS.md 근거**: 「성능 저하 야기 금지 (최우선)」, Decision Policy 「인게임 성능 영향이 있는 경우 정확도보다 성능을 우선」
- **조치**: `current_pattern_needs_upload` 결과를 `drain_detection_results`의 `changed` 지점에서 1회 계산해 `NativeApp.overlay_upload_needed`로 캐시. Windows/Linux 렌더 경로 모두 필드 참조로 변경.
- **잔여**: `is_varchive_account_configured`는 계정 파일 생성/설정 변경 등 `changed` 지점 밖에서도 바뀌므로 그대로 두었다. 매 프레임 `get_merged()` + stat은 §4.2와 함께 처리한다.
- **측정 관련 주의**: "구조적으로 프레임 예산을 침범한다"까지가 확인 범위이며 **정량 ms는 측정하지 않았다**(AGENTS.md 「근거 없는 성능 개선 주장 금지」).

---

## 3. HIGH

### 3.1 `hdr_replay_test` 실패 — ✅ 원인 규명됨: 스냅샷 에셋이 구(舊) 아틀라스 배치 (`2ed4543`)

- **파일**: `rust/overmax_engine/tests/hdr_replay_test.rs:37` (`#[ignore]` 부여), 본문 주석 `:246`
- **최초 판정(철회)**: "d9eabd3..21c9652 병합 구간에서 실측 데이터 기반 씬 감지 회귀"는 틀렸다. **제품 코드 회귀는 없다.**
- **실제 원인**: `scratch/hdr_snapshot/*.raw`가 **43슬롯 아틀라스 배치로 덤프**되어 있고, 커밋 `30939f7`(2026-09-23)이 슬롯을 43→47로 재패킹하며 `ATLAS_SLOTS`의 `atlas_rect`를 전부 변경했다. 스냅샷 픽셀은 옛 좌표에 있고 현재 코드는 새 좌표를 읽는다.
- **규명 근거**:
  1) **스냅샷은 아틀라스 프레임이다.** `WIDTH=512, HEIGHT=512`(`:103-104`), 파일 크기 2,097,152 = 512×512×8(fp16 4채널), `hdr_snapshot1.json`에 `"is_atlas": true`. `RoiManager::new(512, 512)`가 `roi.rs:127`에서 `is_atlas = true`로 진입하고 모든 ROI 조회가 `AtlasTranslator::get_roi_for_scene`(`roi.rs:146-150`)로 분기하므로 **패킹 좌표가 판정에 직결된다.**
  2) **`src_rect`는 불변, `atlas_rect`만 전부 변경됨.** d9eabd3(43슬롯) vs HEAD(47슬롯):

     | ROI | `src_rect` (두 버전 동일) | `atlas_rect` d9eabd3 | `atlas_rect` HEAD |
     |-----|---------------------------|--------------------|-------------------|
     | Freestyle/jacket | (710,533,64,60) | (340,94,64,60) | **(409,94,64,60)** |
     | Freestyle/score | (173,558,104,24) | (129,487,104,24) | **(408,348,104,24)** |
     | Freestyle/rate | (172,583,104,22) | (233,487,104,22) | **(408,372,104,22)** |
     | OpenMatch/jacket | (664,533,64,60) | (317,169,64,60) | **(409,154,64,60)** |
     | Freestyle/btn_mode | (80,130,5,5) | (507,425,5,5) | **(359,162,5,5)** |

  3) **스냅샷 실측: 새 좌표에는 자켓 픽셀이 없다.** `hdr_snapshot1.raw` 직접 파싱 휘도:

     | 영역 | lum mean | min | max |
     |------|----------|-----|-----|
     | 옛 좌표 (340,94) Freestyle/jacket | **1.555** | 0.049 | 4.174 |
     | 새 좌표 (409,94) Freestyle/jacket | **0.044** | 0.017 | 0.054 |
     | 옛 좌표 (317,169) OpenMatch/jacket | 2.260 | 0.526 | 5.168 |
     | 새 좌표 (409,154) OpenMatch/jacket | 0.803 | 0.000 | 2.463 |

     Freestyle 새 좌표는 사실상 검정(0.044)이고, OpenMatch 새 좌표도 옛 좌표 대비 휘도가 크게 낮다(다른 슬롯의 일부가 걸친 것으로 보임). 테스트 로그에서 `[자켓 매칭] Freestyle(340,94)=Some("id=733, sim=0.9314")` — **옛 좌표를 하드코딩 크롭하면 매칭이 성공**한다는 독립 증거가 있다. 씬 판정이 `Unknown`이 되고 `check_category_band_solid` 로그가 출력되지 않는 것도 빈 자켓 ROI에서 downstream 게이트가 조기 반환하는 것과 일치한다.
- **실수 기록 (파일별 이분법 실패)**: `atlas_layout.rs`만 d9eabd3로 되돌리면 HEAD의 `gameplay_scene.rs`/`atlas_translator.rs`가 참조하는 신규 `gp_*` 슬롯이 사라져 컴파일 에러(`E0433`, `E0599`)가 났다. 이 테스트는 atlas 경로이므로 **atlas 관련 파일을 함께 되돌리는 것이 유효한 이분법**이었는데 그 시도 없이 추측했었다.
- **AGENTS.md 근거**: Failure Handling 「확실하지 않은 시스템 상태 → 결과를 보류하거나 verified=False 유지」. 검증 불가 상태를 통과로 처리하지 않고 `#[ignore]`로 명시한 것이 이 조항에 부합한다.
- **조치**: `#[ignore = "stale atlas snapshot: 43-slot dump vs current 47-slot packing"]` + 본문 `TODO(hdr)` 주석. **판정 로직·ROI·캡처 코드는 변경하지 않았다.** 검증: `cargo test -p overmax-engine --test hdr_replay_test` → `14 passed; 0 failed; 1 ignored`.
- **해제 조건**: 47슬롯 배치로 재 덤프한 HDR 캡처로 스냅샷을 교체하고 `#[ignore]`를 해제한다. 그전까지 이 테스트는 **프로덕션 인식 정확도에 대한 어떤 증거도 제공하지 못한다.**

### 3.2 DXGI 출력 교체 실패가 조용히 무시되어 다른 모니터 픽셀이 씬으로 유입 — ✅ 완료 (`a58a502`)

- **파일**: `rust/overmax_engine/src/capture/capture_engine/windows/dxgi.rs:362-392`, `:506`
- **문제**: 게임 창을 서브 모니터로 드래그한 직후 `find_output`(`DuplicateOutput1`)이 실패하면, 엔진은 **옛 출력의 duplication을 유지한 채** 새 모니터 좌표로 크롭했다. `AcquireNextFrame`은 성공하므로 `Ok`가 반환되고, 파이프라인은 다른 모니터 데스크톱 프레임을 정상 입력으로 처리한다. `ensure_output_for_rect`는 `Result`를 반환하지만 호출부가 `let _ =`로 버렸다.
- **AGENTS.md 근거**: Failure Handling 「확실하지 않은 시스템 상태 → 결과를 보류하거나 verified=False 유지」
- **조치**: `find_output` 실패를 `?`로 전파하고 호출부를 `self.ensure_output_for_rect(rect)?`로 변경.
- **부수 효과 (인지된 트레이드오프)**: 이제 출력 교체 실패 시 상위 `windows/mod.rs:166-174`가 DXGI 백엔드를 파괴하고 **3초간 GDI로 강등**된다. §4.4가 문제로 지적한 강등 경로를 이 수정이 새로 타게 된다. 오염 프레임을 정상 입력으로 넘기는 것보다는 강등이 낫다고 판단했으나, §4.4 처리 시 이 경로도 함께 고려해야 한다.
- **미검증**: 실제 멀티모니터 드래그 상황에서의 동작은 실앱으로 확인하지 않았다(유닛 테스트 없음).

### 3.3 V-Archive 전체 조회가 빈 응답 한 번으로 캐시를 전량 소실 — ✅ 완료 (`ef56960`, 주석 정정 `44f2d5f`)

- **파일**: `rust/overmax_data/src/store/record_db/sync.rs:27-38`
- **정정 (2026-10-02)**: 최초 문서는 이를 "증분 동기화" 문제로 서술했으나 **반대다.** 호출부 `native_app.rs:1070`은 `clear_first = since.is_none()`이므로, DELETE가 실행되는 것은 `since` 없는 **전체 조회**와 레거시 JSON 마이그레이션(`schema.rs:157`)이다. 증분 조회(`clear_first=false`)는 원래 DELETE를 하지 않는다.
- **문제**: DELETE가 `records` 배열 추출보다 먼저 실행되어, 서버가 전체 조회에 `{"records":[]}`를 주면 공식 Top-50 랭크/레이팅/실력 프로필이 전부 소실되고 `get_top50_summary_with_fallback`(`record_manager.rs:168`)가 로컬 records로 대체되어 추천 실력 모델이 흔들린다. **재현 확인**: 1건 병합 후 빈 배열로 재호출 → `get_varchive_rating_map(&[1])`가 `{}` 반환.
- **AGENTS.md 근거**: CONTEXT.md 불변 조건 7(추천 엔진 실력 모델 통계 일관성), 「기존 호환성 파괴 금지」
- **조치**: `records` 추출을 DELETE 앞으로 옮기고, 빈 배열이면 트랜잭션을 커밋하지 않고 `Ok(())` 반환. 회귀 테스트 2건 추가.
- **트레이드오프 (결정됨)**: 전체 조회에서 빈 배열은 **정상 상태일 수도 있다**(해당 버튼 모드에 기록이 없거나, V-Archive 기록을 초기화한 경우). 현재 수정은 이 경우에도 옛 캐시를 영구 보존한다. "일시 장애로 인한 빈 응답이 정상적인 빈 상태보다 훨씬 흔하고, 오래된 캐시가 남는 비용이 캐시 소실보다 작다"는 판단에 근거하며, **사용자 결정(2026-10-02)으로 이 방향을 유지한다.** V-Archive의 실제 응답 형태는 §6-19에서 계속 추적한다.
- **주석 정정 (완료, `44f2d5f`)**: `sync.rs:27` 주석과 회귀 테스트 docstring·assert 메시지의 "증분 동기화"를 "전체 조회"로 고쳤다. 코드 동작 변경 없음.

### 3.4 외부 추천 Provider가 클라이언트의 요청 대상 호스트를 결정 — ⚠️ 부분 완료: (a) 완료 (`070cfa8`, `24c8d4a`), (b) 되돌림 (`bd421d7`)

- **파일**: `rust/overmax_data/src/gateway/recommend_provider.rs:138-176`
- **문제**:
  - (a) **서버 응답(`manifest.endpoint`)이 요청 대상 호스트를 결정한다.** `test_connection`(`:71-93`)은 `protocol` 문자열만 검증한다. Provider가 `endpoint: "https://attacker.example/collect"`를 지정하면 클라이언트가 `v_id`를 쿼리 파라미터로 붙여(`:153-156`) 전송한다.
  - (b) 응답 본문이 **검증 없이** `save_path`에 비원자 `fs::write`로 기록되어, 응답이 잘리면 다음 읽기(`composite.rs:98`)에서 JSON 파싱이 실패한다. `ProviderCacheReader`에 파일 크기 상한도 없다.
- **조치**:
  - (a) `http(s)://` 분기에서 `provider_url`과 `manifest.endpoint`의 **host만** 비교하여 다르면 `GatewayError::InvalidProtocol` 반환.
  - (b) `fs::write`를 `save_path.with_extension("tmp")`에 쓴 뒤 `rename`하도록 변경(`63ae6d4`) → **되돌림 (`bd421d7`).** 아래 재검토 사항 2 참조.
- **재검토 사항**:
  1. ~~**host만 비교하고 scheme/port는 비교하지 않는다.**~~ — **해소 (`24c8d4a`).** `https` provider가 `http://같은호스트/...`를 지정하면 `v_id`가 평문으로 전송되는 문제였다. 해석 로직을 `resolve_endpoint`로 분리하고 `Url::origin()` 비교(scheme, host, port)로 강화했다. 한쪽이라도 파싱 실패 시 거부.
  2. **(b)는 §4.3.1의 결론과 모순되어 되돌렸다 (`bd421d7`).** §4.3.1은 "remove 없이 rename만 하면 Windows read-only 대상에서 `PermissionDenied`가 난다"는 실측으로 동일 수정을 되돌렸는데, `63ae6d4`가 같은 패턴을 새로 도입했고 `with_extension("tmp")` 이름 충돌(§4.3)도 가져왔다. 사용자 결정(2026-10-02)에 따라 직접 `fs::write`로 복귀했다.
     - **남은 위험 (수용)**:
       - **잘린 쓰기**: 응답 본문 쓰기 도중 프로세스가 죽거나 디스크가 가득 차면 `save_path`에 잘린 JSON이 남는다. 파싱 실패 시 `ProviderCacheReader::recommend`(`composite.rs:128-137`)는 `SourceStatus::Error`와 빈 결과를 반환하므로 해당 곡/모드/난이도의 외부 추천 섹션이 비어 보인다. 같은 패턴을 다시 조회할 때 캐시 파일이 10초보다 오래됐으면 재요청해 덮어쓰므로(`native_app_recommend.rs:231-240`) 다음 갱신 성공 시 복구된다. 기록·설정 데이터에는 영향이 없다.
       - **쓰기 중 동시 읽기 (미측정)**: 같은 호출부가 백그라운드 스레드로 `fetch_recommend_blocking`을 띄운 직후 UI 스레드에서 같은 파일을 읽는다(`native_app_recommend.rs:242-257`). `fs::write`는 truncate 후 쓰기이므로 읽기가 쓰기 도중에 겹치면 빈 파일이나 일부만 읽혀 같은 `Error` 경로를 탄다. 파일이 작아 겹칠 확률은 낮다고 보이나 측정하지 않았다. 원자 교체가 도입되면 이 경로도 함께 해소된다.
     - **수용 근거**: read-only 회귀는 캐시 갱신을 **영구히** 막는 반면, 위 두 위험은 외부 추천 섹션 1건이 일시적으로 비는 정도이고 다음 갱신에서 복구된다.
     - **닫는 방법**: §4.3에서 read-only 대응 교체 전략(§4.3.1 후보 1 또는 2)이 정해지면 `cache_downloader::write_atomic`과 이 쓰기 지점에 함께 적용한다. 파일 크기 상한 부재는 별도 항목으로 남는다.
  3. ~~**회귀 테스트가 없다.**~~ — **해소 (`24c8d4a`).** `resolve_endpoint` 테스트 6건 추가. host-only 비교로 되돌리면 scheme 다운그레이드·다른 포트 2건이 실패함을 확인했다.
  4. ~~**커밋 규율 위반**~~ — **해소.** 원래 `aeeb763` 한 커밋에 (a), (b), §4.18이 섞여 있었으나 push 전에 `070cfa8`(a), `63ae6d4`(b), `98c2a9f`(§4.18)로 분리했다.

### 3.5 Linux 오버레이가 IPC `set_overlay_visibility`를 무시 — ✅ 완료 (`9a556ca`)

- **파일**: `rust/overmax_app/src/ui/native_app_viewports.rs:601-617` (Linux publish) vs `:528` (Windows)
- **문제**: `ipc_server.rs:329-336`의 `set_overlay_visibility`는 플랫폼 무관 RPC이고 `native_app_commands.rs:50`이 `self.overlay_visible_override`를 설정하는데, Linux `is_hidden()`(`linux_layer_overlay.rs:1460-1468`)은 이 필드를 보지 않았다. 동일 RPC가 Windows에서만 동작했다.
- **조치**: `LinuxOverlaySnapshot`에 `overlay_visible_override: Option<bool>` 추가, `same_display_snapshot` 비교에 포함, `is_hidden()`에서 `Some(false)`면 숨김. 기존 테스트에 `Some(false)`/`Some(true)`/`None` 단언 추가.

---

## 4. MEDIUM

### 4.1 `with_retry`가 재시도 루프 밖에서 op을 4번째 실행 — ❌ 오진, 수정하지 않음

- **파일**: `rust/overmax_data/src/store/record_db/mod.rs:97-118`
```rust
for attempt in 0..3 {
    let conn = self.open_conn()?;
    match op(&conn) { Ok(val) => return Ok(val), Err(...) if ... && attempt < 2 => { ... } Err(e) => return Err(e), }
}
let conn = self.open_conn()?;   // ← 루프 밖 실행
op(&conn)
```
- **원래 주장**: 세 번 BUSY로 실패한 op을 루프 밖에서 가드 없이 4번째 실행하며, `insert_play_event`(`:422-479`)처럼 SELECT→UPDATE/INSERT를 담은 op은 재실행으로 디바운스 윈도우를 다시 통과할 수 있다.
- **반증**: 루프 밖 두 줄은 **도달 불가**다. BUSY 재시도 분기는 `attempt < 2` 가드가 있으므로 `attempt == 2`(세 번째 시도)에서는 `Ok`든 `Err`든 루프 안에서 반환된다. `open_conn()` 실패도 `?`로 즉시 반환된다. 루프 밖 코드는 `for` 루프가 `!` 타입이 아니라 함수 끝에 반환 식이 필요해서 남은 것으로 보인다.
- **실측 (`e93176d`)**: 항상 `SQLITE_BUSY`를 반환하는 op으로 테스트 `with_retry_runs_op_three_times_on_persistent_busy`를 추가했다. op은 **정확히 3회** 호출되고 마지막 BUSY 오류가 그대로 반환된다. 문서화된 재시도 계약과 실제 횟수가 일치한다. 이 테스트는 계약 고정용으로 남겼다(프로덕션 코드 변경 없음).
- **git blame**: `with_retry`는 `22bcc56`(2026-08-18) 도입.
- **결론**: 버그가 재현되지 않으므로 **수정하지 않음.** 도달 불가 코드를 `unreachable!()`이나 루프 재구성으로 정리하는 것은 취향 판단이라 AGENTS.md 기준으로 단독 근거가 되지 않는다. 특히 `unreachable!()`은 release 프로파일의 `panic = "abort"` 아래 새 패닉 지점을 만든다.

### 4.2 `get_merged()`가 매 프레임 settings 전체 JSON을 deep clone + 재파싱 — ⬇️ 실측 결과 비용 미미, 수정하지 않음

- **파일**: `rust/overmax_app/src/ui/native_app.rs:125-131`, 호출부 `native_app_viewports.rs:96, 584, 667, 915, 992`
```rust
let val = match self.merged.lock() { Ok(g) => g.clone(), ... };
serde_json::from_value(val).unwrap_or_default()
```
- **문제**: 호출부 5곳이 모두 프레임 루프 내부. 특히 `:667` `poll_and_drain_events`는 **무조건 매 프레임** `screen_capture().content_protected`를 읽기 위해 호출하고, `read_overlay_settings`도 매 프레임 `settings.merged.lock()`을 건다. §2.2 잔여인 `is_varchive_account_configured`도 같은 경로다.
- **수정**: `state_tracker.prev_protected: Changed<Option<bool>>` 같은 기존 중복 억제 패턴을 적용. 캐시 필드를 두고 설정 변경 시점에만 갱신.
- **측정 관련 주의**: 정량 프레임 비용은 **미측정**. 구조적 문제로만 표기.
- **재검토 (2026-10-07, 실측)**: 수정하지 않는다.
  - **측정 방법**: 실제 `settings.json` + `settings.user.json` 병합 결과(JSON 1,313바이트)에 대해 `get_merged()`와 동일한 동작(`Mutex` 잠금 → `Value` deep clone → `from_value::<Settings>`)을 release 빌드에서 10만 회 반복. 저장소 밖 별도 crate에서 수행.
  - **결과 (호출당)**: `get_merged()` 전체 **5.16 µs**, 그중 `Value` clone 4.02 µs. 비교용 하한인 "잠금 + 필드 하나 읽기"(`read_overlay_settings` 방식)는 0.028 µs로 약 180배 차이.
  - **프레임당 환산**: 무조건 매 프레임인 호출은 `native_app_viewports.rs:667` 하나, 오버레이 표시 시 `:584`가 추가되고 나머지는 조건부. 프레임당 최대 3회·60 FPS로 가정해도 약 0.93 ms/초(코어 1개의 약 0.09%), 프레임당 약 15 µs(16.7 ms 예산의 0.1%).
  - **감지 워커**: `detection_worker.rs`의 `sync_live_settings`도 매 루프 같은 `from_value::<Settings>`를 수행하나 비용은 같은 자릿수라 무시 가능.
  - **판단**: 구조적으로는 필드 하나 읽기에 전체를 복제·재파싱하는 낭비가 맞으나, 절대 비용이 작고 UI 프레임 경로라 인게임 성능과 경쟁하지 않는다. 수정해도 개선을 주장할 근거가 없다(AGENTS.md 「근거 없는 성능 개선 주장 금지」).
  - **측정 한계**: 락 경합, 할당기 압박, 캐시 효과는 반영하지 않았다.
  - **재개 조건**: 설정 JSON이 크게 늘어나거나, 프레임 프로파일에서 이 경로가 상위로 나타나면 재개한다. 그 경우 수정 방향은 위의 캐시 필드 + 변경 시점 갱신 방식을 따른다.

### 4.3 `write_atomic`이 원자적이지 않음 (remove → rename 2단계) — ⛔ 해법 미정

- **파일**: `rust/overmax_data/src/community/cache_downloader.rs:263-274`
```rust
let tmp = path.with_extension("tmp");
std::fs::write(&tmp, bytes)?;
if path.exists() { std::fs::remove_file(path)?; }
std::fs::rename(tmp, path)?;
```
- **문제**: remove 성공 후 rename이 실패하면 `cache/image_index.db`가 **없는 상태로 남고**, `has_all_required_caches`(`:136-141`)가 false가 되어 다음 실행마다 Cold Start가 되고 자켓 매칭이 비활성화된다. 프로브로 재현: `remove_file Ok -> rename Err(NotFound) -> target exists = false`. 부수적으로 tmp 파일이 실패 시 정리되지 않는다.
- **부수 확인**: `x.json`과 `x.db`는 `with_extension("tmp")`로 **같은 `x.tmp`에 수렴**함을 실측했다. 현재 파일 구성(`songs.json`, `image_index.db`)에서는 충돌하지 않지만 잠재 위험은 남는다.

#### 4.3.1 수정 시도 결과: `remove_file` 제거는 read-only 대상에서 회귀를 만든다 (되돌림)

- **시도한 수정**: `remove_file` 분기를 제거하고 rename만 수행. 일반 파일에 대해 rename-over-existing이 성공함을 확인하고 제안했다.
- **반증**: read-only 대상 파일에서 방향이 반대였다.

  | 대상 파일 | old (remove → rename) | new (rename만) |
  |-----------|----------------------|----------------|
  | 일반 파일 | Ok, 내용 교체됨 | Ok, 내용 교체됨 |
  | **read-only 파일** | **Ok, 내용 교체됨** | **Err(PermissionDenied)** |

  Rust std의 `fs::remove_file`은 최근 버전에서 Windows read-only 속성을 무시하고 삭제하지만, `fs::rename`은 read-only 대상 교체를 거부한다. (정정: 최초 문서는 이를 Win32 `RemoveFile`의 특성으로 서술했으나, Win32 `DeleteFileW` 자체는 read-only 파일에서 `ERROR_ACCESS_DENIED`로 실패한다. 관찰된 동작은 Rust std 구현의 특성이다.)
- **왜 문제인가**: 포터블 모드가 실사용 경로다. `config/paths.rs:87, 105-132`가 `.portable` 마커/`OVERMAX_PORTABLE` 환경변수를 지원하며, 외부에서 복사해 온 `cache/` 파일이 read-only 속성을 가질 수 있다. 이 경우 캐시 갱신이 영구히 실패한다.
- **해법 후보 (정정, 미실측)**: 최초 문서가 제시한 "`ReplaceFileW` / `MoveFileEx(MOVEFILE_REPLACE_EXISTING)`"는 **해법이 아닐 가능성이 높다.** Rust `fs::rename`은 Windows에서 이미 `MoveFileExW(MOVEFILE_REPLACE_EXISTING)`(최근 버전은 `SetFileInformationByHandle(FileRenameInfoEx)`)를 사용하며, 위 `PermissionDenied`가 바로 그 경로의 결과다. `ReplaceFileW`도 교체 대상에 쓰기 권한을 요구한다. 실현 가능한 후보는:
  1. rename 직전에 대상이 read-only면 `fs::set_permissions`로 속성을 해제 (std만 사용, FFI 불필요)
  2. `SetFileInformationByHandle(FileRenameInfoEx)` + `FILE_RENAME_FLAG_IGNORE_READONLY_ATTRIBUTE` (Win32 FFI, Windows 10 1809+)

  1번이 diff가 가장 작다. 착수 전 두 후보 모두 read-only 대상 프로브로 확인한다. §3.4(b)의 `recommend_provider` 쓰기(현재 직접 `fs::write`, `bd421d7`)도 같은 방식으로 맞춘다.
- **결정 선행 조건**: 포터블 모드에서 복사된 read-only 캐시가 실제로 존재할 수 있는지 사용자 확인.

### 4.4 DXGI가 오류 한 번에도 즉시 GDI로 강등 — ⬇️ 의도된 fail-safe, 발생 사례 없음, 수정하지 않음

- **파일**: `rust/overmax_engine/src/capture/capture_engine/windows/mod.rs:166-174`
```rust
match dxgi.capture_bgra_inplace(rect, out_frame) {
    Ok(_) => Ok(()),
    Err(e) => {
        self.dxgi_backend = None;                       // 어떤 오류든 백엔드 파괴
        self.last_dxgi_init_attempt = Instant::now();
        self.fallback_to_gdi(rect, out_frame, &format!("DXGI capture failed ({e})"))
    }
}
```
- **문제**: `0x887A0027`(타임아웃) 외 **모든** 오류 — 일시적 `Map` 실패, `DXGI_ERROR_ACCESS_LOST` 1회, 그리고 §3.2 수정 이후의 출력 교체 실패 — 가 DXGI 백엔드를 파괴하고 GDI `BitBlt`로 강등시킨다. 이후 3초 쿨다운(`:145`) 동안 GDI 고정.
- **수정 방향**: HRESULT 코드로 판별하여 `ACCESS_LOST(0x887A0006)`일 때만 내부 `dup_result = None` 후 재협상(1회), 그래도 실패할 때만 `Err`을 올려 상위 폴백 유지. 상위 `mod.rs`는 문자열 대신 상수 비교.
- **미측정**: 3초 GDI 강등의 실측 성능 영향 미측정. Decision Log의 "~4ms vs ~30ms"는 2026-08-15 값이며 현재 아틀라스 경로와 비교 기준이 다르다.
- **재검토 (2026-10-07)**: 수정하지 않는다. (코드와 로컬 텔레메트리로 확인, 실기 재현은 하지 않았다.)
  - **HRESULT 오기 정정**: 위 수정 방향이 `ACCESS_LOST`라고 적은 `0x887A0006`은 `DXGI_ERROR_DEVICE_HUNG`이다. 실제 `DXGI_ERROR_ACCESS_LOST`는 **`0x887A0026`**(windows crate 상수로 확인). 타임아웃 `0x887A0027`은 맞다.
  - **의도된 설계다.** Decision Log 2026-08-15(`docs/decisions/capture_and_window.md`)가 "DXGI ACCESS_LOST/Timeout 및 3초 GDI Fail-Safe 폴백 안정화"로 이 동작을 명시한다. `dxgi.rs`는 타임아웃만 따로 처리하고 나머지 오류는 `Err`로 올려 `mod.rs`가 백엔드를 버린 뒤 3초 쿨다운 후 재생성한다. `ACCESS_LOST`에 duplication을 해제·재생성하는 것은 표준 처리이며 현재 구조도 같은 모양이다.
  - **오류 순간 프레임은 잃지 않는다.** 실패한 호출에서 곧바로 GDI로 같은 프레임을 캡처한다. 비용은 이후 약 3초의 GDI 고정이다.
  - **제안 수정의 비용**: 오류 종류별 분기와 내부 재협상 재시도의 폭주 방지(현재는 3초 쿨다운이 담당)를 새로 설계해야 한다. §3.2에서 출력 교체 실패를 전파하도록 바꾼 경로(`a58a502`)와도 변경이 겹친다.
  - **발생 사례 없음**: 로컬 `cache/telemetry.log` 9개 윈도우 합계 시도 353 / 성공 353 / 실패 0, "DXGI capture failed" 로그 0건(`telemetry.prev.log`는 비어 있음). 단일 세션 표본이라 부재의 증명은 아니다.
  - **재개 조건**: 사용자 로그/텔레메트리에서 DXGI 강등이 반복 관찰되면 재개한다. 그 경우 `ACCESS_LOST`(`0x887A0026`)만 분기해 재협상하고 그 외는 기존 폴백을 유지하는 방안을 검토한다.

### 4.5 DXGI 아틀라스 staging 텍스처가 Clear되지 않아 이전 프레임 픽셀이 남음 — ⬇️ 제안 수정 불가 + 발동 조건 좁음, 수정하지 않음

- **파일**: `rust/overmax_engine/src/capture/capture_engine/windows/dxgi.rs:795-806`
```rust
for slot in ATLAS_SLOTS.iter() {
    let src_x = local_left + slot.src_rect.x;
    if src_x < 0 || src_y < 0 || (... > desktop_width) || (... > desktop_height) {
        continue;          // ← 슬롯을 건너뛰기만 하고 지우지 않음
    }
    context.CopySubresourceRegion(...);
}
```
- **문제**: staging 아틀라스 텍스처는 `ensure_staging_atlas_textures()`에서 한 번만 생성되고 파일 전체에 Clear 호출이 없다(`grep Clear` 0건). 창이 화면 왼쪽으로 일부 벗어나 `local_left < 0`이 되면 일부 슬롯이 `continue`되고, 그 영역에는 **핑퐁 2세대 전 프레임의 픽셀**이 남아 현재 프레임 데이터로 인식된다. 최초 생성 직후에는 `CreateTexture2D(&desc, None, ...)`의 **미초기화 메모리**가 노출된다.
- **수정 방향**: 추상 계층 추가 없이 해당 함수 내부와 `DxgiCaptureEngine`에 RTV 필드 1개만 늘려 staging 텍스처 전체를 1회 clear.
- **재현 미완**: `local_left < 0` 발생 빈도 미확인. 미초기화 메모리 노출은 코드상 확실하나 재현하지 않았다.
- **재검토 (2026-10-07)**: 수정하지 않는다.
  - **제안된 수정은 성립하지 않는다.** 아틀라스 staging 텍스처는 `D3D11_USAGE_STAGING`, `BindFlags: 0`(`dxgi.rs:447-448`)이라 렌더 타깃이 될 수 없어 RTV로 `Clear`할 수 없다. CPU `Map(WRITE)`로 지우면 직전 GPU 복사와 stall이 생겨 핑퐁 더블버퍼링(Decision Log 2026-09-04)의 목적을 해친다.
  - **"미초기화 메모리 노출" 주장은 근거가 약하다.** 새 D3D11 리소스는 드라이버가 0으로 초기화하는 것이 일반적이라 노출되는 값은 쓰레기가 아니라 0일 가능성이 높다. 재현·확인하지 않았다.
  - **발동 조건이 좁다.** 슬롯 skip은 `copy_slots_to_atlas`의 범위 검사(`dxgi.rs:795-806`)에서만 일어난다. 정규화(normalizer) 경로는 항상 `0,0` 기준이라 skip이 없으므로, **창이 정확히 1920×1080이면서 일부가 모니터 밖으로 나간 경우**에만 해당한다. 이때 skip된 슬롯에는 해당 위치가 마지막으로 유효했던 프레임의 픽셀이 남는다.
  - **"해로운가"가 열려 있다.** 화면 밖 슬롯은 어차피 정상 인식이 불가능하다. 마지막 유효 상태 유지가 바람직한지, 0 채움으로 Unknown 처리가 바람직한지는 동작 정책이며 측정 근거가 없다. 비-아틀라스 경로(`crop_texture_to_buffer`)는 범위를 clamp해 더 작은 프레임을 만들어 이미 결과가 다르다.
  - **판단**: 발생 빈도·오인식 사례가 확인되지 않았고, 수정 대상은 캡처 경로(최신 수정 2026-09-04)다. 측정된 회귀가 없어 수정 근거로 부족하다.
  - **재개 조건**: 창을 모니터 밖으로 일부 밀어낸 상태에서 skip 슬롯과 인식 결과를 로그로 재현해 오인식이 확인되면 재개한다. 그 경우 후보는 (a) skip 발생 프레임에만 0으로 채운 DEFAULT 아틀라스 텍스처를 `CopyResource`해 GPU에서 덮는 방식(정상 경로 비용 0, 텍스처 1개 추가), (b) skip 시 프레임 무효 신호(§4.6과 계약이 얽힘)다.

### 4.6 DXGI 타임아웃이 동일 프레임을 `Ok`로 재전달 — ❌ 오진, 수정하지 않음

- **파일**: `rust/overmax_engine/src/capture/capture_engine/windows/dxgi.rs:627-639`
- **문제**: 정적 화면에서 매 tick **동일 프레임이 `Ok`로 재전달**된다. 호출자(`detection_worker.rs:454-509`)는 "새 프레임"과 "직전 프레임 재사용"을 구분할 수단이 없어, history 로직이 매 tick 동일 입력을 받아 안정화 카운터를 전진시킬 수 있다(AGENTS.md 「단일 프레임 판단보다 history 기반 접근」과 상충). 설계 의도는 Decision Log 2026-09-04 더블버퍼링이므로, 문제는 "Ok로 위장"이라는 점이다.
- **수정 방향**: `CapturedFrame`에 `pub reused: bool` 추가, timeout 경로에서 `true`, 호출자는 `reused`일 때 `pipeline.detect`를 스킵하고 `SleepHint`만 갱신.
- **미검증**: 동일 프레임 반복이 안정화 카운터를 실제로 오염시키는지 미확인.
- **재검토 (2026-10-07)**: 오진이다. 수정하지 않는다. (코드와 기존 테스트로 확인했으며 실기 재현은 하지 않았다.)
  - **카운터는 프레임 수가 아니라 벽시계로 게이트된다.** `hysteresis.update()`는 `process_frame_with_scene`에서만 호출되며 이는 `detect_scene_if_due`가 폴링 쿨다운(0.3/1.5/2.0초)을 넘겼을 때만 실행된다. `scene_streak`는 `commit_scene`에서만 증가하고 그 주석이 "cached ticks never call this method"라고 명시한다. 쿨다운 안의 틱은 `process_frame_cached`를 타며 이 카운터들을 건드리지 않는다. `detection_pipeline.rs`의 인게임 씬 테스트가 같은 프레임을 `start + 0.99`에 다시 넣어도 `scene_streak`가 1에서 변하지 않음을 이미 검증한다. 따라서 "매 tick 안정화 카운터 전진" 경로는 없다.
  - **재전달 프레임은 현재 화면이다.** Desktop Duplication 타임아웃은 화면에 변화가 없다는 뜻이므로 직전 프레임을 다시 읽는 것은 새로 캡처해도 같을 픽셀을 읽는 것이다. 아틀라스 더블버퍼에서도 타임아웃 시 읽는 `prev_idx`는 마지막으로 쓴 버퍼라 정상 경로의 1프레임 지연보다 오히려 최신이다.
  - **제안된 수정(`reused`이면 `detect` 스킵)은 기능을 깨뜨린다.** 결과창·인게임 씬은 두 번 연속 관찰로 확정된다(`scene_streak >= 2`). 폴링 사이 정적 화면은 새 프레임을 만들지 않으므로 둘째 관찰은 재전달 프레임에서 일어난다. 스킵하면 정적 결과창은 확정되지 않는다. 쿨다운, `unknown_since`(3초 후 폴링 주기 전환), `JACKET_MATCH_INTERVAL` 같은 시간 기반 로직과 §4.9의 `image_index` 재로드 반영도 틱에 의존한다. **`detect`는 새 프레임이 없어도 호출되어야 한다.**
  - **남는 것은 성능 질문뿐이다.** 재전달 틱마다 `copy_atlas_to_buffer`(Map, HDR이면 fp16→BGRA8 변환 512×512)가 돈다. 이 비용은 측정하지 않았으며 정확성 문제인 이 항목과는 별개의 후보다.

### 4.7 매 프레임 5회 win32 syscall + 읽히지 않는 필드 — ✅ 완료 (`2218b89`, `505e691`)

- **파일**: `rust/overmax_engine/src/capture/capture_engine/windows/mod.rs:130-131`, `:37`, `:50` 및 `detection_worker.rs:476`
```rust
let is_fs = self.tracker.is_fullscreen();   // ← 매 프레임 호출
self.current_is_fullscreen = is_fs;         // ← 읽는 곳이 없음
```
- **문제**: `is_fullscreen`은 `FindWindowW` + `GetWindowLongW` + `GetWindowRect` + `MonitorFromWindow` + `GetMonitorInfoW` 5회 syscall을 수행한다(`window_tracker/windows.rs:41-86`). Decision Log 2026-05 "WindowTracker 동적 폴링 주기 — win32u 시스템 콜 오버헤드 해소"의 의도를 우회한다. `detection_worker.rs:476`도 300ms 스로틀(`WindowQueryScheduler`)을 우회한다. `current_is_fullscreen`은 대입 2곳 외에 읽기가 없다.
- **수정 방향**: `mod.rs`의 `is_fullscreen` 호출과 `current_is_fullscreen` 필드 삭제(dead). `detection_worker.rs:476`은 `WindowQueryScheduler::update()`가 갱신하는 값을 스케줄러에 캐시해 재사용.
- **정정**: syscall 수는 "5회"가 아니라 **2~5회**다. 창이 `WS_POPUP`이 아니면(창 모드) `FindWindowW` + `GetWindowLongW` 2회 후 조기 반환한다.
- **조치**:
  - `2218b89`: `AdaptiveCaptureEngine`에서 `is_fullscreen` 호출, `current_is_fullscreen` 필드, 이 호출에만 쓰이던 `tracker` 필드를 제거. 도입 이후 모든 버전(`954b884`, `4c36b87`, `dcc9015`, `297f07e`, `0d6a07d`)에서 이 필드를 읽은 적이 없음을 확인했다. 캡처 동작 변경 없음.
  - `505e691`: Windows `tick`의 `is_fullscreen`을 `WindowQueryScheduler`의 조회 분기(rect/foreground와 같은 주기: 정지 300ms, 드래그 16ms)에서만 갱신하고 캐시값을 사용. 유일한 소비처는 IPC 스냅샷의 `fullscreen` 필드이며, Linux 경로(`:652`)는 이미 스케줄러 주기로 갱신되는 `WindowSnapshot` 값을 쓰므로 양 플랫폼의 갱신 주기가 같아졌다. 필드는 Windows `tick`에서만 쓰여 `cfg(target_os = "windows")`.
- **git blame 게이트**: 필드 `954b884`(2026-06-04), 호출 `4c36b87`(2026-06-04) 이후 여러 차례 수정된 안정화 코드다. §4.7 작업 요구 + 읽는 곳 없는 코드 제거라는 근거로 수정했다.
- **미검증**: 프레임 시간 개선량은 측정하지 않았다(주장 범위는 "캡처당 Win32 호출 2~5회 제거"). Linux 빌드는 로컬 크로스 체크가 openssl-sys 네이티브 의존성으로 불가해 CI(ubuntu-22.04)에 맡겼다.

### 4.8 GDI `release_resources` 순서 오류로 HBITMAP 누수 — ❌ 오진, 수정하지 않음

- **파일**: `rust/overmax_engine/src/capture/capture_engine/windows/gdi.rs:76-91`
- **원래 주장**: `SelectObject`의 이전 핸들을 복원하지 않아(`gdi.rs:67`) 비트맵이 DC에 선택된 상태에서 `DeleteObject`가 실패하고, 리사이즈마다 HBITMAP이 누수된다.
- **실측 반증** (`windows-sys 0.61.2` + `CreateDIBSection` 프로브):

  | 측정 | 결과 |
  |------|------|
  | 현행 순서(`DeleteObject` → `DeleteDC`) | `DeleteObject=true`, `DeleteDC=true` |
  | 제안 순서(`DeleteDC` → `DeleteObject`) | `DeleteDC=true`, `DeleteObject=true` |
  | 현행 순서로 5회 연속 생성/해제 | 5회 모두 `DeleteObject=true` |

  `DeleteObject`는 DC에 선택된 `CreateDIBSection` 비트맵에 대해 성공했고 누수가 관찰되지 않았다.
- **결론**: AGENTS.md 「버그가 실제로 재현됨」 요건 미충족. **수정하지 않음.** `SelectObject` 이전 핸들 미보관은 실측 근거가 없는 한 그대로 둔다.

### 4.9 `image_index.db` 갱신이 파이프라인에 반영되지 않음

- **파일**: `rust/overmax_data/src/community/cache_downloader.rs:252-257` ↔ `:102-119`
- **문제**: `refresh_image_index`는 `StartupCacheManager` 백그라운드 스레드에서 실행되고, `ImageIndexDb::load`(`store/image_index.rs:65`)는 엔트리를 `Arc<Vec<ImageEntry>>`로 메모리에 복사한 뒤 커넥션을 닫는다. `poll_updates`(`:102-119`)는 `updated_varchive_db`/`updated_sheet_meta`만 swap하고 `image_db`는 건드리지 않는다. 그 결과 `image_db_version.txt`에는 새 tag가 기록되어 다음 실행은 "최신 버전 유지 중"으로 로그하면서, **새 자켓 DB는 재시작 전까지 반영되지 않는다.** 사용자에게는 갱신 성공 로그만 보인다.
- **수정 방향**: 기존 `CacheUpdateResult`에 `updated_image_index: Option<PathBuf>` 필드 1개 추가 후 `poll_updates`에서 호출측에 알린다. 반영 호출부(엔진 파이프라인)는 미검증.

### 4.10 서버 JSON을 검증 없이 `raw_data`로 영속화 — ✅ 완료 (`e527501`, `58ead08`)

- **파일**: `rust/overmax_data/src/store/record_db/sync.rs:44-65`, `queries.rs:293`
- **문제**: `title`이 임의 문자열이면 `song_id` TEXT로 그대로 저장되고, `difficulty`도 `Difficulty::from_str` 검증 없이 저장된다. 서버가 `title: "abc"`를 주면 `load_varchive_records`(`queries.rs:293`)의 `parse().unwrap_or(0)`이 이를 **song_id 0으로 조용히 매핑**한다.
- **영향 정정 (2026-10-02)**: 최초 서술의 "Top-50에 오염 데이터가 섞인다"는 부정확하다. Top-50·레이팅 조회(`queries.rs:159, 211, 445`)는 이미 `parse::<i32>()`와 `Difficulty::from_str` 실패 행을 건너뛴다. 실제 오염 경로는 `load_varchive_records` → `RecordManager::refresh`(`record_manager.rs:61`)의 `varchive_cache` 하나다. 다만 **song_id 0은 실존 곡**(`cache/songs.json`의 기본 수록곡 "비상 ~Stay With Me~")이라, 잘못된 행이 이 곡의 V-Archive 기록(점수·맥스 콤보)으로 둔갑해 로컬 기록과 병합되는 rate map에 섞인다. 영향은 작지 않다.
- **조치**:
  - 쓰기 쪽 (`e527501`): 병합 시 `song_id.parse::<i32>()` 또는 `Difficulty::from_str` 실패 행을 `continue`. 테스트 `varchive_merge_skips_invalid_song_id_and_difficulty` — 수정 전 코드는 잘못된 행 2건(`"abc"`/`SC`, `"42"`/`XX`)을 그대로 저장했다.
  - 읽기 쪽 (`58ead08`): 기존 사용자 DB에 이미 저장된 행을 위해 `unwrap_or(0)`을 `let Ok(..) else { continue; }`로 교체(같은 파일의 다른 쿼리와 동일한 관용구). 테스트 `load_varchive_records_skips_stored_unparsable_song_id` — 수정 전 코드는 raw 삽입한 `"abc"` 행을 `(0, B4, SC)`로 반환했다.
- **git blame 게이트**: 병합 루프는 `e2875d3`(2026-07-16), 읽기 줄은 `0140d56`(2026-05-18)에서 유래하며 둘 다 `f9776f1`(2026-08-26, 모듈 분리)로 이동됐다. 버그 재현으로 수정 근거 충족.
- **남은 사항**:
  - difficulty는 검증만 하고 정규화(`as_str()`)하지 않는다. `Difficulty::from_str`은 `"normal"`, `"mx"` 등도 받으므로 소문자 값이 저장될 수 있고, 이 경우 `query_sync_candidates`(`queries.rs:336`)의 `r.difficulty = v.difficulty` 조인에서 로컬 기록(`as_str()` 대문자로 저장)과 짝이 맞지 않는다. V-Archive가 실제로 대문자 약어 외의 값을 보내는지는 미확인이라 정규화는 하지 않았다.
  - 전체 조회 응답의 **모든** 행이 무효면, 빈 배열 검사(§3.3)는 통과한 뒤 DELETE만 실행되어 캐시가 비워진다. 이전에는 쓰레기 행으로 채워졌으므로 악화는 아니다.

### 4.11 V-Archive API URL에 사용자 입력을 인코딩 없이 보간 — ✅ 완료 (`74094d8`, `8185d53`)

- **파일**: `rust/overmax_data/src/gateway/varchive.rs:126-137` (`fetch_records`), `fetch_single_song_records`
- **철회한 최초 진단**: "`v_id`와 `since`를 화이트리스트(`A-Za-z0-9-_.:`)로 제한" — `v_id`는 `settings_ui.rs:448`의 자유 입력(`trim()`만 거침)이고 실사용 값도 숫자가 아니다. 화이트리스트는 한글 등 정상 입력을 차단한다.
- **실측**: `reqwest::Url::parse`는 이미 존재하는 구분자를 URL 구조로 해석한다.

  | 입력 | `format!` 결과 | 쿼리 키 |
  |------|---------------|---------|
  | `abc&since=evil` (since) | `...?since=abc&since=evil` | **`["since","since"]`** — 인젝션 성립 |
  | `a/b?x=1#frag` (v_id) | `/archive/a/b?x=1#frag/button/4` | **`["x"]`** — 경로 절단 |
  | `한글아이디` (v_id) | `/archive/%ED%95%9C...%EB%94%94/button/4` | `["since"]` — 정상 |

  한글은 문제없고, **구분자를 포함한 값이 경로/쿼리 구조를 조작한다**는 것이 실제 위험이다.
- **조치**:
  - `since` (`74094d8`): `query_pairs_mut().append_pair("since", s)`로 교체. `HttpClient::get`을 `U: IntoUrl` 제네릭으로 변경해 파싱된 `Url`을 넘길 수 있게 했다.
  - `v_id` (`8185d53`): `Url::path_segments_mut().push()`로 세그먼트 단위 이스케이프(url 크레이트는 reqwest 경유로 이미 의존 트리에 있음). `fetch_single_song_records`도 같은 헬퍼 재사용, `title`은 `append_pair`.

    | v_id | `format!` | `path_segments_mut` |
    |------|----------|-------------------|
    | `a/b` | 경로 2단 분리 | `a%2Fb` — 세그먼트 유지 |
    | `a?x=1` | 키 `["x"]` — `since` 소실 | 키 `["since"]` |
    | `a#f` | 키 `[]` — `since` 소실 | 키 `["since"]` |
    | `한글아이디` | 정상 인코딩 | 동일 |

  - `&`는 `path_segments_mut`가 이스케이프하지 않는다(경로에서 합법 문자). 쿼리가 없는 경로 위치라 구조 조작이 없고 값도 보존되므로 허용했다(테스트로 고정).
- **검증**: 회귀 테스트 7건. 옛 `format!` 구현에서 4건(`a/b`, `a?x=1`, `a#f`, `since` 인젝션)이 실패하고 신 구현에서 통과함을 확인.

### 4.12 V-Archive 토큰이 로그로 노출될 수 있는 Debug derive — ✅ 완료 (`b7147a8`), 잔여 1건

- **파일**: `rust/overmax_data/src/gateway/varchive.rs:13-17, 26-32, 77-84`
- **문제**: `AccountInfo`가 `#[derive(Debug, Clone)]`라 `{:?}`로 토큰이 로그에 노출될 수 있었다. `RecordDB`에는 이미 `masked_steam_id`(`mod.rs:71-83`) 관례가 있다.
- **조치**: `Debug` derive 제거, 수동 `impl fmt::Debug`로 `user_no`/`token`을 `<redacted>`로 대체. `user_no`는 토큰과 짝을 이루는 계정 식별자라 함께 숨긴다.
- **잔여 (판단 필요)**: `upload_score` 실패 시 `message: e.to_string()`(`:102`)이 reqwest 에러 문자열을 UI(`sync_ui`)로 넘긴다. reqwest 에러의 Display는 **URL 전체를 포함**함을 실측했다:
  ```
  [DNS fail]  error sending request for url (https://this-host-does-not-exist-overmax.invalid/x)
  [refused]   error sending request for url (http://127.0.0.1:9/secret-path?token=abc)
  [timeout]   error sending request for url (https://httpbin.invalid/delay/30)
  ```
  V-Archive는 토큰을 `Authorization` 헤더로 보내므로(`varchive.rs:92`) 실제 노출은 경로(`v_id`) 정도로 제한되지만, Provider 등 다른 게이트웨이는 쿼리에 값을 실을 수 있다. 무조건 제거하면 진단 정보가 사라지므로, `UploadResult.message`를 UI로 넘기기 전 URL의 쿼리/프래그먼트만 마스킹할지 결정이 필요하다.

### 4.13 `image_index.rs::load()`가 매번 DDL을 실행 — ⬇️ 관찰 가능한 결함 없음, 수정하지 않음

- **파일**: `rust/overmax_data/src/store/image_index.rs:64-71`
- **문제**: `load()`는 파이프라인 초기화 시(`detection_worker.rs:396`) 호출되는데 매번 `ALTER TABLE images ADD COLUMN metadata TEXT`를 시도하고 `let _ =`로 무시한다. **실측**: 첫 로드 후 `PRAGMA table_info(images)`에 이미 `metadata`가 있는데도 매번 실패하는 DDL을 던진다. 읽기 전용 파일에서도 오류가 조용히 사라진다. `ImageIndexDb`는 WAL/busy_timeout도 설정하지 않아 `RecordDB::open_conn`(`mod.rs:86-94`)과 다르다.
- **최초 제안**: 컬럼이 **없을 때만** ALTER하도록 파일 내부 private 헬퍼 1개 추가.
- **재검토 (2026-10-02)**: 관찰 가능한 결함이 없어 수정하지 않는다.
  - **호출 빈도**: `load()`는 디텍션 파이프라인 초기화 시 1회 호출된다(`detection_worker.rs:396`). 프레임 경로가 아니다.
  - **실패 비용**: 중복 컬럼 ALTER는 SQL 파싱 단계에서 `duplicate column name: metadata`로 즉시 실패한다. 실측: 다른 연결이 `BEGIN IMMEDIATE`로 쓰기 잠금을 쥔 상태에서도 잠금 대기 없이 같은 오류로 즉시 반환되었다. 파일을 수정하지 않고 잠금도 잡지 않는다.
  - **현재 배포 DB**: 로컬 `cache/image_index.db`, `cache/image_index_orig.db` 모두 `metadata` 컬럼을 이미 갖고 있다. 이 ALTER가 의미 있는 경우는 `metadata` 이전(`f3adf09`, 2026-07-17 이전) DB뿐이다.
  - **판단**: 제안된 헬퍼는 동작을 바꾸지 않고 실패하는 문장 1개만 없앤다. AGENTS.md 기준 "더 깔끔해 보여서"에 해당하므로 단독 근거가 되지 않는다.
  - WAL/busy_timeout 미설정도 같은 이유로 결함이 아니다. 이 DB는 읽기 전용으로 1회 로드되고, 갱신은 파일 단위 교체(`write_atomic`)로 이루어진다.

### 4.14 스키마 마이그레이션 버전 관리 부재 (`PRAGMA user_version` 미사용) — ⏸️ 근거 부족, 사용자 판단 대기

- **파일**: `rust/overmax_data/src/store/record_db/schema.rs` 전체 / `store/image_index.rs:64-71`
- **문제**: `user_version` 사용 0건. 마이그레이션이 "컬럼이 있나?" 즉각 판정(`table_has_column`, `image_index.rs:66`)으로만 이루어진다. **실측**: `hog` 컬럼이 없는 구 스키마 DB에 `load()` → `Err("no such column: hog ...")` — `metadata`만 추가하므로 구 스키마를 복구하지 못한다.
- **최초 제안**: `RecordDB::initialize`와 `ImageIndexDb::load` 선두에 `PRAGMA user_version` read/write 추가. 기존 컬럼 판정 경로는 유지(호환).
- **재검토 (2026-10-02)**: 제안된 수정은 실측된 증상을 고치지 못한다.
  - **`image_index.db`**: 클라이언트가 만드는 DB가 아니라 GitHub 릴리스에서 내려받는 산출물이고, 태그가 바뀌면 파일째 교체된다(`cache_downloader.rs:241-262`). `hog` 컬럼이 없는 구 DB는 해시·HOG 값을 다시 계산해야 하므로 클라이언트 측 마이그레이션으로는 복구할 수 없다. 버전 번호를 기록해도 `Err("no such column: hog")`는 그대로이며, 실제 복구 경로는 재다운로드다. 로컬 두 DB 모두 `user_version = 0`이며 현행 스키마다.
  - **`record.db`**: 컬럼 존재 판정 마이그레이션은 §2.1·§4.15 이후 실패가 전파되고 회귀 테스트로 고정되어 있다. 재현된 결함이 없다.
  - **판단**: `user_version` 도입은 마이그레이션 체계에 대한 **설계 결정**이며, 현재 재현된 버그나 측정된 회귀가 없다. 앞으로 컬럼 판정만으로 표현할 수 없는 마이그레이션(컬럼 의미 변경, 데이터 변환)이 필요해질 때 도입을 검토한다. 그전까지 수정하지 않는 것을 권장한다.
  - **남는 실제 위험**: 구 스키마 `image_index.db`가 남아 있고 다운로드도 실패하면(§6-13 재시도 부재와 결합) 자켓 매칭이 비활성화된다. 이는 `user_version`이 아니라 다운로드 재시도 정책(§6-13)의 문제로 추적한다.

### 4.15 `initialize()`가 마이그레이션 실패를 삼키고 `is_ready = true`로 전환 — ✅ 완료 (`d6c2546`)

- **파일**: `rust/overmax_data/src/store/record_db/schema.rs:6-23`
```rust
if self.create_records_table(&conn).is_ok() && ... {
    self.ensure_schema(&mut conn);
    self.is_ready = true;
    return true;
}
```
- **문제**: `ensure_schema`가 `()`를 반환하고 내부 전부 `let _ =`이라 마이그레이션(§2.1 이후로는 `ALTER TABLE`)이 실패해도 `is_ready = true`가 된다. 이후 `upsert`/`get`이 "성공" 경로로 실행되며 `mod.rs:217 res.unwrap_or(false)`로 실패가 무변경과 구분되지 않는다.
- **조치 (`d6c2546`)**: `ensure_schema`가 `Result<()>`를 반환하고 내부 `let _ =`를 전부 `?`로 바꿨다. `initialize`는 `is_ready = ensure_schema(..).is_ok()`로 설정하고 그 값을 반환한다. `initialize`의 시그니처는 그대로다.
- **git blame 게이트**: 해당 라인의 마지막 수정은 `f9776f1`(2026-08-26, 모듈 분리)과 `d2954f9`(2026-09-30)이며, 로직 자체는 `cadad75`(2026-05-13)부터 존재했다. 버그 재현으로 수정 근거를 충족했다.
- **재현·검증**: 테스트 `initialize_reports_failure_when_migration_cannot_alter` 추가. 현행 스키마에서 `is_max_combo`만 DROP한 DB에 다른 연결이 `BEGIN IMMEDIATE`로 쓰기 잠금을 쥔 상태에서 초기화하면, ALTER가 `busy_timeout`(5초) 후 실패하는데도 수정 전 코드는 `initialize() == true`를 반환했다(테스트 실패 확인). 수정 후 `false`를 반환하고, 잠금 해제 후 재초기화하면 정상 마이그레이션되어 `is_max_combo=true` upsert가 보존됨을 확인했다. 테스트가 busy_timeout만큼 약 5초 걸린다.
- **후속 (완료, `19457c1`)**: 앱 호출부 `native_app.rs:367`이 `record_db.initialize()`의 반환값을 버려, 실패가 직후 `migrate_json_cache_to_db`의 "DB is not ready" 로그로만 간접 노출되었다. 실패 시 `log_tx`로 `[RecordDB] 기록 DB 초기화 실패` 로그를 남기도록 했다. git blame: `0140d56d`(2026-05-18), §4.15 후속으로 명시된 변경이라 수정 근거 충족. `NativeApp` 생성 경로라 단위 테스트는 붙이지 않았다(fmt·clippy만 확인).

### 4.16 `upsert`가 트랜잭션 없는 read-modify-write — ⬇️ 프로덕션 쓰기 스레드 1개, 수정하지 않음

- **파일**: `rust/overmax_data/src/store/record_db/mod.rs:152-216`
- **문제**: `with_retry`가 매 시도마다 새 커넥션을 열고(`:103`) SELECT와 INSERT 사이에 `BEGIN`이 없다. 두 스레드가 같은 키를 갱신하면 같은 `existing_rate`를 읽고 마지막 writer가 덮어쓸 수 있다.
- **수정**: 클로저 내부를 `BEGIN IMMEDIATE` … `COMMIT`으로 감싼다.
- **재현 실패**: 4스레드 × 50회 프로브에서 `raced_final_rate=93.49`로 정상 수렴. WAL + `busy_timeout=5000`이 자연 직렬화한 결과로 보이며, **재현 실패는 버그 부재를 증명하지 않는다.** 재현 전에는 착수하지 않는다.
- **재검토 (2026-10-07)**: 수정하지 않는다. (코드로 호출 경로 확인, 실기 재현은 하지 않았다.)
  - **`records` 테이블의 프로덕션 쓰기 경로는 전부 UI 스레드다.** `RecordManager::handle_verified_play` → `upsert`(`native_app_recommend.rs:63`)와 자동 업로드 전 로컬 반영 `record_manager.upsert`(`native_app.rs:1032`)는 UI 업데이트 루프(`&mut self`)에서 실행된다. `upsert_varchive_record`(`native_app.rs:726`)는 V-Archive 캐시 테이블이며 `records`가 아니다. IPC 서버(`ipc_server.rs`)는 `record_manager`를 `get_recent_records` 읽기에만 쓴다. `community/sync.rs`의 `rdb.upsert`는 테스트 코드에만 존재한다. `single_instance`가 중복 실행을 막으므로 프로세스 간 경합도 없다.
  - 따라서 같은 키에 대한 동시 read-modify-write는 현재 구조에서 도달 불가능하며, 위 "4스레드×50회 프로브 재현 실패"도 이 구조와 일관된다.
  - **재개 조건**: `records`를 쓰는 경로가 UI 스레드 밖에 추가될 때(예: IPC 쓰기 RPC, 백그라운드 동기화가 `records`에 직접 쓰기). 그 경우 위 수정(`BEGIN IMMEDIATE` … `COMMIT`)이 맞는 방향이다.

### 4.17 OCR 제거 후 남은 죽은 설정 필드와 잘못된 문서 서술 — ✅ 완료 (`5a0c831`, `41b3dab`)

- **파일**: `rust/overmax_data/src/config/settings.rs:456-457`, `settings.json:8`, `CONTEXT.md:11, 177`, `detection_pipeline.rs:1244`
- **문제**: `logo_ocr_cooldown_sec`를 읽는 소비자가 없다(정의 `settings.rs:457, 651`와 `settings.json:8`만 존재). `CONTEXT.md:11`의 "+ OCR (Windows OCR)", `CONTEXT.md:177`의 "Rate OCR 텔레메트리 지원", `detection_pipeline.rs:1244` 테스트 주석 "isolate OCR checksum bypass caches"는 2026-07-28 OCR 완전 제거 이후 유효하지 않다.
- **최초 제안**: 필드는 유지, CONTEXT.md 두 곳의 OCR 서술을 "Rate는 Pure Rust 템플릿 매칭(ZNCC) 기반"으로 갱신, 테스트 주석의 "OCR"를 "template cache"로 수정.
- **정정**: 제안 문구 "Rate는 ZNCC 기반"은 틀렸다. `templates/matching.rs` 기준 **Rate는 이진 템플릿 매칭**(`match_digits_template` → `overmax_cv::match_character`), **ZNCC 소프트 매칭은 Score**(`match_character_soft`, `:51`)에만 쓰인다. 또한 OCR 잔재는 2곳이 아니라 5곳이었다(`CONTEXT.md:11, 61, 64, 84, 177`). `:177`이 서술한 "Rate OCR 텔레메트리" 뷰는 `debug_ui.rs`에 존재하지 않는다.
- **조치**:
  - `5a0c831`: CONTEXT.md 5곳 정정. 인식 방식·엔진 구성·`overmax_cv` 역할·파이프라인 다이어그램(`OcrDetector` → `templates`)을 실제 코드대로 바꾸고, 존재하지 않는 텔레메트리 서술은 삭제. 남은 OCR 언급 3곳(`:11` 제거 시점 주기, `:128` 제거 이력, `:188` 1-Pass 규칙)은 정확한 서술이라 유지.
  - `41b3dab`: `detection_pipeline.rs:1244` 테스트 주석을 실제로 격리하는 대상인 `PlayStateDetector`의 ROI 체크섬 캐시(`mode_diff_cache`, `rate_cache`)로 수정.
  - `logo_ocr_cooldown_sec` 필드는 `settings.user.json` 호환을 위해 유지(「기존 호환성 파괴 금지」). **코드 로직 변경 없음.**

### 4.18 이진화 대비율 72% 롤백이 문서에 미반영 — ✅ 완료 (`98c2a9f`)

- **파일**: `rust/overmax_cv/src/image.rs:783` (`let calculated = (min as f32 + contrast * 0.65) as u8;`)
- **문제**: `docs/decisions/detection_pipeline.md:75`와 `CONTEXT.md:12`가 "72%로 정밀 튜닝"을 현재 상태로 서술했다. `e3c1884`가 0.65→0.72로 올렸고 `8b52da6`(2026-09-11, ZNCC 소프트 매칭 도입)가 되돌렸으나 문서가 갱신되지 않았다.
- **조치**: 코드 변경 없음. `CONTEXT.md`의 72%를 65%로 정정하고 `docs/decisions/detection_pipeline.md`에 롤백 행 추가.
- **잔여**: `image.rs:1153` 테스트 주석의 "기존 하드 이진화(72% 대비)" 표현은 그대로다(과거형 서술이라 오해 소지는 작음).
- **커밋 규율**: 원래 §3.4와 같은 커밋(`aeeb763`)에 섞여 있었으나 단독 커밋으로 분리했다.

### 4.19 `detect_rect_edges`의 margin 8이 unscaled

- **파일**: `rust/overmax_engine/src/detector/detection_pipeline.rs:803-807`
```rust
fn detect_rect_edges(frame: &CapturedFrame, roi: crate::detector::roi::RoiRect) -> Option<f32> {
    let margin = 8;
    roi.with_margin(margin)
```
- **문제**: `b54ce34`의 커밋 메시지는 "scale edge detection and category band margins dynamically based on ROI scale"이지만 스케일 파라미터를 추가만 하고 본문에서 쓰지 않았고(`let margin = 8;`), 이후 `58e7ea0`이 파라미터를 제거했다. `RoiRect`는 `transform_roi`로 이미 스케일되므로(`roi.rs:225-228`) 1440p에서 `player_panel`은 316×40 → 421×53이 되는데 margin은 8px 그대로다. 결정 기록과 코드가 불일치한다.
- **수정 방향**: `scale: f32` 파라미터를 받아 `((8.0 * scale).round() as i32).max(4)`로 계산, 호출부(`:531`, `:535`)에 `rois.scale()` 전달. **동작 변경이므로 1440p 회귀 스냅샷으로 검증 후 별도 커밋.**
- **git blame 게이트**: `b54ce34` (2026-08-09), `58e7ea0`.
- **미측정**: 1440p에서의 정량 영향 및 1440p 스냅샷 존재 여부 미확인.

### 4.20 IPC `/rpc`에 인증·rate limit 부재, 연결마다 무제한 스레드 — ⏸️ 설계 의도 확인 대기

- **파일**: `rust/overmax_app/src/system/ipc_server.rs:329-336`, `system/transport/loopback.rs:220-231, 276-346`
- **문제**: `loopback.rs:353`은 peer가 loopback인지만, `:389-391`은 Host 헤더가 `127.0.0.1`/`localhost`로 **starts_with**하는지만 검증하며 인증 토큰이 없다. `:220-231`은 연결마다 스레드를 무제한 생성한다(SSE만 `MAX_CLIENTS = 16`). `cmd_tx`는 unbounded channel이고, `dispatch_rpc`(`:310`)가 IPC 스레드에서 동기 DB 쿼리를 수행하므로 무제한 스레드와 결합하면 로컬 DoS 벡터(파일 핸들 고갈 + `busy_timeout=5000` 락 대기)가 된다.
- **수정 방향**: 연결별 스레드 수 제한(세마포어/풀). 인증 토큰은 `PROTOCOL_ID = "overmax-ipc/1"` 프로토콜 변경이므로 **사용자 사전 동의 필요**.
- **선행 조건**: 로컬 전용 IPC + 기본 `enabled = false`라는 **설계 결정**일 가능성이 있어 조치 전 사용자 확인.

### 4.21 벤치/검증 하네스가 릴리스 빌드에 포함 — ⬇️ 패키징 주장 오류, 수정하지 않음

- **파일**: `rust/overmax_app/Cargo.toml:18-24`
- **문제**: `build.bat:19`는 `cargo build -p overmax-app --release`만 수행하므로 `verify_pipeline`, `measure_breakdown`이 릴리스 빌드에 컴파일·패키징된다. `measure_breakdown.rs:23`은 Direct3D11 + Windows API를 직접 링크한다. `src/bin/benchmark_lowres.rs`는 `[[bin]]` 선언 없이 자동 탐색으로 빌드되어 동일 문제.
- **최초 제안**: 세 바이너리에 `required-features = ["bench-harness"]`, `[features] bench-harness = []` 추가.
- **재검토 (2026-10-02)**: "패키징된다"는 사실이 아니다.
  - **패키징 스크립트 3종 모두 `overmax-rs.exe`만 복사한다.** `scripts/package-rust.ps1`(`overmax.exe`로 복사 후 zip), `scripts/package-msix.ps1:109, 157`, `scripts/package-linux.sh` 모두 같다. 세 하네스는 `target/release/`에 **컴파일만** 되고 배포물(zip/MSIX/Linux 번들)에는 들어가지 않는다.
  - **남는 실제 비용은 릴리스 빌드 시간뿐이다.** 정량 측정은 하지 않았다.
  - **제안된 feature 게이트의 부작용**: `required-features`를 붙이면 CI(`ci.yml`, ubuntu-22.04·windows-latest 매트릭스)의 `cargo build --workspace`·`cargo clippy --workspace --all-targets`가 세 바이너리를 **건너뛴다.** 기본 feature에 넣지 않는 한 하네스가 컴파일 검증 없이 방치되어, 다음에 `verify_pipeline`을 쓰려 할 때 깨져 있을 수 있다. `verify_pipeline`은 Decision Log(2026-07-17)와 아틀라스 검증(`TASKS.md:54`)에서 회귀 확인 도구로 실제 쓰였다.
  - **판단**: 배포물 영향이 없고 빌드 시간 외 측정된 비용이 없으며, 게이트는 검증 도구를 CI에서 빼는 부작용이 있다. **수정하지 않는다.** 릴리스 빌드 시간이 문제가 되면 `build.bat`/패키징 스크립트에서 `--bin overmax-rs`로 빌드 대상을 좁히는 쪽이 CI 커버리지를 유지하는 대안이다.
  - **부수 발견**: `build.bat`은 `cargo build -p overmax-app --release` 후 `package-rust.ps1`을 호출하는데, 이 스크립트도 같은 빌드를 다시 실행한다(두 번째는 증분이라 사실상 no-op).

### 4.22 CV 파이프라인의 불필요 중복 작업 (성능 항목군)

모두 AGENTS.md 「성능 저하 야기 금지」를 근거로 하며 최소 diff가 가능한 항목이다. **정량 효과는 전부 미측정**이며, 구조적 중복만 확인했다. 각 항목은 수정 전후 계측을 붙여 개별 커밋으로 진행한다(「근거 없는 성능 개선 주장 금지」).

| 항목 | 파일:라인 | 내용 |
|------|-----------|------|
| 자켓 매칭 중 1회가 항상 버려짐 | `detection_pipeline.rs:757-788` | freestyle/openmatch를 둘 다 실행한 뒤 similarity로 승자 선택. `match_jacket`는 그리드 히스토그램 + 전체 DB 선형 순회를 포함한 최대 비용 연산(`jacket_matcher.rs:174-178, 232-262`). 이 구분은 `is_unknown`일 때만 필요하다(`:686-690`이 이미 전제). |
| 카테고리 띠 검사가 같은 픽셀을 2회 순회 | `detection_pipeline.rs:834-857` | 평균 계산 후 편차 계산에서 띠 픽셀 재순회. 절대 비용은 작지만 「다중 패스 루프 금지」 조항과 상충. 1차 패스에서 스택 버퍼에 담으면 2차 순회 제거. |
| 동일 자켓 ROI를 한 함수에서 2회 힙 복사 | `detection_pipeline.rs:406, 420` | `make_thumbnail(&jacket)` 내부 `to_image_region()` + `:420`의 재호출. 60×60×4 = 14.4KB를 ~4Hz 반복 복사. |
| 3종 해시가 동일 픽셀을 3회 리샘플링하며 5회 힙 할당 | `overmax_cv/src/image.rs:33-39, 61-85, 115-125` | `ahash`/`dhash`/`phash` 각각 리샘플 + `dct_2d_32`의 `vec![0.0; 1024]` 2개. 같은 파일의 다른 경로(`d472589`)는 이미 무할당화. |
| `detect_rate`가 버리는 이진화 버퍼 | `templates/matching.rs:6-7, 186, 225` | 유일한 프로덕션 호출자가 `binary`를 `_`로 버림. 129×32 기준 8KB 확정 낭비. 반환 타입을 `(String, u8, u8)`로 축소. |
| `median_result_rate`가 매 호출 Vec 할당 + 정렬 | `play_state.rs:302-306` | 윈도우 최대 7개 고정(`:297`). `[f32; 7]` 스택 배열로 대체. |
| `ImageView` zero-copy 계약이 해시·에지 진입점에서 깨짐 | `capture/frame_utils.rs:130-141, 94-105` | `to_image_region()`이 매번 소유 `Vec<u8>` 할당 + 전 행 복사. `play_state.rs:690, 715`가 매 프레임 `compute_hashes(4)` 호출. `crop`(`:66-85`)이 stride == width*4를 보장하므로 슬라이스 직접 전달 가능. |
| 씬 미스마다 비용을 내는 텔레메트리 경로 | `detection_pipeline.rs:349-350, 360-364` | `screen_static_thumb_diff`가 크롭 + 힙 복사 + 그레이 + resize를 수행하나 결과는 stats 로깅으로만 소비. 텔레메트리 비활성 시 스킵 가드 1개. |

### 4.23 Linux 경로의 정규화 부재로 Windows와 인식 결과가 달라질 수 있음 — ⏸️ 측정 전 보류

- **파일**: `capture_engine/linux.rs:608-620` vs `windows/dxgi.rs:511-624`, `windows/normalizer.rs:34-77`, `detector/roi.rs:212-233`
- **문제**: Windows는 512×512 GPU 아틀라스 또는 1920×1080 GPU bilinear 정규화로 재샘플링한다. Linux는 정규화/아틀라스 경로가 없고 `capture_bgra_inplace`의 `rect`를 `_rect`로 버린다(`linux.rs:138`). 소비측 `RoiManager::calculate_transform`은 `roi.rs:243`에서 정수 절삭으로 ROI를 만든다. 동일 게임 상태라도 ROI 픽셀이 달라 같은 임계값에 대한 인식 결과가 달라질 수 있다. `atlas_layout.rs`의 47슬롯 검증 테스트는 Windows 아틀라스 경로에만 적용되어 Linux 커버리지는 0이다.
- **수정 방향**: 지금 단계에서 Linux 정규화는 도입하지 않는다. `_rect` 대신 실제 `WindowInfo` geometry로 ROI 스케일을 명시하는 것만 권고(동작 변경 없음).
- **미측정**: 실제 오인식률 영향은 측정하지 않았다. 실측 전에는 버그로 단정하지 않는다.

### 4.24 Linux 풀 프레임 2회 순회

- **파일**: `rust/overmax_engine/src/capture/capture_engine/linux.rs:573-603` (특히 `:596-598`)
```rust
for (source, destination) in generation.map.chunks_exact(generation.stride)
                                      .zip(out_frame.bgra.chunks_exact_mut(row)) {
    destination.copy_from_slice(&source[..row]);
    for alpha in destination[3..].iter_mut().step_by(4) { *alpha = 255; }
}
```
- **문제**: Windows 아틀라스 경로가 47개 슬롯만 CPU로 옮기는 것과 달리 Linux는 풀 프레임을 매 캡처 복사하고 4바이트마다 별도로 쓴다(1080p 기준 2,073,600회). 기능상 필요한 작업(알파 강제)이지만 프레임 전체를 2회 순회한다.
- **수정**: 알파 강제 루프를 `chunks_exact_mut(4)`로 바꾸는 2줄 변경에 한정. 계측 후 진행.

### 4.25 문서-코드 드리프트 및 릴리스 추적 누락 — ⚠️ 부분 완료 (`4bcfbf2`)

- ✅ **아틀라스 슬롯 수 불일치** (`4bcfbf2`): 코드는 `ATLAS_SLOTS: [AtlasSlot; 47]`(`atlas_layout.rs:24`)인데 `CONTEXT.md`/`TASKS.md`가 43슬롯으로 기재했다. 47슬롯 / 217,952 px로 5곳 정정.
- ⏳ **PR #27 기능이 TASKS.md에 미등록**: `TASKS.md`에서 `gameplay|paused|인게임 씬|일시정지` 0건. 인게임/Paused 씬 감지, 씬 독립 Global ROI + `gp_*`/`pause_title` 패킹, IPC 스냅샷 씬/컨텍스트 분리가 릴리스 추적에서 빠졌다. `[Active] Milestone v0.4.1`은 1~5번 섹션만 있고 `8.1 래더매치 씬 감지 대응`은 `[ ]` 상태.
- ⏳ **릴리스 노트가 코드 상태를 반영 못 함**: `Cargo.toml:11`은 `0.4.1`이고 `RELEASE_NOTES_v0.4.1.md`는 2026-09-08 이후 갱신되지 않았는데 9월 27일 커밋들이 대량 병합됐다. 노트에 인게임 씬 언급 0건.
- ⏳ **README에 현재 버전 표기 없음**: `README.md:103-105`, `README.en.md:103-105` 모두 `v0.5.0 로드맵`만 있고 `0.4.1` 표기 0건. AGENTS.md Release Protocol 체크리스트 3번 미수행 상태.
- ⏳ **미완료 계획 문서**:
  - `docs/plans/2026-09-23-atlas-optimization-game-cycle-plan.md:26` — `Step 4: 캡처 파이프라인 연동 검증` 체크박스 미갱신. 판단 자체는 Decision Log 2026-09-27 행에 기록되었으나 **신규 ROI의 DXGI 실캡처 실측 검증은 수행된 적이 없다.**
  - `docs/plans/2026-09-23-gameplay-scene-pipeline-redesign.md:156` — 실앱 GDI/DXGI 상태 전이 확인 미실행. 같은 문서 97행에 "가정으로 메우지 않고 기록함"이라 적었으나 이 미검증 사실이 `TASKS.md`/`CONTEXT.md`로 전파되지 않았다.
- ⏸️ **CONTEXT.md 변경 이력 섹션 부재**: Release Protocol 체크리스트 4번이 요구하는 이력이 `docs/decisions/`로 위임된 구조인지 누락인지 사용자 판단 필요.
- ❌ ~~CONTEXT.md에 Gameplay ROI의 씬 독립(Global) 설명 누락~~ — **오탐(철회).** `CONTEXT.md:131`에 이미 존재한다(`a278aaf`, 2026-09-27). 리뷰 워커가 `grep 'gp_'` 범위를 잘못 좁혀 판단했다. 재검토 시 `CONTEXT.md` 130~134행 전체를 읽을 것.

잔여 항목은 릴리스 전략(버전 표기, 노트 범위) 판단이 필요해 보류한다.

---

## 5. LOW

즉시 조치 대상이 아니며, **삭제/변경 시 사용자 승인이 필요한 항목**이다. AGENTS.md 「무관한 포맷팅/정리를 기능 변경과 같은 커밋에 섞지 않는다」에 따라 각각 별도 커밋으로 취급한다.

| 파일:라인 | 내용 |
|-----------|------|
| `overmax_engine/src/capture/capture_engine/windows/dxgi.rs:44, 142-144, 207, 384-385` | `hdr_lut: Arc<[u8; 65536]>`가 선언·대입만 있고 **읽는 곳이 없다**. `build_lut_table`(`hdr_pipeline.rs:111-128`)이 65,536회 `powf`를 엔진 생성 시·화이트레벨 변경 시·출력 교체 시마다 수행한다. 전역 `ACTIVE_HDR_LUT`/`get_active_lut`도 호출자 없음. |
| `overmax_app/src/ui/overlay_ui.rs:92`, `linux_layer_overlay.rs:1404`, `native_app_viewports.rs:891` | `OverlayProps.record_manager`가 `ui/` 하위에서 읽히지 않음. DB 핸들을 매 프레임 UI props에 넘기는 구조 자체가 위험. `OverlayProps` 쪽만 먼저 제거 권장(`LinuxOverlaySnapshot`의 것은 `Arc::ptr_eq` 비교에 쓰이므로 별도). |
| `overmax_app/src/ui/sync_ui.rs:371` | `ui.add_sized([170.0, 20.0], egui::Label::new(""))` — 빈 Label이 spacer 용도로 남은 것으로 보임. `ui.allocate_space(egui::vec2(170.0, 20.0))`로 동작 동일. |
| `overmax_app/src/ui/linux_layer_overlay.rs:408` | `bytes.try_into().expect("four-byte chunk")` — `chunks_exact(4)`가 보장하므로 도달 불가. 다만 Wayland 백엔드 스레드에서 호출되어 `panic = "abort"`(release 프로파일)로 프로세스 전체가 종료되는 지점이다. |
| `overmax_app/src/ui/debug_ui.rs:129, 154, 189, 225, 252, 274, 302, 330, 358, 385, 416, 464, 516` | 디버그 창 섹션 헤더 13곳이 `t!` 매크로를 우회한 영어 하드코딩. `i18n.rs`는 다수 편집 이력으로 **git blame 게이트 적용 대상**(`docs/decisions/ui_and_i18n.md:26-27,35`). 개발자 전용 뷰포트라 플레이어 영향 없음. |
| `overmax_engine/src/detector/templates/gameplay_scene.rs:18-25` | `matches!((frame.width, frame.height), (512, 512) \| (1920, 1080))` 해상도 화이트리스트. 불일치 시 `read_scene`이 **로그 없이** `Unknown`을 반환하고(`:28-30`), 파이프라인은 `detection_pipeline.rs:155-159`에서 인게임 히스토리를 폐기한다. GDI 백엔드에서 네이티브 1440p 프레임이 오면 인게임 탐지가 조용히 비활성화된다. |
| `overmax_data/src/community/cache_downloader.rs:122-141, 240-241` | `has_all_required_caches`의 `db_path`가 설정값이라 `root.join("../../..")`로 `root`를 벗어날 수 있고, `write_atomic`이 `create_dir_all(parent)`로 탈출 경로에 파일을 쓴다. `starts_with(root)` 가드로 차단 가능. |
| `overmax_data/src/community/sync.rs:108-120, 164-168` | `partial_cmp().unwrap_or(Ordering::Equal)`이 NaN을 무시해 정렬 순서가 비결정적이 된다. `records`는 `rate > 0` 필터로 걸리나 `varchive_records.rating`은 명시적 방어가 없다. |
| `overmax_engine/src/detector/telemetry.rs:439`, `capture_engine/windows/hdr_pipeline.rs:4` | `#[allow(dead_code)]`인 `environment_value`와 `SCRGB_REFERENCE_WHITE_NITS` 모두 호출자 없음. 후자는 "80"이라는 물리 기준이 미사용 상수 1곳과 하드코딩 계산식 2곳에 분산됨. |
| `overmax_engine/src/capture/capture_engine/linux.rs:110, 145, 560` | `expect("initialized X11 connection")`, `expect("capture generation")` 3건. 현재는 도달 불가하나 리팩토링 한 번에 깨질 수 있는 지점. `ok_or("...")?`로 교체 권고. |
| `.gitignore:15-25` | Python/PyInstaller용 패턴(`dist/`, `build/`, `lib/`, `lib64/`, `var/`, `parts/`)이 저장소 전체에 적용. `build/`와 `dist/`는 2회 기재. `/dist/`, `/build/`처럼 루트 앵커로 좁히는 단독 커밋 권장. |
| `rust/overmax_engine/src/detector/templates/gameplay_scene/pause_title-play-092.gray` (`21c9652`) | diff stat에서 텍스트("1 +")로 기록되었으나 실제는 4144 bytes 8비트 그레이스케일 바이너리. `.gitattributes`에 `*.gray binary` 추가 권장(단독 커밋). |
| `overmax_data/src/service/recommend/scoring.rs:575-607` | `#[cfg(test)] fn derive_top50_base_floor`가 테스트(`tests.rs:436-491`)에서만 사용됨. blame `9e2b139`(2026-08-31)이며 제거 시 테스트가 깨지므로 **그대로 둔다**. |

### 남은 `#[allow(dead_code)]` 4건 (정상 판정)

`detection_worker.rs:218`(`presentation_observation` — Linux `tick_linux`에서만 읽힘), `dxgi.rs:212`(`active_sdr_white_level` 게터), `dxgi.rs:431`(`enable_gpu_atlas` 게터 — 설정 주입 경로는 확인되나 읽기 소비자는 grep 미확인), `overlay_theme.rs:53`(`DANGER` 색상 상수).

### 테스트 커버리지 갭

리뷰 시점에 `store/record_db/{queries,schema,sync}.rs`와 `gateway/{asset_download,error,recommend_provider,varchive}.rs`에 `#[cfg(test)]` 모듈이 없었다. 통합 검증은 `record_db/mod.rs`의 테스트와 `recommend/tests.rs`(1966줄)가 담당한다. 이후 §2.1·§3.3 회귀 테스트가 `record_db/mod.rs`에, §4.11 회귀 테스트가 `varchive.rs`에 추가되었다. `recommend_provider.rs`의 endpoint 해석은 §3.4 후속(`24c8d4a`)에서 커버되었다. 스키마 마이그레이션 실패 경로는 §4.15(`d6c2546`)의 잠금 재현 테스트로 커버되었다.

---

## 6. 미검증 (Unverified)

판단을 보류한 항목. 각 항목의 "닫는 방법"이 선행 조건이다.

1. **§4.19 margin 8 unscaled의 실제 오차 크기.** 1440p 스냅샷 존재 여부 미확인. 닫는 방법: margin 스케일 적용 전후 엣지 strength 비교.
2. **§4.22 CV 중복 작업의 실측 기여도.** `cache/image_index.db` 크기(`phash_list.len()`), 씬 폴당 `match_jacket` 비용이 전체 detect 시간 대비 몇 ms인지 미산출.
3. **§4.5 미제거 슬롯이 실제 오인식을 유발하는 조건.** `local_left < 0` 발생 빈도 미확인.
4. **§4.6 타임아웃 `Ok` 재전달이 안정화 카운터를 오염시키는가.** `detection_pipeline`의 history/stable 카운터 로직을 끝까지 추적하지 않았다.
5. **§4.4의 3초 GDI 강등이 실측 성능에 미치는 영향.** 현재 아틀라스 경로 기준 수치 미측정. §3.2 수정 이후 강등 빈도가 늘었는지도 미확인.
6. **§4.23 Windows bilinear vs Linux 정수 절삭의 인식 불일치 크기.** 메커니즘은 확실하나 오인식률 영향 미측정.
7. **§4.16 upsert read-modify-write 경쟁.** 4스레드 × 50회 프로브에서 재현 실패. 부하 조건 의존.
8. **§4.20 무인증 IPC의 실제 익스플로잇 가능성.** Threat Model 부재. 설계 결정 여부 사용자 확인 필요.
9. **`uses_manual_position`의 Windows/Linux 조건 불일치.** Linux(`linux_layer_overlay.rs:1498-1503`)는 `snap == "manual" || !window.fullscreen`, Windows(`native_app_viewports.rs:890`)는 `snap_position == "manual"`만 본다. `docs/decisions/linux_support.md` 확인 대기.
10. **§2.2 잔여·§4.2 `get_merged()`의 실제 프레임 비용.** "매 프레임 호출 + deep clone + 역직렬화"까지만 확인. 수치를 지어내지 않았다.
11. **멀티모니터 + `engine: "auto"` 기본값의 현재 성능.** `mod.rs:136-140`이 멀티모니터에서 GDI를 우선한다. 「성능 저하 금지」와 상충 가능하나 실측 없음. 사용자 프로필 확인 필요.
12. **i18n 미번역 키 잔존 여부.** `i18n.rs`의 Ja 분기 개수(157)와 정적 키 수를 정확히 대조하지 못했다. `docs/decisions/ui_and_i18n.md:35`의 "Ja 번역 100% 완료" 주장 미검증.
13. **캐시 1회 실패 후 재시도 부재가 설계인지 누락인지.** `cache_downloader.rs:166-206`에서 실패해도 mtime이 갱신되지 않아 `is_stale`은 true로 남지만, `StartupCacheManager`가 1회만 호출하므로 같은 프로세스 내 재시도가 없다. 일시적 DNS/5xx에 대한 자가 복구 부재. `docs/decisions/data_and_sync.md`에 관련 항목 없음 → 사용자 확인.
14. **`ensure_dirs_and_seed`(`paths.rs:231-266`)의 포터블→Installed 전환 시 `record.db` 복사.** WAL 모드 DB를 `fs::copy` 단일 파일로 복사하므로 미체크포인트 `-wal` 데이터가 유실될 수 있으나 재현하지 못했다.
15. **`recommended` provider 캐시 읽기의 UI 스레드 지연.** `composite.rs:93-105`의 `read_to_string`이 `refresh_overlay_data`(UI 스레드, `changed` 시점에만 호출)로 들어온다. 파일이 큰 경우 지연 미측정(§3.4의 크기 상한 부재와 연관).
16. **`LATEST_STATE.try_lock` 갱신 누락의 실 영향.** `ipc_server.rs:355, 358`. IPC 전용 관찰자라 파이프라인 영향은 없음. 경합 빈도 미측정.
17. **`dxgi.rs:644-648`의 `0x887A0027` 외** OS 버전별 다른 타임아웃 코드 존재 여부 미확인.
18. **테스트 전용 `unwrap()`의 프로덕션 유입 가능성.** `include!` 경로를 전체 추적하지 않았다(현 구조상 불가능하나 명시 확인 안 함).
19. **§3.3 전체 조회 빈 배열의 정상 발생 가능성.** V-Archive가 기록 없는 버튼 모드에 빈 배열을 주는지, 오류를 주는지 미확인. 닫는 방법: 기록 없는 모드로 실제 API 응답 확인. (현재 방식 유지는 결정됨, 이 항목은 결정을 다시 검토할 근거 확보용이다.)
20. **§4.3.1 해법 후보의 실효성.** `set_permissions` 해제 후 rename, `FILE_RENAME_FLAG_IGNORE_READONLY_ATTRIBUTE` 모두 read-only 대상 프로브로 확인하지 않았다.

### 해소된 미검증 항목

- ~~`hdr_replay_test` 실패의 파일 단위 원인~~ → §3.1에서 규명(구 43슬롯 스냅샷 에셋).
- ~~§4.8 HBITMAP 누수 심각도~~ → 실측으로 누수 없음 확인, 오진 처리.

---

## 7. 착수 계획

### 7.1 완료 내역

| 순서 | 항목 | 커밋 |
|------|------|------|
| 1 | §3.1 hdr_replay `#[ignore]` | `2ed4543` |
| 2 | §4.25 아틀라스 슬롯 수 정정 | `4bcfbf2` |
| 3 | §2.1 레거시 DB `ALTER TABLE` | `d2954f9` |
| 4 | §2.2 업로드 플래그 캐시 | `ac1063b` |
| 5 | §3.3 빈 응답 캐시 보존 | `ef56960` |
| 6 | §3.5 Linux IPC 표시 명령 | `9a556ca` |
| 7 | §3.2 DXGI 출력 교체 실패 전파 | `a58a502` |
| 8 | §4.11 `since` 쿼리 이스케이프 | `74094d8` |
| 9 | §4.11 `v_id` 경로 세그먼트 | `8185d53` |
| 10 | §4.12 `AccountInfo` Debug 마스킹 | `b7147a8` |
| 11 | §3.4(a) 호스트 검증 | `070cfa8` |
| 12 | §3.4(b) 임시 파일 + rename 쓰기 (이후 되돌림) | `63ae6d4` |
| 13 | §4.18 이진화 대비율 문서 정정 | `98c2a9f` |
| 14 | §3.3 주석·docstring 정정 | `44f2d5f` |
| 15 | §3.4(a) origin 비교 강화 + 회귀 테스트 | `24c8d4a` |
| 16 | §3.4(b) 되돌림 (read-only 회귀 위험) | `bd421d7` |
| 17 | §4.15 마이그레이션 실패를 `is_ready`에 반영 | `d6c2546` |
| 18 | §4.15 후속: 앱 시작 시 초기화 실패 로그 | `19457c1` |
| 19 | §4.1 오진 확인: `with_retry` 3회 계약 테스트 | `e93176d` |
| 20 | §4.10 쓰기 쪽: 해석 불가 V-Archive 행 저장 안 함 | `e527501` |
| 21 | §4.10 읽기 쪽: 저장된 해석 불가 song_id 건너뜀 | `58ead08` |
| 22 | §4.17 CONTEXT.md OCR 잔재 정정 | `5a0c831` |
| 23 | §4.17 테스트 주석 정정 | `41b3dab` |
| 24 | §4.7 캡처 엔진 dead 필드와 매 프레임 조회 제거 | `2218b89` |
| 25 | §4.7 Windows fullscreen 플래그를 조회 주기에 맞춤 | `505e691` |

### 7.2 2026-10-02 후속 리뷰 지적 사항 (우선 처리)

1. ~~**`aeeb763` 분리**~~ — **완료.** `070cfa8`, `63ae6d4`, `98c2a9f`로 분리(push 전, 사용자 승인).
2. ~~**§3.4(a) origin 비교**~~ — **완료** (`24c8d4a`).
3. ~~**§3.4(b) 재결정**~~ — **되돌림** (`bd421d7`). 잘린 쓰기 위험은 §3.4 재검토 사항 2에 수용 근거와 함께 기록.
4. ~~**§3.3 주석·docstring 정정**~~ — **완료** (`44f2d5f`).
5. ~~**§3.3 트레이드오프 사용자 확인**~~ — **결정: 현재 방식 유지** (2026-10-02).

### 7.3 다음 착수 대상

1. ~~**§4.15**~~ — **완료** (`d6c2546`, 앱 로그 후속 `19457c1`).
2. ~~**§4.1**~~ — **오진** (`e93176d`). 루프 밖 코드는 도달 불가, op은 정확히 3회 실행됨을 테스트로 고정.
3. ~~**§4.10**~~ — **완료** (`e527501`, `58ead08`).
4. ~~**§4.13 → §4.14**~~ — **수정하지 않음.** §4.13은 관찰 가능한 결함 없음, §4.14는 실측 증상을 고치지 못하는 설계 결정이라 §7.4 보류로 이동.
5. ~~**§4.17**~~ — **완료** (`5a0c831`, `41b3dab`).
6. ~~**§4.21**~~ — **수정하지 않음.** 패키징 스크립트가 `overmax-rs.exe`만 복사하므로 배포물 영향 없음. feature 게이트는 CI에서 하네스를 빼는 부작용이 있음.
7. ~~**§4.7**~~ — **완료** (`2218b89`, `505e691`). **§4.2** — 프레임 경로 deep clone 제거. 계측 동반.
8. **§4.4~§4.6** — DXGI 오류 분류·staging clear·reused 플래그. §3.2 부수 효과와 함께 검토.
9. **§4.22, §4.24** — 성능 항목. 각각 수정 전 계측 필수.

### 7.4 보류 (사용자 판단 필요)

- §4.3 `write_atomic` 해법 (포터블 read-only 캐시 존재 가능성). 정해지면 §3.4(b) `recommend_provider` 쓰기에도 적용
- §4.12 잔여: 에러 메시지 URL 마스킹 여부
- §4.20 IPC 인증/스레드 제한 (설계 의도)
- §4.23 Linux 정규화 (측정 선행)
- §4.25 잔여: 버전 표기·릴리스 노트·TASKS.md 등록 (릴리스 전략)
- §6-13 캐시 재시도 부재가 설계인지
- §4.14 `user_version` 도입 여부 (권장: 컬럼 판정으로 표현 못 하는 마이그레이션이 생길 때까지 보류)

**여러 finding을 한 커밋에 묶지 말 것.** 각 diff는 하나의 검증 가능한 주장만 담는다(AGENTS.md 「Diff & Commit Discipline」).

---

## 8. 리뷰 과정 산출물 정리 상태

- ✅ 진단용 worktree(`om_base`, 고아 등록 `_wb_check`)와 `target_base_check/` 디렉터리는 정리되었다(2026-10-02 `git worktree list` 기준 메인 1개).
- ⚠️ `stash@{0}: On main: feat(cv): experiment with soft template matching for score` — 리뷰 이전부터 있던 것인지 확인하지 못했다. **삭제하지 말 것.**

---

## 9. 참고: 리뷰 방식

5개 도메인으로 분할하여 병렬 정적 리뷰를 수행했다.

| 워커 | 범위 | 핵심 파일 |
|------|------|-----------|
| CV/디텍션 | `overmax_cv`, `overmax_engine/src/detector/**` | `detection_pipeline.rs`, `play_state.rs`, `atlas_layout.rs`, `image.rs` |
| 캡처 | `overmax_engine/src/capture/**` | `dxgi.rs`, `gdi.rs`, `linux.rs`, `frame_utils.rs` |
| 데이터 | `overmax_data/**` | `config/`, `store/`, `service/recommend/`, `community/`, `gateway/` |
| UI/시스템 | `overmax_app/**`, `overmax_core/**` | `native_app*.rs`, `linux_layer_overlay.rs`, `ipc_server.rs`, `i18n.rs` |
| 저장소 위생 | 문서/커밋/테스트/grep | `CONTEXT.md`, `TASKS.md`, `docs/`, `Cargo.toml` |

각 finding은 실제 파일을 읽고 `파일:라인`을 인용했으며, 판단이 불가능한 항목은 미검증으로 분리했다. §2.1(DROP TABLE), §3.3(V-Archive 캐시 소실), §4.3(rename 동작), §4.8(GDI 해제 순서), §4.11(URL 구조 조작), §4.13(table_info) 등은 **실제 실행으로 재현 또는 반증**되었다.

### 리뷰 오류 기록

이 리뷰에서 철회·정정된 판단. 재검토 시 같은 실수를 반복하지 않기 위해 남긴다.

| 항목 | 최초 판단 | 정정 | 원인 |
|------|-----------|------|------|
| §3.1 | 씬 감지 제품 코드 회귀 | 구 아틀라스 스냅샷 에셋 문제 | 유효하지 않은 파일별 이분법 후 추측 |
| §4.8 | HBITMAP 누수 | 누수 없음 | GDI 상식을 실측 없이 적용 |
| §4.11 | 화이트리스트 필터 | `query_pairs_mut`/`path_segments_mut` | 실사용 입력 분포 미확인, URL API 탐색 부족 |
| §4.3 | `remove_file` 제거 | read-only 회귀로 되돌림 | 일반 파일만 실측 |
| §4.25 | Global ROI 문서 누락 | 이미 존재 | grep 범위를 좁혀 인접 행 미확인 |
| §3.3 | "증분 동기화" 문제 | 전체 조회 문제 | 호출부 `clear_first` 산출식 미확인 |
| §4.3.1 | `MoveFileEx`/`ReplaceFileW`가 해법 | 해법 아닐 가능성 높음 | std `rename`의 내부 구현 미확인 |
| §4.17 | "Rate는 ZNCC 기반", OCR 잔재 2곳 | Rate는 이진 매칭(ZNCC는 Score), 잔재 5곳 | 매칭 함수 호출부와 문서 전체를 확인하지 않음 |
| §4.21 | 하네스가 릴리스에 패키징됨 | 컴파일만 되고 배포물엔 없음 | `cargo build` 대상과 패키징 스크립트의 복사 대상을 구분하지 않음 |
| §4.1 | `with_retry`가 op을 4번째 실행 | 루프 밖 코드 도달 불가, 정확히 3회 | 분기 가드(`attempt < 2`)를 따라가지 않고 코드 모양으로 판단 |
| 인용 | 「추상 추가 금지」(AGENTS.md) | AGENTS.md에 없는 조항 | 규약 원문 미대조 |
