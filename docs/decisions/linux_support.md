# Linux Support Decision Log

Linux 플랫폼 지원(Wayland/XWayland, wlr-layer-shell, XComposite/MIT-SHM 캡처, Vulkan 렌더링, 패키징 및 배포) 관련 주요 설계 결정의 배경과 이유를 기록한다.

---

## 📋 Decision History

| 날짜 | 결정 | 이유 | 참조 |
|:---|:---|:---|:---|
| 2026-07-17 | 초기 Linux 지원 범위와 additive 변경 원칙 확정 | Windows 동작과 공용 인식 파이프라인을 유지하면서 검증 가능한 Proton/XWayland 환경만 지원하기 위함 | [docs/linux-support.md](../guides/linux-support.md) |
| 2026-07-17 | Linux 핵심 실행 경로 연결 | exact-title window snapshot, XComposite/MIT-SHM 캡처, 기존 디텍션 파이프라인과 native layer overlay를 fail-closed 경계로 연결 | [detection_worker.rs](../../rust/overmax_engine/src/detector/detection_worker.rs) / [linux_layer_overlay.rs](../../rust/overmax_app/src/ui/linux_layer_overlay.rs) |
| 2026-07-17 | Xvfb+Openbox lifecycle 게이트 추가 | window 추적, BGRA 캡처, resize·remap·recreate와 extension 부재 경로를 hosted CI에서 재현하기 위함 | [linux-vertical-slice-lifecycle.sh](../../.github/scripts/linux-vertical-slice-lifecycle.sh) |
| 2026-07-18 | named pixmap을 매 캡처마다 재획득 | XWayland가 map/resize 이벤트 없이 backing pixmap을 교체할 때 이전 handle이 frozen frame을 반환하는 문제를 방지 | [linux.rs](../../rust/overmax_engine/src/capture/capture_engine/linux.rs) |
| 2026-07-31 | x86_64 tarball과 glibc 2.39 ABI 기준 확정 | 실행 권한과 실행 디렉터리 기준 설정·캐시 계약을 유지하면서 빌드 호스트의 최신 glibc가 배포물에 유입되는 것을 방지 | [package-linux.sh](../../scripts/package-linux.sh) / [ci.yml](../../.github/workflows/ci.yml) |
| 2026-08-03 | Linux 단일 인스턴스 파일 락 추가 | 중복 캡처·overlay와 설정 및 SQLite 캐시 갱신 경합을 방지 | [linux.rs](../../rust/overmax_app/src/system/single_instance/linux.rs) |
| 2026-08-03 | layer overlay 지연 repaint 예약 | egui의 `request_repaint_after` 요청을 poll timeout으로 연결해 외부 Wayland 이벤트가 없어도 tooltip 등 지연 UI를 갱신 | [linux_layer_overlay.rs](../../rust/overmax_app/src/ui/linux_layer_overlay.rs) |
| 2026-08-03 | 캡처 오류 복구 정책 분리 | transport 오류만 재연결하고 영구 capability 오류의 반복 probe·로그·repaint를 중단해 성능 우선 제약을 유지 | [capture_engine.rs](../../rust/overmax_engine/src/capture/capture_engine.rs) / [detection_worker.rs](../../rust/overmax_engine/src/detector/detection_worker.rs) |
| 2026-08-04 | Linux 앱 자동 업데이트 연결 | 기존 `self_update`와 공용 락 해제·재시작 흐름을 재사용하고 Linux tarball에서 실행 파일만 원자적으로 교체해 설정·캐시 호환성을 유지 | [linux.rs](../../rust/overmax_app/src/system/updater/linux.rs) / [package-linux.sh](../../scripts/package-linux.sh) |
| 2026-08-04 | Linux 앱 메뉴 바로가기 및 Steam 동시 실행 안내 추가 | 실행 디렉터리 기반 설정·캐시 계약을 유지하면서 터미널 없는 실행과 게임 동시 시작을 지원 | [desktop_entry_linux.rs](../../rust/overmax_app/src/system/desktop_entry_linux.rs) / [docs/linux-support.md](../guides/linux-support.md) |
| 2026-08-11 | Linux 배포 기준을 Ubuntu 22.04/glibc 2.35로 하향 | Ubuntu Base 22.04.5에서 실제 release tarball 생성·설치·업데이트와 max GLIBC_2.35를 확인하고, 동일한 smoke gate로 우발적인 ABI 상승을 차단 | [package-linux.sh](../../scripts/package-linux.sh) / [smoke-linux-bundle.sh](../../scripts/smoke-linux-bundle.sh) / [ci.yml](../../.github/workflows/ci.yml) |
| 2026-08-11 | Flatpak Steam 경로 추가 | Flatpak 설치의 `loginusers.vdf`를 native 경로 다음 fallback으로 탐색해 기존 계정 연동 흐름을 그대로 재사용 | [linux.rs](../../rust/overmax_app/src/system/steam_session/linux.rs) |
| 2026-08-11 | Linux overlay 다중 출력 및 fractional scale 지원 | 게임 창과 겹치는 output을 직접 지정하고 좌표 원점을 변환하며, niri eDP-1에서 preferred scale 1.25 수신과 실제 렌더 버퍼 적용을 확인 | [linux_layer_overlay.rs](../../rust/overmax_app/src/ui/linux_layer_overlay.rs) / [docs/linux-support.md](../guides/linux-support.md) |
| 2026-08-11 | X11 overlay fallback 구현 보류 | 구현은 가능하지만 eframe event loop 생성 전 layer-shell probe와 별도 X11 root-overlay 수명 관리가 필요하며 현재 지원 대상에는 layer-shell 세션만 포함 | [docs/linux-support.md](../guides/linux-support.md#x11-overlay-fallback-검증-결과) |
| 2026-10-28 | 창 geometry를 하위 crate trait 경계로 분리하고 오버레이는 좌표 입구만 유지 (계획) | 이슈 #35처럼 컴포지터별 창 위치 소스(niri IPC, KWin 등)를 기여자가 app을 건드리지 않고 붙일 수 있도록, 창 추적은 하위 crate의 trait 구현으로 두고 `linux_layer_overlay.rs`는 외부 좌표를 받기만 한다. 위치 정보가 없으면 현재 수동 배치를 유지(fail-closed)하고 Windows 구현은 경계만 맞춘다 | [TASKS.md](../../TASKS.md) 11.2·12 / [orphera/overmax#35](https://github.com/orphera/overmax/issues/35) |
| 2026-10-28 | 창 geometry 경계를 컴포지터별 위치 소스(`GameGeometrySource`)로 한정 | Wayland는 다른 창의 전역 좌표를 주지 않고(layer-shell은 출력 고정, foreign-toplevel은 좌표 없음, XWayland 좌표는 설정 의존) 표준 경로가 없으므로 컴포지터 IPC 어댑터만 확장 지점으로 둔다. XWayland 좌표 휴리스틱·foreign-toplevel 이동·`WindowTracker` 통합은 기각 | [2026-10-28-linux-window-geometry-sources.md](../plans/2026-10-28-linux-window-geometry-sources.md) |
| 2026-10-30 | Hyprland·Sway geometry source를 세션 환경변수로 자동 선택 | 지원 compositor의 창 위치를 별도 설정 없이 반영하되 두 감지 변수가 동시에 있거나 어느 쪽도 없으면 수동 배치로 fail-closed한다. 파서 fixture는 통과했으며 실제 compositor 검증은 남아 있다 | [geometry_source.rs](../../rust/overmax_engine/src/capture/window_tracker/geometry_source.rs) / [2026-10-28-linux-window-geometry-sources.md](../plans/2026-10-28-linux-window-geometry-sources.md) |
