# 모듈화 기준선 (단계 0)

> 대상: [orphera/overmax#43](https://github.com/orphera/overmax/issues/43) · 계획: [2026-11-19-modularization-plan.md](2026-11-19-modularization-plan.md)
> 측정 시점 코드: `main` `a3f16c3` (브랜치 `refactor/modularization` 분기점) · 2026-11-19 · rustc 1.96.0 · Windows 10 Pro 19045
> 단계 7에서 같은 절차를 반복해 이 문서의 값과 비교한다.

---

## 1. 인식 기준선 (`verify_pipeline`)

### 결과 요약

| 항목 | 값 |
| :--- | :--- |
| 판독 케이스 | **109건** (로드 실패 0) |
| 씬 분포 | Freestyle 26 · OpenMatch 26 · ResultFreestyle 27 · ResultOpen3 27 · ResultOpen2 3 |
| 아틀라스 모드 | 전 파일이 풀 프레임 판독과 일치 (`Atlas=OK` 109/109) |
| 결정성 | 같은 입력으로 2회 실행, 결과 파일 바이트 단위 동일 |

- 케이스별 결과(씬/곡 ID/곡명/모드/난이도/Rate·판정 정보/아틀라스 일치): [2026-11-19-modularization-baseline-recognition.txt](2026-11-19-modularization-baseline-recognition.txt)
- 계획 문서와 TASKS의 "117개 테스트셋"은 과거 수치다. 현재 `scratch/` 폴더 구성으로는 109건이 판독된다 (`_mcbadge_`/`cropped_`/`debug_` 파일은 도구가 제외). 단계 7은 이 109건을 기준으로 한다.

### 입력 고정

판독 결과는 `cache/songs.json`·`cache/image_index.db`에 따라 달라지므로 측정 당시 파일을 메인 체크아웃의 `scratch/modularization_baseline/cache/`에 스냅샷으로 보관했다 (git 미포함).

| 파일 | SHA-256 |
| :--- | :--- |
| `songs.json` | `4b5b3e4dd310bc935497cbe651c33493a89a3a5753cb9dd1d71aa8ca4687b710` |
| `image_index.db` | `a2d90dae34de9a78d3ea9569a352cd732df3093b1a8497ac1f5cf2c1fb1285e8` |
| `image_db_version.txt` (`v2026.09.17.0`) | `1f16407b882cb6216109448a93121a4578c09f73324114df369635045d3c82ec` |

테스트 이미지: 메인 체크아웃 `scratch/` 아래 `freestyle_songselect`, `openmatch_songselect`, `freestyle_results`, `openmatch3_results`, `other_screenshots`.

### 재현 절차 (단계 7)

메인 체크아웃 루트에서 (`<worktree>`는 측정할 코드의 체크아웃):

```bash
cargo build --release -p overmax-app --bin verify_pipeline
cd scratch/modularization_baseline
<worktree>/target/release/verify_pipeline.exe ../freestyle_songselect ../openmatch_songselect ../freestyle_results ../openmatch3_results ../other_screenshots
diff --strip-trailing-cr <worktree>/docs/plans/2026-11-19-modularization-baseline-recognition.txt scratch/verify_result.txt
```

- 작업 디렉터리를 스냅샷 폴더로 두어 고정된 `cache/`를 읽고, 결과는 그 아래 `scratch/verify_result.txt`에 쓴다.
- 합격 기준: `diff` 출력 없음. 단계 1~6에서 `verify_pipeline`의 import 경로가 바뀌어도 출력 형식은 유지한다.

---

## 2. 테스트·정적 검사 기준선

### `cargo test --workspace` — 통과 (exit 0)

| 대상 | passed | ignored |
| :--- | ---: | ---: |
| `overmax_app` (lib) | 21 | 0 |
| `overmax_core` (lib) | 3 | 0 |
| `overmax_core` `tests/game_state_fixture.rs` | 1 | 0 |
| `overmax_data` (lib) | 106 | 0 |
| `overmax_engine` (lib) | 77 | 1 |
| `overmax_engine` `tests/hdr_replay_test.rs` | 14 | 1 |
| `overmax_runtime` (lib) | 16 | 0 |
| `overmax_runtime` `tests/ipc_server_integration.rs` | 1 | 0 |
| `overmax_cv` (lib) | 10 | 0 |
| **합계** | **249** | **2** |

- 실패 0. ignored 2건 중 하나는 `test_analyze_all_hdr_snapshots` (사유: 43슬롯 아틀라스 덤프가 현재 47슬롯 패킹과 맞지 않음).
- 테스트는 crate 이동에 따라 위치가 바뀔 수 있다. 단계 7은 합계 249 이상 통과·실패 0·ignored 사유 동일을 기준으로 한다.

### `cargo clippy --all-targets` — 경고 0 (exit 0)

참고: `cargo build --release`에서는 rustc 경고 1건이 있다 — `rust/overmax_engine/src/detector/templates/matching.rs:307` 미사용 변수 `cand`. 기준선이므로 이 단계에서는 고치지 않는다.

---

## 3. 성능 기준선

### 3.1 오프라인 합성 벤치 (`measure_breakdown`, 게임 불필요)

`cargo run --release -p overmax-app --bin measure_breakdown`. 실행마다 수십 µs 단위로 흔들리므로 참고값으로만 쓴다.

| 항목 | 평균 | P95 | 최대 |
| :--- | ---: | ---: | ---: |
| D3D11 1080p staging 전송 | 0.81 ms | 1.02 ms | 1.54 ms |
| D3D11 512×512 아틀라스 staging 전송 | 0.18 ms | 0.26 ms | 0.49 ms |
| D3D11 640×360 staging 전송 | 0.14 ms | 0.22 ms | 0.31 ms |
| 자켓 해시 (ahash+dhash+phash) | 712.0 µs | – | – |
| 자켓 매칭 파이프라인 전체 | 862.3 µs | – | – |

### 3.2 인게임 실측

앱 텔레메트리(`cache/telemetry.log`의 `[TelemetryFlow]` 줄, 약 5초 주기)는 캡처 시간을 `time=평균/P95/최대`로 기록한다. P50은 기록하지 않으므로 기준선은 **평균·P95**로 잡는다. 과거 참고값(TASKS 아카이브 4.6): DXGI 캡처 P50 0.62ms · P95 0.80ms.

측정 절차:

1. 이 브랜치에서 `cargo build --release -p overmax-app` (측정 대상 = `target/release/overmax-rs.exe`).
2. 게임(1080p, 평소 설정) 실행 후 `overmax-rs.exe` 실행. 시작 전 `cache/telemetry.log`가 새 세션으로 시작되는지 확인.
3. 다음 순서로 플레이: 선곡 화면에서 곡 이동 약 2분 → 프리스타일 1곡 플레이 → 결과 화면 → 선곡 복귀.
4. 플레이와 동시에 PowerShell에서 CPU 사용률을 2분간 1초 간격으로 수집:

   ```powershell
   typeperf "\Process(overmax-rs)\% Processor Time" -si 1 -sc 120 -f CSV -o scratch/modularization_baseline/cpu_baseline.csv
   ```

   (`% Processor Time`은 논리 코어 1개 = 100% 기준. 비교 시 같은 PC에서 같은 방식으로 잰다.)
5. 종료 후 `cache/telemetry.log`를 `scratch/modularization_baseline/telemetry_baseline.log`로 복사.

#### (1) release 빌드 (#44 수정 후) — 주 기준선

- 측정: 2026-11-19 19:29:53~19:31:26, `main` `7f5df1e`(#44 수정)를 `cargo build --release -p overmax-app --features telemetry`로 빌드 (텔레메트리 헤더 `build=release`). 선곡 화면에서 곡을 옮겨 다니며 측정.
- 원본: 메인 체크아웃 `scratch/modularization_baseline/cpu_after_fix.csv`, `telemetry_after_fix.log` (git 미포함).
- CPU는 typeperf `% Processor Time` (논리 코어 1개 = 100%), 1초 간격, 빈 샘플 제외. 캡처 평균은 5초 창별 평균을 성공 횟수로 가중 평균, P95는 창별 P95의 중앙값 (최댓값). 시작 직후 첫 창(초기화, 캡처 19회 · 평균 10.5ms)은 제외했다.

| 항목 (release, 수정 후) | 선곡 화면 | 인게임 플레이 | 결과 화면 |
| :--- | :--- | :--- | :--- |
| CPU 평균 / 최대 | **6.1% / 17.0%** (n=39) | 재측정 안 함 | **미측정** |
| 캡처 시간 평균 | 0.56 ms | 재측정 안 함 | 미측정 |
| 캡처 P95 (창별 중앙값 / 최댓값) | 0.88 / 0.95 ms | 재측정 안 함 | 미측정 |
| 캡처 최대 | 2.07 ms | 재측정 안 함 | 미측정 |
| 캡처 성공/시도 | 270/270 (5초당 약 42회) | 재측정 안 함 | 미측정 |

- **인게임 플레이는 수정 뒤 다시 재지 않았다.** 수정 전 release CPU가 0.3%였고, #44 수정은 오버레이가 숨겨지는 플레이 구간 경로를 건드리지 않는다. 기준값은 수정 전 0.3% / 3.1%를 그대로 쓴다.
- **결과 화면은 수정 뒤 측정하지 않았다.** 단계 7에서 같은 절차로 한 번 재서 기록하고, 이후 비교 기준으로 삼는다.

수정 전 (#44) 기록 — 2026-11-19 18:39~18:42, `a3f16c3` 일반 release 빌드. 구간은 CPU 패턴으로 추정했고, 원본 CSV는 이후 진단 실행으로 덮어써져 남아 있지 않다:

| 구간 (추정) | CPU 평균 | CPU 최대 |
| :--- | ---: | ---: |
| 선곡 화면 | 87.4% | 104.3% |
| 인게임 플레이 | 0.3% | 3.1% |
| 결과 화면 | 91.1% | 101.2% |
| 세션 전체 | 18.9% | 104.3% |

#### 해결된 문제: 선곡·결과 화면의 높은 release CPU ([orphera/overmax#44](https://github.com/orphera/overmax/issues/44), closed)

- 원인: 오버레이가 매 프레임 `ViewportCommand::InnerSize`를 보내 `SetWindowPos`가 반복 호출됐고, NVIDIA OpenGL 드라이버가 이를 처리하느라 계속 돌았다 (프로파일상 UI 스레드 CPU의 93.5%가 `nvoglv64.dll`).
- 수정: `7f5df1e` — 창 크기가 다를 때만 `InnerSize`를 보낸다. 선곡 화면 CPU 평균 87.4% → 6.1%.
- 감지 워커는 원래 원인이 아니었다 (수정 전에도 선곡 화면에서 코어 1개의 1% 미만).
- 단계 7은 위 수정 후 숫자와 비교한다.

#### (2) debug 실행 — 캡처 시간·CPU 참고값

- 측정: 2026-11-19 18:22~18:28, 같은 코드의 **debug 빌드** (텔레메트리 헤더 `build=debug`). 원본: `scratch/modularization_baseline/telemetry.log`, `cpu_baseline_debug.csv`. 집계 스크립트: `scratch/mb_parse.py`.
- 구간은 텔레메트리 씬 전환으로 나눴다: 선곡 18:23:43~18:25:30 · 플레이 ~18:27:40 · 결과 화면 18:27:40~로그 끝 (선곡 복귀 포함).
- 캡처 평균은 5초 창별 평균을 성공 횟수로 가중 평균, P95는 5초 창별 P95의 중앙값 (최댓값). 원 샘플이 로그에 없어 구간 전체 P95는 직접 구할 수 없다.

| 항목 (debug) | 선곡 화면 | 인게임 플레이 | 결과 화면 |
| :--- | :--- | :--- | :--- |
| 캡처 시간 평균 | 0.66 ms | 0.59 ms | 0.58 ms |
| 캡처 P95 (창별 중앙값 / 최댓값) | 0.99 / 2.20 ms | 0.83 / 3.03 ms | 1.05 / 1.11 ms |
| 캡처 최대 | 23.5 ms | 3.03 ms | 3.60 ms |
| 캡처 성공/시도 (`capture=`) | 804/804 | 130/130 | 97/97 |
| CPU 평균 / 최대 (%) | 28.8 / 79.7 (n=107) | 0.3 / 3.1 (n=128) | 21.6 / 46.3 (n=19) |
| 체감 프레임 드랍 | 거의 없음 (사용자 체감, debug 실행) | 거의 없음 (사용자 체감, debug 실행) | 거의 없음 (사용자 체감, debug 실행) |

세션 전체 CPU (debug): 평균 13.9% · 최대 79.7% (255샘플, 약 257초).

#### 단계 7 합격 기준

1. **CPU**: 같은 PC에서 위 절차로 release 빌드를 재고, 선곡 화면 CPU 평균이 (1) 표(수정 후 6.1%) 대비 10% 이내. 인게임 플레이는 0.3% 수준 유지. 결과 화면은 이때 처음 기록한다.
2. **캡처 시간**: 같은 절차로 release + `--features telemetry` 빌드를 재고, (1) 표의 캡처 평균·P95와 비교 (10% 이내를 동등으로 본다). (2) debug 수치는 참고용.
3. **체감**: 사용자가 플레이하며 끊김이 기준선(거의 없음)보다 나빠지지 않았음을 확인.

---

## 4. Windows 수동 확인 체크리스트

단계 게이트마다 릴리스 빌드로 확인한다. 모든 항목은 "기준선과 동일하게 동작"이 합격.

단계 7 결과 (2026-11-20, 사용자 확인): `[x]` 확인함 · `[~]` 당분간 확인할 수 없어 보류 (오픈매치 2·3인 결과, 곡 DB·이미지 인덱스 동기화의 버전 갱신, 업데이트 확인).

**오버레이·감지**
- [x] 앱 시작 시 게임 창을 찾아 오버레이가 표시된다 (창 이동/포커스 전환 시 따라감)
- [x] 선곡 화면에서 곡·모드·난이도 전환이 즉시 반영된다 (프리스타일 / 오픈매치)
- [x] 추천 목록(floor 기반 정렬)이 이전과 같은 순서로 표시된다
- [x] 인게임 플레이 중 오버레이가 숨겨지고 체감 끊김이 없다
- [x] 게임 최소화·종료 후 재실행 시 다시 붙는다

**표시 모드**
- [x] 일반 모드 ↔ 라이트 모드 전환, 헤더 버튼 동작, 라이트 헤더 수동 드래그
- [x] 창 위치·크기가 재시작 후 유지된다

**결과 화면·기록**
- [x] 프리스타일 결과 화면에서 Rate·MAX COMBO가 기록 DB(`cache/record.db`)에 저장된다
- [~] 오픈매치 2인/3인 결과 화면이 각각 올바르게 구분·기록된다
- [x] 미플레이 패턴과 판독 실패가 구분되어 표시된다

**업로드·동기화**
- [x] V-Archive 기록 업로드가 성공한다 (계정 설정된 경우)
- [~] 곡 DB·이미지 인덱스 동기화가 정상 완료되고 버전이 갱신된다
- [~] 업데이트 확인(self_update)이 오류 없이 동작한다

**설정**
- [x] 설정 창 열기/저장, 언어 전환(i18n) 반영
- [x] 기존 `settings.user.json`이 그대로 읽히고, 저장 시 기본값과 다른 항목만 기록된다
- [x] 기존 `cache/record.db`가 마이그레이션 없이 열린다
