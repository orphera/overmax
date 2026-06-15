# TASKS

Overmax의 차기 마일스톤(v0.3.0)을 위한 작업 목록 및 백로그입니다.

---

## 1. pattern_meta 키(Key) 마이그레이션 및 유사 곡 매칭

- [x] `songs.json` 데이터베이스 내에서 특정 패턴 정보와 가장 유사한 곡을 탐색하는 검색 알고리즘 신설 (`overmax_data`)
  - [x] 곡명 매칭(Fuzzy / Exact) 및 버튼, 난이도 속성 등 패턴의 상세 스펙 대조 알고리즘 설계
- [x] `pattern_meta.json` 로딩 시 기존 `mode|title|diff` 형식의 Key를 중복 문제가 없는 `song_id|mode|diff` 형식으로 전환 및 마이그레이션
  - [x] 타이틀이 동일한 중복 곡들의 세부 데이터 비교를 통한 오배치 방지 예외 처리
- [x] 마이그레이션 전후 곡 정보 매칭 정확도 검증용 골든 테스트 작성

## 2. Rust 데이터 모델 메모리 레이아웃 및 힙 할당 최적화

- [x] `PatternSheetMetaItem` 구조체의 String 필드 축소 및 Enum 변환을 통한 메모리 압축
  - [x] `gold` (`GoldMeta`) 및 `assist_key` (`AssistMeta`)를 Enum 타입으로 전환하여 스택 1바이트로 최적화
  - [x] `pattern_meta.json` 역직렬화 및 로직 전반에 Enum 적용
- [x] V-Archive `Song` 및 패턴 정보 구조체의 HashMap 제거 및 배열 최적화
  - [x] `Song.patterns` 구조를 `HashMap<String, HashMap<String, PatternInfo>>`에서 고정 크기 중첩 배열 `[[Option<PatternInfo>; 4]; 4]`로 전환하여 수천 개의 HashMap 힙 오버헤드 해제
  - [x] `Mode` 및 `Difficulty`를 표현하는 전용 Enum 타입 도입 및 연동
- [x] 중복 문자열 풀링 또는 `Arc<str>` 도입
  - [x] `composer`, `dlc_code` 등 여러 곡에서 중복해서 생성되는 문자열에 `Arc<str>`을 적용하여 힙 메모리 중복 할당 방지
- [x] 리팩토링 후 전체 유닛 테스트(`cargo test`) 및 오버레이 작동 검증

## 3. 라이트모드 (Lite Mode) 및 자석 스냅 구현

- [x] `settings.user.json` 구조에 `lite_mode` 토글 옵션 추가 및 기본값 정의
- [x] 설정 UI(`settings_ui.rs`) 내 라이트모드 켜기/끄기 체크박스 컴포넌트 추가
- [x] 라이트모드 켜짐 상태일 때 오버레이 UI 레이아웃 동적 분기 (`overlay_ui.rs`)
  - [x] 추천곡 목록 리스트 렌더링 생략
  - [x] 선택된 곡의 비공식 난이도 및 선택한 패턴의 메타 정보(BPM, 레벨, 노트수 등) 레이아웃 재배치 및 집중 노출
- [x] 래더 매칭 등 선곡 정보 파악이 긴급한 씬 상황에서 오버레이 정보 가독성 확인
- [x] 오버레이 스냅(고정 위치) 기능과 라이트 모드를 완전 직교 분리 및 '수동(manual)' 모드 드래그 가능성 보장
- [x] 설정 UI 상에 280x120 크기의 가상 모니터 레이아웃을 구현하고, 모퉁이와 중앙(수동)에 직관적으로 버튼 매핑

## 4. V-Archive 플레이 기록 갱신 알림 기능

- [x] 실시간 감지된 Rate가 V-Archive 로컬 캐시 또는 서버 내 기존 최고 기록보다 높을 시 갱신 판정 로직 추가
- [x] 게임 실행을 방해하지 않는 간결한 알림 UI (오버레이 헤더 내 ⬆ 단독 업로드 버튼 및 램프 기능) 구현
- [x] 설정 및 V-Archive 계정 연동 여부(account.txt 실존 여부)에 따른 버튼 활성화/비활성화 및 툴팁 가이드 추가

## 5. 감지 가능 씬(Scene) 다양화

- [ ] FREESTYLE 및 ONLINE 대기방 외에 래더 매칭(Ladder), 결과 화면(Result) 등 신규 Scene 탐색 및 정의
- [ ] `SceneType` enum 확장 및 각 씬에 해당하는 OCR 인식 키워드 추가
- [ ] `scene_config.rs` 및 `RoiManager`에 씬별 동적 ROI 좌표 매핑 테이블 정의
- [ ] 씬 전환 단계에서의 `HysteresisBuffer` 상태 전이 안정성 보완

## 6. 전체화면 (Fullscreen) 호환성 검증

