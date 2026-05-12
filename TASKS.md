# TASKS

Overmax의 현재 작업은 Python 기반 verified pipeline을 유지하면서,
Win32 직접 오버레이 및 보조 창의 프로덕션 안정성을 확보하고, UI 인프라를 표준화하는 것이다.

## 아카이브 (Archived Decisions)

상세 내역은 `docs/decisions/` 디렉토리의 문서를 참조한다.

- [OpenCV 제거 및 Rust HOG 검증](./docs/decisions/opencv-removal.md)
- [Qt 런타임 및 UI 구조 정리](./docs/decisions/qt-cleanup.md)
- [Win32 네이티브 전환 및 보조 창 이관](./docs/decisions/win32-transition.md)

---

## 현재 진행 중: Win32 Infra Consolidation (Phase 13)

Win32 GUI 인프라를 중앙 집중화하고 표준화한다. 프로젝트 중립적인 로직을 `infra/gui`로 이동한다.

- [ ] `infra/gui/controls.py` 신규 생성: 공용 컨트롤 생성 헬퍼 이동
- [ ] `infra/gui/layout.py` 신규 생성: `LayoutContext`, `LayoutPadding` 이동
- [ ] `infra/gui/dpi.py` 보강: `scale_for_dpi`, `scaled_value` 이동
- [ ] `infra/gui/placement.py` 보강: `center_position` 이동
- [ ] `infra/gui/theme.py` 보강: `create_font` 이동 및 기본 색상 토큰 추가
- [ ] `overlay/win32/settings_common.py` 제거 및 관련 창 임포트 갱신
- [ ] 모든 Win32 smoke 테스트 통과 확인 (Overlay, Settings, Sync, Debug, Status)

---

## 제약 (Constraints)

- **기존 verified pipeline은 변경하지 않는다.**
- 선곡 화면 전용 로직은 정확도를 우선하되, 인게임 성능 영향은 피한다.
- Rust backend는 검증 스크립트에서 충분히 확인된 뒤 프로덕션 검색 경로에 연결한다.
- 메모리 접근/인젝션 없이 화면 캡처 기반을 유지한다.

## 검증 기준 (Verification Criteria)

- [ ] 모든 Win32 보조 창이 `overlay.main_backend=win32` 설정 시 정상 작동한다.
- [ ] `test/win32_*_smoke.py` 모든 테스트 통과.
- [ ] `maturin develop` 기반 Rust 확장 모듈 연동 및 정확도 유지 (mean > 0.99).
- [ ] PyInstaller 빌드 후 배포 크기 및 런타임 임포트 속도 확인.
