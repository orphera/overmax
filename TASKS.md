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

- [ ] `PatternSheetMetaItem` 구조체의 String 필드 축소 및 Enum 변환을 통한 메모리 압축
  - [ ] `gold` (`GoldMeta`) 및 `assist_key` (`AssistMeta`)를 Enum 타입으로 전환하여 스택 1바이트로 최적화
  - [ ] `pattern_meta.json` 역직렬화 및 로직 전반에 Enum 적용
- [ ] V-Archive `Song` 및 패턴 정보 구조체의 HashMap 제거 및 배열 최적화
  - [ ] `Song.patterns` 구조를 `HashMap<String, HashMap<String, PatternInfo>>`에서 고정 크기 중첩 배열 `[[Option<PatternInfo>; 4]; 4]`로 전환하여 수천 개의 HashMap 힙 오버헤드 해제
  - [ ] `Mode` 및 `Difficulty`를 표현하는 전용 Enum 타입 도입 및 연동
- [ ] 중복 문자열 풀링 또는 `Arc<str>` 도입
  - [ ] `composer`, `dlc_code` 등 여러 곡에서 중복해서 생성되는 문자열에 `Arc<str>`을 적용하여 힙 메모리 중복 할당 방지
- [ ] 리팩토링 후 전체 유닛 테스트(`cargo test`) 및 오버레이 작동 검증

## 3. 라이트모드 (Lite Mode) 구현

- [ ] `settings.user.json` 구조에 `lite_mode` 토글 옵션 추가 및 기본값 정의
- [ ] 설정 UI(`settings_ui.rs`) 내 라이트모드 켜기/끄기 체크박스 컴포넌트 추가
- [ ] 라이트모드 켜짐 상태일 때 오버레이 UI 레이아웃 동적 분기 (`overlay_ui.rs`)
  - [ ] 추천곡 목록 리스트 렌더링 생략
  - [ ] 선택된 곡의 비공식 난이도 및 선택한 패턴의 메타 정보(BPM, 레벨, 노트수 등) 레이아웃 재배치 및 집중 노출
- [ ] 래더 매칭 등 선곡 정보 파악이 긴급한 씬 상황에서 오버레이 정보 가독성 확인

## 4. V-Archive 플레이 기록 갱신 알림 기능

- [ ] 실시간 감지된 Rate가 V-Archive 로컬 캐시 또는 서버 내 기존 최고 기록보다 높을 시 갱신 판정 로직 추가
- [ ] 게임 실행을 방해하지 않는 간결한 알림 UI (Toast / Notification / Fade-in 배너 등) 구현
- [ ] 설정 UI에 알림 토글 옵션 및 알림 지속 시간 커스텀 설정 항목 추가

## 5. 감지 가능 씬(Scene) 다양화

- [ ] FREESTYLE 및 ONLINE 대기방 외에 래더 매칭(Ladder), 결과 화면(Result) 등 신규 Scene 탐색 및 정의
- [ ] `SceneType` enum 확장 및 각 씬에 해당하는 OCR 인식 키워드 추가
- [ ] `scene_config.rs` 및 `RoiManager`에 씬별 동적 ROI 좌표 매핑 테이블 정의
- [ ] 씬 전환 단계에서의 `HysteresisBuffer` 상태 전이 안정성 보완

## 6. 전체화면 (Fullscreen) 호환성 검증

- [ ] DJMAX RESPECT V 전체화면(Fullscreen) 모드 구동 시 Win32 창 트래킹 및 GDI 화면 캡처 신뢰성 검증
- [ ] winit 기반 투명 오버레이 창이 전체화면 게임 위에 올바르게 오버레이되는지(Z-order 및 포커스 뺏김 현상 등) 확인
- [ ] 전체화면 실행 중 오버레이 인터랙션 시 게임 창 복원 로직 예외 처리 보강

## 7. OBS 방송 송출용 화면 모드 (OBS Mode)

- [ ] 스트리머/방송 송출을 위한 OBS 전용 캡처 모드 설계
- [ ] 설정 UI에 크로마키(Green Screen 등) 단색 배경 토글 또는 방송 전용 레이아웃 스킨 옵션 추가
- [ ] 창 캡처 시 불필요한 영역 노출 방지 및 고정 레이아웃 바인딩 처리

## 8. V-Archive 클라이언트 완전 대체 (장기 목표)

- [ ] [장기] 공식 V-Archive 클라이언트를 보완/대체하기 위한 백그라운드 기록 자동 업로드 파이프라인 설계
- [ ] [장기] Steam 세션 감지 및 로컬 갱신 데이터를 API를 통해 V-Archive로 안전하게 즉각 백업 업로드하는 모듈 구현

## 9. HOG 피처 데이터베이스 갱신 및 재빌드

- [ ] `orphera/overmax-image-db` 리포지토리 재작업을 통한 전체 자켓 이미지의 HOG 피처 일괄 갱신 및 재빌드
  - [ ] 특정 자켓(Fundamental, ID 388 등)의 로컬 이미지 왜곡으로 인한 HOG 유사도 저하 근본 해결
  - [ ] Rust 버전 정식 배포 전까지 임시로 낮춘 `settings.json`의 자켓 매칭 임계치(`0.75`)를 피처 갱신 완료 후 기존 값(`0.85`)으로 원복

