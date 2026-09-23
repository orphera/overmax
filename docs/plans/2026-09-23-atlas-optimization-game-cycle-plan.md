# 아틀라스 최적화 및 Gameplay 연동 작업 계획

## 1. 개요
현재 `feat/game-cycle`에서 추가된 gameplay/pause scene 인식 로직이 1080p 전체 프레임을 매번 캡처하여 성능 병목을 유발하고 있음. 이를 해결하기 위해 기존 GPU ROI Atlas 파이프라인에 Gameplay 전용 ROI를 패킹하고 효율적으로 인식하도록 구조를 개선함.

## 2. 작업 단계 (Roadmap)

### [x] Step 1: 아틀라스 슬롯 다이어트 및 Gameplay ROI 추가
- `ResultFreestyle/mode` (340x75) 슬롯 제거 (여유 공간 확보: 25,500 px)
- 자켓 통합: `ResultFreestyle/jacket`, `ResultOpen3/jacket`, `ResultOpen2/jacket`을 단일 슬롯으로 통합 (여유 공간 확보: 7,200 px)
- 확보된 공간(총 32,700 px)에 Gameplay 관련 ROI 7개 신규 패킹
- `rust/overmax_engine/src/detector/atlas_layout.rs` 수정

### [x] Step 2: `AtlasTranslator` 매핑 갱신
- `rust/overmax_engine/src/detector/atlas_translator.rs`에 신규 추가된 7개 ROI에 대한 매핑 추가

### [x] Step 3: `GameplaySceneReader` 리팩토링
- `rust/overmax_engine/src/detector/gameplay_scene.rs`에서 1080p 프레임 의존성을 제거
- 아틀라스 기반의 `ImageView` 호출 방식으로 전환 (GPU 아틀라스 우선, 레거시 1080p 폴백 유지)

### [x] Step 3.5: Gameplay/Paused와 정적 씬 감지 경계 정리
- `detect_scene_if_due`에서 Gameplay/Paused 관측을 정적 씬 파싱보다 먼저 수행
- 인게임 씬이 확인되면 자켓 매칭을 실행하지 않고 즉시 공통 scene commitment 경로로 전달
- `last_static_scene` 명칭을 실제 역할에 맞는 `last_scene`으로 변경

### [ ] Step 4: 캡처 파이프라인 연동 검증
- DXGI 캡처 엔진의 `copy_slots_to_atlas` 로직에 신규 7개 ROI 복사 로직 추가
- Windows 환경에서 성능 및 인식 정확도 최종 검증

## 3. 예상 결과
- 메모리 대역폭 절감 및 GPU 스톨 제거
- 캡처 지연 0.6ms 수준 복구
- 파편화된 인식 로직 통합