- [x] DJMAX RESPECT V 전체화면(Fullscreen) 모드 구동 시 Win32 창 트래킹 및 GDI 화면 캡처 신뢰성 검증
- [x] winit 기반 투명 오버레이 창이 전체화면 게임 위에 올바르게 오버레이되는지(Z-order 및 포커스 뺏김 현상 등) 확인
- [x] 전체화면 실행 중 오버레이 인터랙션 시 게임 창 복원 로직 예외 처리 보강

## 7. OBS 방송 송출용 화면 모드 (OBS Mode)

- [ ] 스트리머/방송 송출을 위한 OBS 전용 캡처 모드 설계
- [ ] 설정 UI에 크로마키(Green Screen 등) 단색 배경 토글 또는 방송 전용 레이아웃 스킨 옵션 추가
- [ ] 창 캡처 시 불필요한 영역 노출 방지 및 고정 레이아웃 바인딩 처리

## 8. V-Archive 클라이언트 완전 대체 (장기 목표)

- [ ] [장기] 공식 V-Archive 클라이언트를 보완/대체하기 위한 백그라운드 기록 자동 업로드 파이프라인 설계
- [ ] [장기] Steam 세션 감지 및 로컬 갱신 데이터를 API를 통해 V-Archive로 안전하게 즉각 백업 업로드하는 모듈 구현

## 9. HOG 피처 데이터베이스 갱신 및 재빌드 (진행 중)

- [ ] `orphera/overmax-image-db` 리포지토리 재작업을 통한 전체 자켓 이미지의 HOG 피처 일괄 갱신 및 재빌드
  - [x] 이미지 피처 연산 SSOT를 `overmax_cv`로 이전하기 위한 Rust CLI (`db-builder`) 및 파이썬 연동 설계 완료 ([image_db_redesign_plan.md](docs/2026-06-15-image_db_redesign_plan.md))
  - [ ] `overmax_data` 내에 `db-builder` CLI 바이너리 타겟 구현
  - [ ] `overmax-image-db` 저장소 내 `build_image_db.py`가 임시 이미지 저장 후 Rust CLI를 호출하도록 수정
  - [ ] GitHub Actions에 `cargo install --git` 방식 및 액션 캐싱(`actions/cache`)을 통한 자동화 워크플로우 적용
  - [ ] 특정 자켓(Fundamental, ID 388 등)의 로컬 이미지 왜곡으로 인한 HOG 유사도 저하 근본 해결
  - [ ] Rust 버전 정식 배포 전까지 임시로 낮춘 `settings.json`의 자켓 매칭 임계치(`0.75`)를 피처 갱신 완료 후 기존 값(`0.85`)으로 원복

## 10. 오버레이 반응성 및 사용성 개선 (백로그)

- [x] 프리스타일 화면 이탈 시 오버레이 창 닫힘 반응 속도 개선 (연주 시작 시 즉시 닫히지 않고 한참 남아있는 현상 완화)
- [x] 시스템 트레이 영역에서 설정창 등 보조 뷰포트를 호출할 때 런타임 갱신 주기 지연 현상 개선 (바로 뜨지 않고 대기하는 문제)
- [x] 라이트모드 최초 기동/진입 시, 특정 기본 좌표(예: 오른쪽 중앙)에서 깜빡거린 뒤 목표 구석 위치로 이동하는 초기 렌더링 Jitter 현상 제거
- [x] 자가 업데이트 후 자동 재시작 시 단일 인스턴스 락(Named Mutex) 해제 지연으로 새 프로세스가 조기 종료되던 치명적인 이슈를 프로세스 spawn 직전 가드 drop()을 수행하도록 수정하여 완전 해결
- [x] 오버레이 창에 마우스 호버 시 일시적으로 불투명해졌다가 다시 투명해지는 오작동 수정 (v0.2.3 topmost 윈도우 스타일 검증 캐시 버그 수정 및 egui native StartDrag 연동을 통해 완전 해결)
- [x] topmost 윈도우 스타일 검증 캐시 버그 수정으로 비활성 시 SetWindowPos 스팸 호출 차단 및 z-order 꼬임 깜빡임 해결
- [x] game_rect 락 try_lock 교체 및 Snap 기하 캐싱을 통해 락 경합 및 SetWindowPos 호출 횟수 최적화 (매 프레임 -> 0회)
- [x] 보조창 종료 순서 안전화 및 트레이 메뉴 클릭 시 켜져 있는 보조창 Focus Bring to Front 구현
- [x] 마우스 오버 감지 target_os 조건부 컴파일 가드 적용으로 크로스 플랫폼 빌드 이식성 확보 (v0.2.3)
- [x] 오버레이 창 위에 마우스 진입 시(passthrough 해제 시) 마우스 커서가 사라지던 소실 버그를 십자선 모양(Crosshair)의 소프트웨어 커서 직접 렌더링을 통해 해결 (v0.2.3)




